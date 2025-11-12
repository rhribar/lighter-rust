use anyhow::Result;
use chrono::Utc;
use cron::Schedule;
use log::{error, info};
use rust_decimal::{
    prelude::{FromPrimitive, ToPrimitive},
    Decimal,
};
use std::str::FromStr;
use tokio::{
    task,
    time::{sleep, Duration},
};

use points_bot_rs::{
    fetchers::{AccountData, Fetcher, MarketInfo, Position},
    operators::{Operator, OrderRequest, OrderResponse, OrderType},
    BotEnvConfig, BotJsonConfig, BotMode, ExchangeName, PointsBotResult, PositionSide,
};
use rust_decimal::RoundingStrategy;

#[derive(Debug, Clone)]
struct ArbitrageOpportunity {
    symbol: String,
    rate_a: Decimal,
    rate_b: Decimal,
    diff: Decimal,
    leverage_a: Decimal,
    leverage_b: Decimal,
    price_crossed: bool,
    bid_opportunity_price: Decimal,
    ask_opportunity_price: Decimal,
    market_a: MarketInfo,
    market_b: MarketInfo,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let env_config = BotEnvConfig::load_env().expect("Failed to load configuration");
    info!(
        "Starting Cluster - Mode: {:?} | Config File Path: {}",
        env_config.mode,
        env_config.config_file_path.as_deref().unwrap_or("None")
    );

    if env_config.mode == BotMode::Production {
        let schedule = Schedule::from_str("0 */10 * * * * *").unwrap();
        // 0 30 0,4,8,12,16,20 * * * *
        // 0 40 0,8,16 * * * *
        // 0 */10 * * * * *

        loop {
            let now = Utc::now();

            if let Some(next) = schedule.upcoming(Utc).next() {
                let duration = (next - now).to_std().unwrap();
                info!("Next trade scheduled at: {:?}", next);

                sleep(duration).await;

                let configs = BotJsonConfig::load_config_file(&env_config).expect("Failed to load config file");

                for config_json in configs {
                    let bot_config = BotJsonConfig::process_config(&config_json).await;

                    task::spawn(async move {
                        trade(
                            bot_config.fetcher_a.as_ref(),
                            bot_config.fetcher_b.as_ref(),
                            bot_config.operator_a.as_ref(),
                            bot_config.operator_b.as_ref(),
                        )
                        .await;
                    });
                }
            }
        }
    } else {
        let config = BotJsonConfig::load_config_file(&env_config)
            .expect("Failed to load config file")
            .into_iter()
            .next();

        info!("Loaded config: {:?}", config);

        let bot_config = BotJsonConfig::process_config(&config.unwrap()).await;

        trade(
            bot_config.fetcher_a.as_ref(),
            bot_config.fetcher_b.as_ref(),
            bot_config.operator_a.as_ref(),
            bot_config.operator_b.as_ref(),
        )
        .await;
        Ok(())
    }
}

async fn trade(fetcher_a: &dyn Fetcher, fetcher_b: &dyn Fetcher, operator_a: &dyn Operator, operator_b: &dyn Operator) {
    let env_config = BotEnvConfig::load_env().expect("Failed to load configuration");
    let (positions_a, positions_b, _, markets_a, markets_b) = get_trading_data(fetcher_a, fetcher_b).await;

    // business logic, decide if rebalance is needed
    let arbitrage_opportunities = get_arbitrage_opportunities_from_markets(&markets_a, &markets_b).await;
    let best_op = get_best_op(&arbitrage_opportunities).await;

    let op = best_op.unwrap();
    let symbol = &op.symbol;
    let diff = op.diff;

    let prev_symbol = positions_a.first().map(|p| &p.symbol);
    let prev_best_diff = prev_symbol
        .and_then(|s| {
            arbitrage_opportunities
                .iter()
                .find(|op| &op.symbol == s)
                .map(|op| op.diff)
        })
        .unwrap_or(Decimal::ZERO);

    let is_better = diff > prev_best_diff * Decimal::from_f32(1.11).unwrap();
    let is_same_symbol = prev_symbol.map_or(false, |s| symbol == s);

    info!(
        "Decision Making: Prev op symbol: {:?}, Prev op current funding diff: {:?}%, Next op symbol: {:?}, Next op funding diff: {:?}%, Already Holding Same Asset: {}, Is Better (33% threshold): {}",
        prev_symbol,
        prev_best_diff * Decimal::from(24 * 365 * 100),
        symbol,
        diff * Decimal::from(24 * 365 * 100),
        is_same_symbol,
        is_better
    );

    let is_first_time = positions_a.is_empty() && positions_b.is_empty();
    let change_position = !is_first_time && !is_same_symbol && is_better;

    info!(
        "Trade Execution | Is First Time: {} | Change Position: {}",
        is_first_time, change_position
    );

    if change_position {
        close_positions_for_market(&positions_a, &markets_a, operator_a)
            .await
            .unwrap_or_else(|e| {
                error!("Failed to close positions: {:?}", e);
            });

        close_positions_for_market(&positions_b, &markets_b, operator_b)
            .await
            .unwrap_or_else(|e| {
                error!("Failed to close positions: {:?}", e);
            });

        let sleep_time = if env_config.mode == BotMode::Production {
            5 * 60
        } else {
            5 * 60
        };

        info!("Sleeping for {} seconds to allow orders to fill and close", sleep_time);
        info!(
            "Next trade scheduled at: {:?}",
            Utc::now() + Duration::from_secs(sleep_time)
        );

        sleep(Duration::from_secs(sleep_time)).await;
    }

    if is_first_time || change_position {
        let (_, _, min_available_balance, markets_a, markets_b) = get_trading_data(fetcher_a, fetcher_b).await;

        let arbitrage_opportunities = get_arbitrage_opportunities_from_markets(&markets_a, &markets_b).await;
        let best_op = get_best_op(&arbitrage_opportunities).await;

        if let Some(ArbitrageOpportunity {
            symbol: _,
            rate_a,
            rate_b,
            diff: _,
            leverage_a: _,
            leverage_b: _,
            price_crossed: _,
            bid_opportunity_price,
            ask_opportunity_price,
            market_a,
            market_b,
        }) = best_op
        {
            info!(
                "Markets for arbitrage | Market {}: {:?} | Market {}: {:?}",
                operator_a.get_exchange_info(),
                market_a,
                operator_b.get_exchange_info(),
                market_b
            );

            let (amount, long_market, short_market, long_operator, short_operator) = calculate_trade_attributes(
                rate_a,
                rate_b,
                &market_a,
                &market_b,
                operator_a,
                operator_b,
                min_available_balance.unwrap(),
            );

            if let Err(e) = set_same_leverage(&market_a, &market_b, operator_a, operator_b).await {
                error!("Failed to set leverage: {:?}", e);
            }

            let (price_adjusted_long, _) =
                get_adjusted_price_and_side(long_market, &PositionSide::Long, false, long_operator).await;
            let (price_adjusted_short, _) =
                get_adjusted_price_and_side(short_market, &PositionSide::Short, false, short_operator).await;
            info!(
                "Amount: {}, Price Adjusted Short: {} Price Adjusted Long: {}, Bid Opportunity Price: {}, Ask Opportunity Price: {}",
                amount, price_adjusted_short, price_adjusted_long, bid_opportunity_price, ask_opportunity_price
            );

            match create_order(
                short_operator,
                &short_market,
                PositionSide::Short,
                &amount,
                &price_adjusted_short,
                Some(false),
            )
            .await
            {
                Ok(order_result) => info!("Short order: {:?}", order_result),
                Err(e) => error!("Short order failed: {:?}", e),
            }

            match create_order(
                long_operator,
                &long_market,
                PositionSide::Long,
                &amount,
                &price_adjusted_long,
                Some(false),
            )
            .await
            {
                Ok(order_result) => info!("Long order: {:?}", order_result),
                Err(e) => error!("Long order failed: {:?}", e),
            }
        } else {
            error!("No arbitrage opportunities available.");
        }
    }
}

async fn get_trading_data(
    fetcher_a: &dyn Fetcher,
    fetcher_b: &dyn Fetcher,
) -> (
    Vec<Position>,
    Vec<Position>,
    Option<Decimal>,
    Vec<MarketInfo>,
    Vec<MarketInfo>,
) {
    let account_a = get_account_data(fetcher_a).await;
    let account_b = get_account_data(fetcher_b).await;

    let positions_a = get_positions(&account_a);
    let positions_b = get_positions(&account_b);

    let min_available_balance = match (&account_a, &account_b) {
        (Some(a), Some(b)) => Some(a.available_balance.min(b.available_balance)),
        _ => None,
    };

    info!("Maximum balance to trade across exchanges: {:?}", min_available_balance);

    let markets_a = get_markets(fetcher_a).await;
    let markets_b = get_markets(fetcher_b).await;

    (positions_a, positions_b, min_available_balance, markets_a, markets_b)
}

async fn get_account_data(fetcher: &dyn Fetcher) -> Option<AccountData> {
    fetcher.get_account_data().await.map(Some).unwrap_or_else(|e| {
        error!("Failed to get A account: {:?}", e);
        None
    })
}

fn get_positions(account: &Option<AccountData>) -> Vec<Position> {
    account.as_ref().map_or(vec![], |a| a.positions.clone())
}

async fn get_markets(fetcher: &dyn Fetcher) -> Vec<MarketInfo> {
    let markets = fetcher.get_markets().await.unwrap_or_else(|e| {
        error!("Failed to fetch {} markets: {:?}", fetcher.get_exchange_info(), e);
        vec![]
    });

    let markets: Vec<MarketInfo> = markets
        .into_iter()
        .filter(|m| !m.bid_price.is_zero() && !m.ask_price.is_zero())
        .collect();

    info!("First Market on {}: {:?}", fetcher.get_exchange_info(), markets.first());

    markets
}

async fn get_best_op(arb_ops: &[ArbitrageOpportunity]) -> Option<ArbitrageOpportunity> {
    if let Some(op) = arb_ops.first() {
        info!(
            "Best funding op: Symbol: {}, Market A Rate: {}, Market B Rate: {}, Difference: {}, Market A Leverage: {}, Market B Leverage: {}, Price Crossed: {} Bid Opportunity Price: {}, Ask Opportunity Price: {}",
            op.symbol,
            op.rate_a,
            op.rate_b,
            op.diff * Decimal::from(24 * 365 * 100),
            op.leverage_a,
            op.leverage_b,
            op.price_crossed,
            op.bid_opportunity_price,
            op.ask_opportunity_price
        );
    }

    let best_price_crossed_opportunity = arb_ops.iter().find(|op| op.price_crossed).cloned();

    if let Some(op) = best_price_crossed_opportunity.clone() {
        info!(
            "Best price crossed op: Symbol: {}, Market A Rate: {}, Market B Rate: {}, Difference: {}, Market A Leverage: {}, Market B Leverage: {}, Price Crossed: {} Bid Opportunity Price: {}, Ask Opportunity Price: {}",
            op.symbol,
            op.rate_a,
            op.rate_b,
            op.diff * Decimal::from(24 * 365 * 100),
            op.leverage_a,
            op.leverage_b,
            op.price_crossed,
            op.bid_opportunity_price,
            op.ask_opportunity_price
        );
    }

    best_price_crossed_opportunity
}

async fn get_arbitrage_opportunities_from_markets(
    markets_a: &[MarketInfo],
    markets_b: &[MarketInfo],
) -> Vec<ArbitrageOpportunity> {
    let mut arbitrage_opportunities = Vec::new();

    for market_a in markets_a {
        if let Some(market_b) = markets_b.iter().find(|r| r.symbol == market_a.symbol) {
            let diff = (market_a.funding_rate - market_b.funding_rate).abs();

            // op specific things
            let (price_crossed, (bid_opportunity_price, ask_opportunity_price)) =
                if market_a.funding_rate > market_b.funding_rate {
                    (
                        market_a.ask_price > market_b.bid_price,
                        (market_b.bid_price, market_a.ask_price),
                    )
                } else {
                    (
                        market_b.ask_price > market_a.bid_price,
                        (market_a.bid_price, market_b.ask_price),
                    )
                };

            arbitrage_opportunities.push(ArbitrageOpportunity {
                symbol: market_a.symbol.clone(),
                rate_a: market_a.funding_rate,
                rate_b: market_b.funding_rate,
                diff,
                leverage_a: market_a.leverage,
                leverage_b: market_b.leverage,
                price_crossed,
                bid_opportunity_price,
                ask_opportunity_price,
                market_a: market_a.clone(),
                market_b: market_b.clone(),
            });
        }
    }

    arbitrage_opportunities.sort_by(|a, b| b.diff.cmp(&a.diff));

    info!("Found {} arbitrage opportunities", arbitrage_opportunities.len());

    let exclude_tickers = vec!["MEGA", "MON"];

    arbitrage_opportunities
        .into_iter()
        .filter(|op| !exclude_tickers.contains(&op.symbol.as_str()))
        .collect::<Vec<_>>()
}

fn calculate_trade_attributes<'a>(
    rate_a: Decimal,
    rate_b: Decimal,
    market_a: &'a MarketInfo,
    market_b: &'a MarketInfo,
    operator_a: &'a dyn Operator,
    operator_b: &'a dyn Operator,
    min_available_balance: Decimal,
) -> (
    Decimal,
    &'a MarketInfo,
    &'a MarketInfo,
    &'a dyn Operator,
    &'a dyn Operator,
) {
    let (long_market, short_market, long_operator, short_operator) = if rate_a < rate_b {
        (market_a, market_b, operator_a, operator_b)
    } else {
        (market_b, market_a, operator_b, operator_a)
    };

    let leverage = long_market.leverage.min(short_market.leverage);

    let max_amount_long = (min_available_balance * leverage) / long_market.ask_price;
    let max_amount_short = (min_available_balance * leverage) / short_market.bid_price;

    info!(
        "Trade Sizes Symbol: {} {}| Long Market {}: Decimals {} | Short Market {}: Decimals {}",
        long_market.symbol,
        short_market.symbol,
        long_operator.get_exchange_info(),
        long_market.sz_decimals,
        short_operator.get_exchange_info(),
        short_market.sz_decimals
    );

    let sz_decimals = long_market.sz_decimals.min(short_market.sz_decimals);
    info!("sz_decimals: {}", sz_decimals);

    let max_amount = max_amount_long.min(max_amount_short);
    let amount = max_amount.round_dp_with_strategy(sz_decimals.to_u32().unwrap_or(0), RoundingStrategy::ToZero);

    let size_increment = long_market
        .min_order_size_change
        .max(short_market.min_order_size_change);
    let size_increment_dp = size_increment.normalize().scale();

    // Direct quantization: compute max increments that fit in amount
    let increments = (amount / size_increment).to_u128().unwrap_or(0);
    let quantized_amount =
        (Decimal::from_u128(increments).unwrap_or(Decimal::ZERO) * size_increment).round_dp(size_increment_dp);

    info!(
        "Max Amount {} | Amount: {} Quantized: {}",
        max_amount, amount, quantized_amount
    );

    (
        quantized_amount,
        long_market,
        short_market,
        long_operator,
        short_operator,
    )
}

async fn set_same_leverage(
    market_a: &MarketInfo,
    market_b: &MarketInfo,
    operator_a: &dyn Operator,
    operator_b: &dyn Operator,
) -> PointsBotResult<()> {
    let min_leverage = market_a.leverage.min(market_b.leverage);

    if market_a.leverage > min_leverage {
        operator_a
            .change_leverage(market_a.clone(), min_leverage)
            .await
            .map_err(|e| e)
    } else {
        operator_b
            .change_leverage(market_b.clone(), min_leverage)
            .await
            .map_err(|e| e)
    }
}

async fn get_adjusted_price_and_side(
    market: &MarketInfo,
    side: &PositionSide,
    close: bool,
    operator: &dyn Operator,
) -> (Decimal, PositionSide) {
    let entry_offset = match operator.get_exchange_info() {
        ExchangeName::Hyperliquid => Decimal::from_f64(0.0).unwrap(),
        ExchangeName::Extended => Decimal::from_f64(0.0).unwrap(),
        ExchangeName::Lighter => Decimal::from_f64(0.0).unwrap(),
    };

    /* let exit_offset = match operator.get_exchange_info() {
        ExchangeName::Hyperliquid => Decimal::from_f64(0.0).unwrap(),
        ExchangeName::Extended => Decimal::from_f64(0.0).unwrap(),
        ExchangeName::Lighter => Decimal::from_f64(0.0).unwrap()
    }; */

    let scale_ask = market.ask_price.scale();
    let scale_bid = market.bid_price.scale();

    let exit_offset = Decimal::from_f64(0.000).unwrap();

    match side {
        PositionSide::Long => {
            if close {
                (
                    (market.bid_price * (Decimal::ONE + exit_offset)).round_dp(scale_ask),
                    PositionSide::Short,
                )
            } else {
                (
                    (market.ask_price * (Decimal::ONE - entry_offset)).round_dp(scale_ask),
                    PositionSide::Long,
                )
            }
        }
        PositionSide::Short => {
            if close {
                (
                    (market.ask_price * (Decimal::ONE - exit_offset)).round_dp(scale_bid),
                    PositionSide::Long,
                )
            } else {
                (
                    (market.bid_price * (Decimal::ONE + entry_offset)).round_dp(scale_bid),
                    PositionSide::Short,
                )
            }
        }
    }
}

async fn create_order(
    operator: &dyn Operator,
    market: &MarketInfo,
    side: PositionSide,
    quantity: &Decimal,
    price: &Decimal,
    reduce_only: Option<bool>,
) -> PointsBotResult<OrderResponse> {
    let order_request = OrderRequest {
        id: uuid::Uuid::new_v4().to_string(),
        market: market.clone(),
        side,
        order_type: OrderType::Limit,
        quantity: *quantity,
        price: Some(*price),
        stop_price: None,
        time_in_force: Some("GTC".to_string()),
        reduce_only,
    };

    info!(
        "Exchange: {:?} | Creating order: {:?}",
        operator.get_exchange_info(),
        order_request
    );

    let result = operator.create_order(order_request).await?;

    Ok(result)
}

async fn close_positions_for_market(
    positions: &[Position],
    markets: &[MarketInfo],
    operator: &dyn Operator,
) -> anyhow::Result<()> {
    let label = operator.get_exchange_info();

    if positions.is_empty() {
        info!("No positions to close on {}", label);
        return Ok(());
    } else {
        info!("Closing {} positions on {}", positions.len(), label);
    }

    for position in positions {
        let Some(market) = markets.iter().find(|m| m.symbol == position.symbol) else {
            error!("Market data missing for {} on {}", position.symbol, label);
            continue;
        };

        let (price_adjusted, side) = get_adjusted_price_and_side(market, &position.side, true, operator).await;

        if let Err(e) = create_order(
            operator,
            market,
            side,
            &position.size.abs(),
            &price_adjusted,
            Some(true),
        )
        .await
        {
            error!("Failed to close {} on {}: {:?}", position.symbol, label, e);
        } else {
            info!("Closed {} position on {}", position.symbol, label);
        }
    }

    Ok(())
}

// two params, funding and price
// if funding bad and price good
// if funding good and price not good
// if both good
// if both bad

// check each hour
// if price is good and (funding is not 50% than next op dont close and price is good of the next pos)
// if price is good and (funding is not that good than dont close and price is not that good)
// if price is bad and funding is positive (dont do anything)
// if price is bad and funding is negative (close pos)

// cene
// cene z trading feejom
// cene z moji offsetom

// lighter cene fuckery
