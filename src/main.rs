use anyhow::Result;
use chrono::Utc;
use cron::Schedule;
use log::{error, info};
use rust_decimal::{
    prelude::{FromPrimitive, ToPrimitive},
    Decimal,
};
use std::{collections::HashMap, str::FromStr};
use tokio::{
    task,
    time::{sleep, Duration},
};

use points_bot_rs::{
    config,
    fetchers::{AccountData, Fetcher, MarketInfo, Position},
    operators::{Operator, OrderRequest, OrderResponse, OrderType},
    BotEnvConfig, BotJsonConfig, BotMode, ExchangeName, PointsBotResult, PositionSide,
};
use rust_decimal::RoundingStrategy;

#[derive(Debug, Clone)]
struct ArbitrageOpportunity {
    symbol: String,
    long_market: MarketInfo,
    short_market: MarketInfo,
    entry_arbitrage: Option<EntryArbitrage>,
    exit_arbitrage: Option<ExitArbitrage>,
    funding_diff: Decimal,
}

#[derive(Debug, Clone)]
struct EntryArbitrage {
    long_entry_px: Decimal,
    short_entry_px: Decimal,
    long_entry_px_wfees: Decimal,
    short_entry_px_wfees: Decimal,
    entry_arb_valid_before_fees: bool,
    entry_arb_valid_after_fees: bool,
}

#[derive(Debug, Clone)]
struct ExitArbitrage {
    long_exit_px: Decimal,
    short_exit_px: Decimal,
    long_exit_px_wfees: Decimal,
    short_exit_px_wfees: Decimal,
    exit_arb_valid_before_fees: bool,
    exit_arb_valid_after_fees: bool,
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
        let schedule = Schedule::from_str("0 20 * * * * *").unwrap();
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
                            &config_json,
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

        let config_ref = config.as_ref().expect("Config is None");
        let bot_config = BotJsonConfig::process_config(config_ref).await;

        trade(
            config_ref,
            bot_config.fetcher_a.as_ref(),
            bot_config.fetcher_b.as_ref(),
            bot_config.operator_a.as_ref(),
            bot_config.operator_b.as_ref(),
        )
        .await;
        Ok(())
    }
}

async fn trade(
    config: &BotJsonConfig,
    fetcher_a: &dyn Fetcher,
    fetcher_b: &dyn Fetcher,
    operator_a: &dyn Operator,
    operator_b: &dyn Operator,
) {
    let env_config = BotEnvConfig::load_env().expect("Failed to load configuration");
    let (positions_a, positions_b, _, markets_a, markets_b) = get_trading_data(fetcher_a, fetcher_b).await;

    let mut operator_map: HashMap<ExchangeName, &dyn Operator> = HashMap::new();
    operator_map.insert(operator_a.get_exchange_info(), operator_a);
    operator_map.insert(operator_b.get_exchange_info(), operator_b);

    let has_positions_open = positions_a.len() > 0 || positions_b.len() > 0;
    let mut change_position = true;

    if has_positions_open {
        let current_symbol = if positions_a.len() > 0 {
            positions_a.first().map(|p| &p.symbol)
        } else {
            positions_b.first().map(|p| &p.symbol)
        };

        let arbitrage_opportunities = calculate_arbitrage_opportunities(&markets_a, &markets_b, config).await;

        let current_op_data = current_symbol.and_then(|s| arbitrage_opportunities.iter().find(|op| &op.symbol == s));

        let current_funding_diff_ann = current_op_data
            .map(|op| op.funding_diff * Decimal::from(24 * 365 * 100))
            .unwrap_or(Decimal::ZERO);

        info!(
            "Current open position symbol: {:?}, funding diff ann: {:?}%",
            current_symbol, current_funding_diff_ann
        );

        let new_op = get_first_op(&arbitrage_opportunities).await.unwrap();
        let new_symbol = &new_op.symbol;
        let new_funding_diff_ann = new_op.funding_diff * Decimal::from(24 * 365 * 100);
        // let is_same_symbol =  current_symbol.map_or(false, |s| new_symbol == s);

        info!(
            "Current best new op symbol: {:?}, funding diff ann: {:?}%",
            new_symbol, new_funding_diff_ann
        );

        let position_a = positions_a.iter().find(|p| p.symbol == *current_symbol.unwrap());
        let position_b = positions_b.iter().find(|p| p.symbol == *current_symbol.unwrap());

        let market_a = position_a
            .and_then(|pos| get_market_for_position(pos, &markets_a))
            .unwrap();
        let market_b = position_b
            .and_then(|pos| get_market_for_position(pos, &markets_b))
            .unwrap();

        let (long_market, short_market) = if position_a.unwrap().side == PositionSide::Long {
            (market_a, market_b)
        } else {
            (market_b, market_a)
        };

        // smth wrong kill all
        if positions_a.len() != 1 && positions_b.len() != 1 {
            info!("Something went wrong with positions, closing all positions");
            close_positions_for_markets(&positions_a, &markets_a, operator_a, config)
                .await
                .unwrap_or_else(|e| {
                    error!("Failed to close positions on A: {:?}", e);
                });
            close_positions_for_markets(&positions_b, &markets_b, operator_b, config)
                .await
                .unwrap_or_else(|e| {
                    error!("Failed to close positions on B: {:?}", e);
                });
            return;
        }

        let exit_arb = Some(get_exit_arb_data(long_market.clone(), short_market.clone(), config));

        if exit_arb.as_ref().unwrap().exit_arb_valid_after_fees {
            info!("Our exit is profitable after fees, close position");
            change_position = true;
        } else if current_funding_diff_ann < Decimal::from(-10) {
            info!(
                "Current position funding ann is negative {}: change position",
                current_funding_diff_ann
            );
            change_position = true;
        } else {
            info!("Dont do anything funding positive, price not ready to exit");
            change_position = false
        }

        info!(
            "Trade Execution | Has Positions Open: {} | Change Position: {}",
            has_positions_open, change_position
        );

        if change_position {
            info!("Can exit position profitably, or funding is to negative and we kill");

            let exit_arb = exit_arb.as_ref().unwrap();
            let long_exit_px = exit_arb.long_exit_px;
            let short_exit_px = exit_arb.short_exit_px;

            let long_operator = operator_map.get(&long_market.exchange).unwrap();
            let short_operator = operator_map.get(&short_market.exchange).unwrap();

            info!(
                "Exit long {} with price: {} on {}",
                current_symbol.as_ref().unwrap(),
                long_exit_px,
                long_operator.get_exchange_info()
            );
            info!(
                "Exit short {} with price: {} on {}",
                current_symbol.as_ref().unwrap(),
                short_exit_px,
                short_operator.get_exchange_info()
            );
            info!(
                "EXIT: Spread between prices: {}, %spread {}",
                long_exit_px - short_exit_px,
                (long_exit_px - short_exit_px) / short_exit_px * Decimal::from(100)
            );

            match create_order(
                *long_operator,
                &long_market,
                PositionSide::Short,
                &position_a.unwrap().size,
                &long_exit_px,
                Some(true),
            )
            .await
            {
                Ok(order_result) => info!("Long order: {:?}", order_result),
                Err(e) => error!("Long order failed: {:?}", e),
            }

            match create_order(
                *short_operator,
                &short_market,
                PositionSide::Long,
                &position_a.unwrap().size,
                &short_exit_px,
                Some(true),
            )
            .await
            {
                Ok(order_result) => info!("Short order: {:?}", order_result),
                Err(e) => error!("Short order failed: {:?}", e),
            } 

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
    }

    if !has_positions_open || change_position {
        let (_, _, min_available_balance, markets_a, markets_b) = get_trading_data(fetcher_a, fetcher_b).await;

        let arbitrage_opportunities = calculate_arbitrage_opportunities(&markets_a, &markets_b, &config).await;
        let first_op = get_first_op(&arbitrage_opportunities).await;

        if first_op.is_none() {
            error!("No arbitrage opportunities available! Wait for next session!");
            return;
        }

        if first_op.is_some() {
            let op = first_op.unwrap();
            info!(
                "Markets for arbitrage | Long on {} | Short on {}",
                op.long_market.exchange, op.short_market.exchange,
            );

            let long_operator = operator_map.get(&op.long_market.exchange).unwrap();
            let short_operator = operator_map.get(&op.short_market.exchange).unwrap();
            let qty = get_order_qty(&op.long_market, &op.short_market, min_available_balance.unwrap());

            if let Err(e) = set_same_leverage(&op.long_market, &op.short_market, *long_operator, *short_operator).await
            {
                error!("Failed to set leverage: {:?}", e);
            }

            let entry_arbitrage = op.entry_arbitrage.as_ref().unwrap();
            let long_entry_px = entry_arbitrage.long_entry_px;
            let short_entry_px = entry_arbitrage.short_entry_px;

            info!(
                "Long with price: {} on {}",
                long_entry_px,
                long_operator.get_exchange_info()
            );
            info!(
                "Short with price: {} on {}",
                short_entry_px,
                short_operator.get_exchange_info()
            );
            info!(
                "Spread between prices: {}, %spread {}",
                long_entry_px - short_entry_px,
                (long_entry_px - short_entry_px) / short_entry_px * Decimal::from(100)
            );

            match create_order(
                *long_operator,
                &op.long_market,
                PositionSide::Long,
                &qty,
                &long_entry_px,
                Some(false),
            )
            .await
            {
                Ok(order_result) => info!("Long order: {:?}", order_result),
                Err(e) => error!("Long order failed: {:?}", e),
            }

            match create_order(
                *short_operator,
                &op.short_market,
                PositionSide::Short,
                &qty,
                &short_entry_px,
                Some(false),
            )
            .await
            {
                Ok(order_result) => info!("Short order: {:?}", order_result),
                Err(e) => error!("Short order failed: {:?}", e),
            }
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

async fn get_first_op(arb_ops: &[ArbitrageOpportunity]) -> Option<ArbitrageOpportunity> {
    if let Some(op) = arb_ops.first() {
        let entry_arbitrage = op.entry_arbitrage.as_ref().unwrap();

        info!(
            "First funding op: Symbol: {}, Long Exchange: {}, Short Exchange: {}, Funding Diff: {}, Long Entry Px (w/fees): {}, Short Entry Px (w/fees): {}, Arb Valid Before Fees: {}, Arb Valid After Fees: {}",
            op.symbol,
            op.long_market.exchange,
            op.short_market.exchange,
            op.funding_diff * Decimal::from(24 * 365 * 100),
            entry_arbitrage.long_entry_px_wfees,
            entry_arbitrage.short_entry_px_wfees,
            entry_arbitrage.entry_arb_valid_before_fees,
            entry_arbitrage.entry_arb_valid_after_fees
        );
    }

    let best_arbitrage_op = arb_ops
        .iter()
        .find(|op| op.entry_arbitrage.as_ref().unwrap().entry_arb_valid_after_fees)
        .cloned();

    if let Some(op) = best_arbitrage_op.clone() {
        let entry_arbitrage = op.entry_arbitrage.unwrap();

        info!(
            "First valid arbitrage op: Symbol: {}, Long Exchange: {}, Short Exchange: {}, Funding Diff: {}, Long Entry Px (w/fees): {}, Short Entry Px (w/fees): {}, Arb Valid Before Fees: {}, Arb Valid After Fees: {}",
            op.symbol,
            op.long_market.exchange,
            op.short_market.exchange,
            op.funding_diff * Decimal::from(24 * 365 * 100),
            entry_arbitrage.long_entry_px_wfees,
            entry_arbitrage.short_entry_px_wfees,
            entry_arbitrage.entry_arb_valid_before_fees,
            entry_arbitrage.entry_arb_valid_after_fees
        );
    }

    best_arbitrage_op
}

async fn calculate_arbitrage_opportunities(
    markets_a: &[MarketInfo],
    markets_b: &[MarketInfo],
    config: &BotJsonConfig,
) -> Vec<ArbitrageOpportunity> {
    let exclude_tickers = ["MEGA", "MON"];
    let mut opportunities = Vec::new();

    for market_a in markets_a {
        if exclude_tickers.contains(&market_a.symbol.as_str()) {
            continue;
        }
        if let Some(market_b) = markets_b.iter().find(|r| r.symbol == market_a.symbol) {
            let funding_diff = (market_a.funding_rate - market_b.funding_rate).abs();

            // get funding diff
            let (long_market, short_market) = if market_a.funding_rate < market_b.funding_rate {
                (market_a, market_b)
            } else {
                (market_b, market_a)
            };

            let entry_arb = Some(get_entry_arb_data(long_market.clone(), short_market.clone(), config));
            opportunities.push(ArbitrageOpportunity {
                symbol: market_a.symbol.clone(),
                long_market: long_market.clone(),
                short_market: short_market.clone(),
                entry_arbitrage: entry_arb,
                exit_arbitrage: None,
                funding_diff,
            });
        }
    }

    // Sort by best funding diff, descending
    opportunities.sort_by(|a, b| b.funding_diff.cmp(&a.funding_diff));

    info!("Found {} arbitrage opportunities", opportunities.len());

    opportunities
}

fn get_entry_arb_data(market_long: MarketInfo, market_short: MarketInfo, config: &BotJsonConfig) -> EntryArbitrage {
    let long_entry_px = market_long.ask_price; // this is not mid, this is next price a seller is willing to sell to us
    let short_entry_px = market_short.bid_price; // this is not mid, this is next price a buyer is willing to buy from us

    let long_entry_px_wfees = long_entry_px
        * (Decimal::ONE
            + BotJsonConfig::get_taker_fee(market_long.exchange)
            + BotJsonConfig::get_entry_offset(config, market_long.exchange, true));
    let short_entry_px_wfees = short_entry_px
        * (Decimal::ONE
            + BotJsonConfig::get_taker_fee(market_short.exchange)
            + BotJsonConfig::get_entry_offset(config, market_short.exchange, false));

    let entry_arb_valid_before_fees = long_entry_px < short_entry_px;

    let entry_arb_valid_after_fees = long_entry_px_wfees < short_entry_px_wfees;
    if entry_arb_valid_before_fees {
        info!(
            "ENTRY ARB VALID BEFORE FEES: {} | Long Market: {} | Short Market: {} | Long Entry Price: {} | Short Entry Price: {} | spread {} | spread% {}% | my check: {}  my check with fees: {} long_entry_px_wfees {} short_entry_px_wfees {}",
            market_long.symbol,
            market_long.exchange,
            market_short.exchange,
            long_entry_px,
            short_entry_px,
            short_entry_px - long_entry_px,
            ((short_entry_px - long_entry_px) / short_entry_px) * Decimal::from(100),
            long_entry_px < short_entry_px,
            entry_arb_valid_after_fees,
            long_entry_px_wfees,
            short_entry_px_wfees
        );
    }

    EntryArbitrage {
        long_entry_px,
        short_entry_px,
        long_entry_px_wfees,
        short_entry_px_wfees,
        entry_arb_valid_before_fees,
        entry_arb_valid_after_fees,
    }
}

fn get_exit_arb_data(market_long: MarketInfo, market_short: MarketInfo, config: &BotJsonConfig) -> ExitArbitrage {
    let long_exit_px = market_long.bid_price; // this is not mid, this is next price a seller is willing to buy from us
    let short_exit_px = market_short.ask_price; // this is not mid, this is next price a buyer is willing to sell to us

    let long_exit_px_wfees = long_exit_px
        * (Decimal::ONE
            + BotJsonConfig::get_taker_fee(market_long.exchange)
            + BotJsonConfig::get_exit_offset(config, market_long.exchange, false));
    let short_exit_px_wfees = short_exit_px
        * (Decimal::ONE
            + BotJsonConfig::get_taker_fee(market_short.exchange)
            + BotJsonConfig::get_exit_offset(config, market_short.exchange, true));

    let exit_arb_valid_before_fees = long_exit_px > short_exit_px;

    let exit_arb_valid_after_fees = long_exit_px_wfees > short_exit_px_wfees;
    info!(
        "EXIT ARB DATA BEFORE FEES: {} | Long Market: {} | Short Market: {} | Long Exit Price: {} | Short Exit Price: {} | spread {} | spread% {}% | my check: {}  my check with fees: {} long_exit_px_wfees {} short_exit_px_wfees {}",
        market_long.symbol,
        market_long.exchange,
        market_short.exchange,
        long_exit_px,
        short_exit_px,
        short_exit_px - long_exit_px,
        ((short_exit_px - long_exit_px) / short_exit_px) * Decimal::from(100),
        long_exit_px > short_exit_px,
        exit_arb_valid_after_fees,
        long_exit_px_wfees,
        short_exit_px_wfees,
    );

    ExitArbitrage {
        long_exit_px,
        short_exit_px,
        long_exit_px_wfees,
        short_exit_px_wfees,
        exit_arb_valid_before_fees,
        exit_arb_valid_after_fees,
    }
}

fn get_order_qty(long_market: &MarketInfo, short_market: &MarketInfo, min_available_balance: Decimal) -> Decimal {
    let leverage = long_market.leverage.min(short_market.leverage);

    let max_amount_long = (min_available_balance * leverage) / long_market.ask_price;
    let max_amount_short = (min_available_balance * leverage) / short_market.bid_price;

    info!(
        "Qty calc for Symbol: {} {}| Long Market {}: No. decimals {} | Short Market {}: No. decimals {}",
        long_market.symbol,
        short_market.symbol,
        long_market.exchange,
        long_market.sz_decimals,
        short_market.exchange,
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

    quantized_amount
}

async fn set_same_leverage(
    long_market: &MarketInfo,
    short_market: &MarketInfo,
    long_operator: &dyn Operator,
    short_operator: &dyn Operator,
) -> PointsBotResult<()> {
    let min_leverage = long_market.leverage.min(short_market.leverage);

    if long_market.leverage > min_leverage {
        long_operator
            .change_leverage(long_market.clone(), min_leverage)
            .await
            .map_err(|e| e)
    } else {
        short_operator
            .change_leverage(short_market.clone(), min_leverage)
            .await
            .map_err(|e| e)
    }
}

async fn get_adjusted_price_and_side(market: &MarketInfo, side: &PositionSide, close: bool) -> (Decimal, PositionSide) {
    /* let entry_offset = match operator.get_exchange_info() {
        ExchangeName::Hyperliquid => Decimal::from_f64(0.0).unwrap(),
        ExchangeName::Extended => Decimal::from_f64(0.0).unwrap(),
        ExchangeName::Lighter => Decimal::from_f64(0.0).unwrap(),
    };

    let exit_offset = match operator.get_exchange_info() {
        ExchangeName::Hyperliquid => Decimal::from_f64(0.0).unwrap(),
        ExchangeName::Extended => Decimal::from_f64(0.0).unwrap(),
        ExchangeName::Lighter => Decimal::from_f64(0.0).unwrap()
    }; */

    let scale_ask = market.ask_price.scale();
    let scale_bid = market.bid_price.scale();

    let entry_offset = Decimal::from_f64(0.000).unwrap();
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

// kill all
async fn close_positions_for_markets(
    positions: &[Position],
    markets: &[MarketInfo],
    operator: &dyn Operator,
    config: &BotJsonConfig,
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

        let (price_adjusted, side) = get_adjusted_price_and_side(market, &position.side, true).await;

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

fn get_market_for_position<'a>(position: &Position, markets: &'a [MarketInfo]) -> Option<&'a MarketInfo> {
    markets.iter().find(|m| m.symbol == position.symbol)
}
