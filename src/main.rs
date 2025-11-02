use anyhow::Result;
use chrono::Utc;
use cron::Schedule;
use log::{error, info};
use rust_decimal::{prelude::FromPrimitive, prelude::ToPrimitive, Decimal};
use std::str::FromStr;
use tokio::time::{sleep, Duration};

use points_bot_rs::{
    fetchers::{AccountData, Fetcher, FetcherExtended, FetcherHyperliquid, MarketInfo, Position},
    operators::{create_operator_hyperliquid, Operator, OperatorExtended, OrderRequest, OrderResponse, OrderType},
    BotConfig, BotMode, ExchangeName, PointsBotResult, PositionSide,
};

#[tokio::main]
async fn main() -> Result<()> {
    use ethers::signers::LocalWallet;
    env_logger::init();

    let config = BotConfig::load_env().expect("Failed to load configuration");
    info!("Starting Points Bot - Mode: {:?} | Wallet: {}", config.mode, config.wallet_address);

    let private_key = config.private_key.as_deref().expect("Missing private_key in config");
    let wallet = LocalWallet::from_str(private_key).expect("Failed to create wallet from private key");

    let fetcher_hyperliquid = Box::new(FetcherHyperliquid::new());
    let fetcher_extended = Box::new(FetcherExtended::new());

    let operator_hyperliquid = create_operator_hyperliquid(wallet).await;
    let operator_extended = Box::new(OperatorExtended::new().await);

    if config.mode == BotMode::Production {
        let schedule = Schedule::from_str("0 20 0,8,16 * * * *").unwrap(); 
        // 0 30 0,4,8,12,16,20 * * * *
        // 0 40 0,8,16 * * * *
        // 0 */10 * * * * *

        loop {
            let now = Utc::now();
            if let Some(next) = schedule.upcoming(Utc).next() {
                let duration = (next - now).to_std().unwrap();
                info!("Next trade scheduled at: {:?}", next);

                sleep(duration).await;
                trade(
                    config.clone(),
                    fetcher_extended.as_ref(),
                    fetcher_hyperliquid.as_ref(),
                    operator_hyperliquid.as_ref(),
                    operator_extended.as_ref(),
                )
                .await;
            }
        }
    } else {
        trade(
            config.clone(),
            fetcher_extended.as_ref(),
            fetcher_hyperliquid.as_ref(),
            operator_hyperliquid.as_ref(),
            operator_extended.as_ref(),
        )
        .await;
        Ok(())
    }
}

async fn trade(
    config: BotConfig,
    fetcher_extended: &dyn Fetcher,
    fetcher_hyperliquid: &dyn Fetcher,
    operator_hyperliquid: &dyn Operator,
    operator_extended: &dyn Operator,
) {
    let (hyperliquid_positions, extended_positions, _) = get_and_handle_account_data(fetcher_hyperliquid, fetcher_extended, &config).await;

    let (hyperliquid_markets, extended_markets) = get_and_handle_markets(fetcher_hyperliquid, fetcher_extended).await;

    let (arbitrage_opportunities, best_price_crossed_opportunity) = get_and_handle_opportunities(&hyperliquid_markets, &extended_markets).await;

    let (symbol, _, _, diff, ..) = best_price_crossed_opportunity.unwrap();

    let prev_symbol = extended_positions.first().map(|p| &p.symbol);
    let prev_best_diff = prev_symbol
        .and_then(|s| arbitrage_opportunities.iter().find(|op| &op.0 == s).map(|op| op.3))
        .unwrap_or(Decimal::ZERO);

    let is_better = diff > prev_best_diff * Decimal::from_f32(1.33).unwrap();
    let is_same_symbol = prev_symbol.map_or(false, |s| symbol == *s);

    info!(
        "Decision Making | Prev op symbol: {:?} | Prev op current funding diff: {:?}% | Next op symbol: {:?} | Next op funding diff: {:?}% | Already Holding Same Asset: {} | Is Better (33% threshold): {}",
        prev_symbol,
        prev_best_diff * Decimal::from(24 * 365 * 100),
        symbol,
        diff * Decimal::from(24 * 365 * 100),
        is_same_symbol,
        is_better
    );

    let is_first_time = extended_positions.is_empty();
    let change_position = !is_first_time && !is_same_symbol && is_better;

    info!("Trade Execution | Is First Time: {} | Change Position: {}", is_first_time, change_position);

    if change_position {
        if let Err(e) = close_all_open_positions(
            &hyperliquid_positions,
            &extended_positions,
            &hyperliquid_markets,
            &extended_markets,
            operator_hyperliquid,
            operator_extended,
        )
        .await
        {
            error!("Failed to close positions: {:?}", e);
        }
        info!("Sleeping for 20 minutes to allow orders to fill and close");
        info!("Next trade scheduled at: {:?}", Utc::now() + Duration::from_secs(20 * 60));

        sleep(Duration::from_secs(20 * 60)).await;
    }

    if is_first_time || change_position {
        let (_, _, min_available_balance) = get_and_handle_account_data(fetcher_hyperliquid, fetcher_extended, &config).await;

        let (hyperliquid_markets, extended_markets) = get_and_handle_markets(fetcher_hyperliquid, fetcher_extended).await;

        let (_, best_price_crossed_opportunity) = get_and_handle_opportunities(&hyperliquid_markets, &extended_markets).await;

        if let Some((symbol, hyper_rate, ext_rate, _, _, _, _, bid_opportunity_price, ask_opportunity_price)) = best_price_crossed_opportunity {
            // TODO: here not really needed to use from markets since you can pass markets in arb opportunities directly
            let hyper_market = hyperliquid_markets.iter().find(|m| m.symbol == *symbol).unwrap();
            let ext_market = extended_markets.iter().find(|m| m.symbol == *symbol).unwrap();
            info!("Markets for arbitrage | Hyperliquid: {:?} | Extended: {:?}", hyper_market, ext_market);

            let (amount, long_market, short_market, long_operator, short_operator) = calculate_trade_attributes(
                hyper_rate,
                ext_rate,
                hyper_market,
                ext_market,
                operator_hyperliquid,
                operator_extended,
                min_available_balance.unwrap(),
            );

            if let Err(e) = set_same_leverage(symbol.to_string(), hyper_market, ext_market, operator_hyperliquid, operator_extended).await {
                error!("Failed to set leverage: {:?}", e);
            }

            let (price_adjusted_long, _) = get_adjusted_price_and_side(long_market, &PositionSide::Long, false, long_operator).await;
            let (price_adjusted_short, _) = get_adjusted_price_and_side(short_market, &PositionSide::Short, false, short_operator).await;
            info!(
                "Amount: {}, Price Adjusted Short: {} Price Adjusted Long: {}, Bid Opportunity Price: {}, Ask Opportunity Price: {}",
                amount, price_adjusted_short, price_adjusted_long, bid_opportunity_price, ask_opportunity_price
            );

            match create_order(short_operator, &symbol, PositionSide::Short, &amount, &price_adjusted_short, Some(false)).await {
                Ok(order_result) => info!("Short order: {:?}", order_result),
                Err(e) => error!("Short order failed: {:?}", e),
            }
            match create_order(long_operator, &symbol, PositionSide::Long, &amount, &price_adjusted_long, Some(false)).await {
                Ok(order_result) => info!("Long order: {:?}", order_result),
                Err(e) => error!("Long order failed: {:?}", e),
            }
        } else {
            error!("No arbitrage opportunities available.");
        }
    }   
}

async fn get_and_handle_account_data(
    fetcher_hyperliquid: &dyn Fetcher,
    fetcher_extended: &dyn Fetcher,
    config: &BotConfig,
) -> (Vec<Position>, Vec<Position>, Option<Decimal>) {
    let (hyperliquid_account, extended_account) = get_all_account_data(fetcher_hyperliquid, fetcher_extended, &config.wallet_address).await;

    let hyperliquid_positions = hyperliquid_account.as_ref().map_or(vec![], |a| a.positions.clone());
    let extended_positions = extended_account.as_ref().map_or(vec![], |a| a.positions.clone());
    info!(
        "Positions | Hyperliquid: {:?} | Extended: {:?}",
        hyperliquid_positions, extended_positions
    );

    let min_available_balance = if let (Some(hyper), Some(ext)) = (&hyperliquid_account, &extended_account) {
        Some(hyper.available_balance.min(ext.available_balance))
    } else {
        None
    };

    info!("Balance to trade min([...exchanges]): {:?}", min_available_balance);

    (hyperliquid_positions, extended_positions, min_available_balance)
}

async fn get_and_handle_markets(fetcher_hyperliquid: &dyn Fetcher, fetcher_extended: &dyn Fetcher) -> (Vec<MarketInfo>, Vec<MarketInfo>) {
    let (hyperliquid_markets, extended_markets) = get_all_markets(fetcher_hyperliquid, fetcher_extended).await;

    // handle empty markets
    let hyperliquid_markets: Vec<MarketInfo> = hyperliquid_markets
        .into_iter()
        .filter(|m| !m.bid_price.is_zero() && !m.ask_price.is_zero())
        .collect();
    let extended_markets: Vec<MarketInfo> = extended_markets
        .into_iter()
        .filter(|m| !m.bid_price.is_zero() && !m.ask_price.is_zero())
        .collect();
    info!(
        "First Markets | Hyperliquid: {:?} | Extended: {:?}",
        hyperliquid_markets.first(),
        extended_markets.first()
    );

    (hyperliquid_markets, extended_markets)
}

async fn get_and_handle_opportunities(
    hyperliquid_markets: &Vec<MarketInfo>,
    extended_markets: &Vec<MarketInfo>,
) -> (
    Vec<(String, Decimal, Decimal, Decimal, Decimal, Decimal, bool, Decimal, Decimal)>,
    Option<(String, Decimal, Decimal, Decimal, Decimal, Decimal, bool, Decimal, Decimal)>,
) {
    let exclude_tickers = vec!["MEGA", "MON"];

    let arbitrage_opportunities = get_arbitrage_opportunities_from_markets(hyperliquid_markets, extended_markets)
        .await
        .into_iter()
        .filter(|op| !exclude_tickers.contains(&op.0.as_str()))
        .collect::<Vec<_>>();

    info!("Arbitrage Opportunities: {:?}", arbitrage_opportunities);

    if let Some((symbol, hyper_rate, ext_rate, diff, hyper_leverage, ext_leverage, price_crossed, bid_opportunity_price, ask_opportunity_price)) =
        arbitrage_opportunities.first()
    {
        info!(
            "Best funding op: Symbol: {}, Hyperliquid Rate: {}, Extended Rate: {}, Difference: {}, Hyper Leverage: {}, Extended Leverage: {}, Price Crossed: {} Bid Opportunity Price: {}, Ask Opportunity Price: {}",
            symbol,
            hyper_rate,
            ext_rate,
            diff * Decimal::from(24 * 365 * 100),
            hyper_leverage,
            ext_leverage,
            price_crossed,
            bid_opportunity_price,
            ask_opportunity_price
        );
    }

    let best_price_crossed_opportunity = arbitrage_opportunities.iter().find(|op| op.6).cloned();

    if let Some((symbol, hyper_rate, ext_rate, diff, hyper_leverage, ext_leverage, price_crossed, bid_opportunity_price, ask_opportunity_price)) =
        best_price_crossed_opportunity.clone()
    {
        info!(
            "Best price crossed op: Symbol: {}, Hyperliquid Rate: {}, Extended Rate: {}, Difference: {}, Hyper Leverage: {}, Extended Leverage: {}, Price Crossed: {} Bid Opportunity Price: {}, Ask Opportunity Price: {}",
            symbol,
            hyper_rate,
            ext_rate,
            diff * Decimal::from(24 * 365 * 100),
            hyper_leverage,
            ext_leverage,
            price_crossed,
            bid_opportunity_price,
            ask_opportunity_price
        );
    }

    (arbitrage_opportunities, best_price_crossed_opportunity)
}

async fn get_all_account_data(
    fetcher_hyperliquid: &dyn Fetcher,
    fetcher_extended: &dyn Fetcher,
    wallet_address: &str,
) -> (Option<AccountData>, Option<AccountData>) {
    let hyperliquid_account = fetcher_hyperliquid.get_account_data(wallet_address).await.map(Some).unwrap_or_else(|e| {
        error!("Failed to get Hyperliquid account: {:?}", e);
        None
    });

    let extended_account = fetcher_extended.get_account_data(wallet_address).await.map(Some).unwrap_or_else(|e| {
        error!("Failed to get Extended account: {:?}", e);
        None
    });

    (hyperliquid_account, extended_account)
}

async fn get_all_markets(fetcher_hyperliquid: &dyn Fetcher, fetcher_extended: &dyn Fetcher) -> (Vec<MarketInfo>, Vec<MarketInfo>) {
    let hyperliquid_markets = fetcher_hyperliquid.get_markets().await.unwrap_or_else(|e| {
        error!("Failed to fetch Hyperliquid markets: {:?}", e);
        vec![]
    });

    let extended_markets = fetcher_extended.get_markets().await.unwrap_or_else(|e| {
        error!("Failed to fetch Extended markets: {:?}", e);
        vec![]
    });

    (hyperliquid_markets, extended_markets)
}

async fn get_arbitrage_opportunities_from_markets(
    hyperliquid_markets: &[MarketInfo],
    extended_markets: &[MarketInfo],
) -> Vec<(String, Decimal, Decimal, Decimal, Decimal, Decimal, bool, Decimal, Decimal)> {
    let mut arbitrage_opportunities = Vec::new();

    for hyper_market in hyperliquid_markets {
        if let Some(ext_market) = extended_markets.iter().find(|r| r.symbol == hyper_market.symbol) {
            let diff = (hyper_market.funding_rate - ext_market.funding_rate).abs();

            // op specific things
            let (price_crossed, (bid_opportunity_price, ask_opportunity_price)) = if hyper_market.funding_rate > ext_market.funding_rate {
                (
                    hyper_market.ask_price > ext_market.bid_price,
                    (ext_market.bid_price, hyper_market.ask_price),
                )
            } else {
                (
                    ext_market.ask_price > hyper_market.bid_price,
                    (hyper_market.bid_price, ext_market.ask_price),
                )
            };

            arbitrage_opportunities.push((
                hyper_market.symbol.clone(),
                hyper_market.funding_rate,
                ext_market.funding_rate,
                diff,
                hyper_market.leverage,
                ext_market.leverage,
                price_crossed,
                bid_opportunity_price,
                ask_opportunity_price,
            ));
        }
    }

    arbitrage_opportunities.sort_by(|a, b| b.3.cmp(&a.3));

    info!("Found {} arbitrage opportunities", arbitrage_opportunities.len());

    arbitrage_opportunities
}

fn calculate_trade_attributes<'a>(
    hyper_rate: Decimal,
    ext_rate: Decimal,
    hyper_market: &'a MarketInfo,
    ext_market: &'a MarketInfo,
    operator_hyperliquid: &'a dyn Operator,
    operator_extended: &'a dyn Operator,
    min_available_balance: Decimal,
) -> (Decimal, &'a MarketInfo, &'a MarketInfo, &'a dyn Operator, &'a dyn Operator) {
    let (long_market, short_market, long_operator, short_operator) = if hyper_rate < ext_rate {
        (hyper_market, ext_market, operator_hyperliquid, operator_extended)
    } else {
        (ext_market, hyper_market, operator_extended, operator_hyperliquid)
    };

    let leverage = long_market.leverage.min(short_market.leverage);

    let max_amount_long = (min_available_balance * leverage) / long_market.ask_price;
    let max_amount_short = (min_available_balance * leverage) / short_market.bid_price;

    info!("Trade Sizes | Long Market: {} | Short Market: {}", long_market.sz_decimals, short_market.sz_decimals);

    let sz_decimals = long_market.sz_decimals.min(short_market.sz_decimals);
    info!("sz_decimals: {}", sz_decimals);

    let max_amount = max_amount_long.min(max_amount_short);
    use rust_decimal::RoundingStrategy;
    let amount = max_amount.round_dp_with_strategy(sz_decimals.to_u32().unwrap_or(0), RoundingStrategy::ToZero);

    info!("Max Amount {} | Amount: {}", max_amount, amount);

    info!("Trade Execution | Amount: {} | Long Market: {} | Short Market: {}", amount, long_market.symbol, short_market.symbol);

    (amount, long_market, short_market, long_operator, short_operator)
}

async fn set_same_leverage(
    symbol: String,
    hyper_market: &MarketInfo,
    ext_market: &MarketInfo,
    operator_hyperliquid: &dyn Operator,
    operator_extended: &dyn Operator,
) -> PointsBotResult<()> {
    let min_leverage = hyper_market.leverage.min(ext_market.leverage);

    if hyper_market.leverage > min_leverage {
        operator_hyperliquid.change_leverage(symbol, min_leverage).await.map_err(|e| e)
    } else {
        operator_extended.change_leverage(symbol, min_leverage).await.map_err(|e| e)
    }
}

async fn get_adjusted_price_and_side(market: &MarketInfo, side: &PositionSide, close: bool, operator: &dyn Operator) -> (Decimal, PositionSide) {
    // Dynamically adjust bips_offset based on operator type
    let bips_offset = match operator.get_exchange_info() {
        ExchangeName::Hyperliquid => Decimal::from_f64(0.0).unwrap(),
        ExchangeName::Extended => Decimal::from_f64(0.0).unwrap(),
        _ => Decimal::from_f64(0.00015).unwrap(),
    };

    //     ExchangeName::Hyperliquid => Decimal::from_f64(if *side == PositionSide::Long { 0.0 } else { -0.001 }).unwrap(),

    let scale_ask = market.ask_price.scale();
    let scale_bid = market.bid_price.scale();

    let cross_book_offset_for_closing = Decimal::from_f64(0.00015).unwrap();

    match side {
        PositionSide::Long => {
            if close {
                (
                    (market.bid_price * (Decimal::ONE + bips_offset - cross_book_offset_for_closing)).round_dp(scale_ask),
                    PositionSide::Short,
                )
            } else {
                ((market.ask_price * (Decimal::ONE - bips_offset)).round_dp(scale_ask), PositionSide::Long)
            }
        }
        PositionSide::Short => {
            if close {
                (
                    (market.ask_price * (Decimal::ONE - bips_offset + cross_book_offset_for_closing)).round_dp(scale_bid),
                    PositionSide::Long,
                )
            } else {
                ((market.bid_price * (Decimal::ONE + bips_offset)).round_dp(scale_bid), PositionSide::Short)
            }
        }
    }
}

async fn create_order(
    operator: &dyn Operator,
    symbol: &str,
    side: PositionSide,
    quantity: &Decimal,
    price: &Decimal,
    reduce_only: Option<bool>,
) -> PointsBotResult<OrderResponse> {
    let order_request = OrderRequest {
        id: uuid::Uuid::new_v4().to_string(),
        symbol: symbol.to_string(),
        side,
        order_type: OrderType::Limit,
        quantity: *quantity,
        price: Some(*price),
        stop_price: None,
        time_in_force: Some("GTC".to_string()),
        reduce_only,
    };

    info!("Exchange: {:?} | Creating order: {:?}", operator.get_exchange_info(), order_request);

    let result = operator.create_order(order_request).await?;

    Ok(result)
}

async fn close_all_open_positions(
    hyperliquid_positions: &[Position],
    extended_positions: &[Position],
    hyperliquid_markets: &[MarketInfo],
    extended_markets: &[MarketInfo],
    operator_hyperliquid: &dyn Operator,
    operator_extended: &dyn Operator,
) -> anyhow::Result<()> {
    if hyperliquid_positions.is_empty() && extended_positions.is_empty() {
        info!("No positions available to close.");
        return Ok(());
    }
    info!("Closing existing positions...");

    for (positions, markets, operator, label) in [
        (hyperliquid_positions, hyperliquid_markets, operator_hyperliquid, "Hyperliquid"),
        (extended_positions, extended_markets, operator_extended, "Extended"),
    ] {
        for position in positions {
            if let Some(market) = markets.iter().find(|m| m.symbol == position.symbol) {
                let (price_adjusted, side) = get_adjusted_price_and_side(market, &position.side, true, operator).await;

                info!("Closing {label} position: {} with adjusted price: {} {:?} Original ask price: {} Original bid price: {}", position.symbol, price_adjusted, side, market.ask_price, market.bid_price);
                if let Err(e) = create_order(operator, &position.symbol, side, &position.size.abs(), &price_adjusted, Some(true)).await {
                    error!("Failed to close {label} position: {:?}", e);
                }
            } else {
                error!("Market data not found for symbol: {}", position.symbol);
            }
        }
    }
    Ok(())
}
