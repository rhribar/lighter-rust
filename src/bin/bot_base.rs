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
    fetchers::{write_last_change, AccountData, Fetcher, MarketInfo, Position},
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
    long_px: Decimal,
    short_px: Decimal,
    long_px_with_fees: Decimal,
    short_px_with_fees: Decimal,
    is_valid_before_fees: bool,
    is_valid_after_fees: bool,
}

#[derive(Debug, Clone)]
struct ExitArbitrage {
    long_px: Decimal,
    short_px: Decimal,
    long_px_with_fees: Decimal,
    short_px_with_fees: Decimal,
    is_valid_before_fees: bool,
    is_valid_after_fees: bool,
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
        // 0 20 * * * * *

        loop {
            let now = Utc::now();

            if let Some(next) = schedule.upcoming(Utc).next() {
                let duration = (next - now).to_std().unwrap();
                info!("[MAIN] Next trade scheduled at, timestamp={:?}", next);

                sleep(duration).await;

                let configs = BotJsonConfig::load_config_file(&env_config)
                    .expect("Failed to load config file");

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

    let mut operator_map: HashMap<ExchangeName, &dyn Operator> = HashMap::new();
    operator_map.insert(operator_a.get_exchange_info(), operator_a);
    operator_map.insert(operator_b.get_exchange_info(), operator_b);

    let (positions_a, positions_b, _, markets_a, markets_b) =
        get_trading_data(fetcher_a, fetcher_b).await;

    let has_positions_open = positions_a.len() > 0 || positions_b.len() > 0;
    let mut change_position = true;

    if has_positions_open {
        let current_symbol = if positions_a.len() > 0 {
            positions_a.first().map(|p| &p.symbol)
        } else {
            positions_b.first().map(|p| &p.symbol)
        };

        let position_a = positions_a
            .iter()
            .find(|p| p.symbol == *current_symbol.unwrap())
            .expect(&format!(
                "[ERROR] No position found, symbol={} exchange={}",
                current_symbol.unwrap(),
                operator_a.get_exchange_info()
            ));
        let position_b = positions_b
            .iter()
            .find(|p| p.symbol == *current_symbol.unwrap())
            .expect(&format!(
                "[ERROR] No position found, symbol={} exchange={}",
                current_symbol.unwrap(),
                operator_b.get_exchange_info()
            ));

        let (position_long, position_short, markets_long, markets_short) =
            if position_a.side == PositionSide::Long {
                (position_a, position_b, &markets_a, &markets_b)
            } else {
                (position_b, position_a, &markets_b, &markets_a)
            };

        if positions_a.len() != 1 || positions_b.len() != 1 {
            error!("[MAIN] Positions not symmetrical, past order didnt get filled, closing all positions, positions_a_len={} positions_b_len={}", positions_a.len(), positions_b.len());
            close_positions_for_markets(&positions_a, &markets_a, operator_a)
                .await
                .unwrap_or_else(|e| {
                    error!(
                        "Failed to close positions: exchange={} e={:?}",
                        operator_a.get_exchange_info(),
                        e
                    );
                });
            close_positions_for_markets(&positions_b, &markets_b, operator_b)
                .await
                .unwrap_or_else(|e| {
                    error!(
                        "Failed to close positions: exchange={} e={:?}",
                        operator_b.get_exchange_info(),
                        e
                    );
                });
            return;
        }

        // info!("Available market symbols: {:?}", markets_long);

        let long_market = get_market_for_position(position_long, &markets_long).expect(&format!(
            "[ERROR] Could not find market for long position, symbol={} exchange={}",
            position_long.symbol,
            markets_long.first().unwrap().exchange
        ));
        let short_market =
            get_market_for_position(position_short, &markets_short).expect(&format!(
                "[ERROR] Could not find market for short position, symbol={} exchange={}",
                position_short.symbol,
                markets_short.first().unwrap().exchange
            ));

        let arbitrage_opportunities =
            calculate_arbitrage_opportunities(&markets_a, &markets_b, config).await;

        // current position data
        let current_op_data = arbitrage_opportunities
            .iter()
            .find(|op| &op.symbol == current_symbol.unwrap())
            .unwrap();

        let current_funding_diff = if long_market.exchange == current_op_data.long_market.exchange {
            current_op_data.funding_diff
        } else {
            -current_op_data.funding_diff
        };

        let leverage = current_op_data
            .long_market
            .leverage
            .min(current_op_data.short_market.leverage);

        info!(
            "[MAIN] Current open position data, symbol={:?}, funding rate={:?}%, leverage={}",
            current_symbol, current_funding_diff, leverage
        );

        // new position data
        if let Some(new_op) = get_valid_opportunity(&arbitrage_opportunities).await {
            let new_symbol = &new_op.symbol;
            let new_funding_diff = new_op.funding_diff;

            info!(
                "[MAIN] New best position data, symbol={:?}, funding rate={:?}%",
                new_symbol, new_funding_diff
            );
        }

        // calculate stats
        let exit_arb = Some(get_exit_arb_data(
            long_market.clone(),
            short_market.clone(),
            config,
        ));
        let exit_arb = exit_arb.as_ref().unwrap();
        let long_exit_px = exit_arb.long_px;
        let short_exit_px = exit_arb.short_px;

        info!(
            "[MAIN] Current position exit prices, long_exit_px={:?}, short_exit_px={:?}, is_valid_after_fees={:?}, is_valid_before_fees={:?}",
            long_exit_px, short_exit_px, exit_arb.is_valid_after_fees, exit_arb.is_valid_before_fees
        );

        let pnl_cum = calc_pnl(exit_arb, position_long, position_short).await;

        let percent_exposure_long = ((long_exit_px / position_long.entry_price) - Decimal::ONE)
            * leverage
            * Decimal::from(100);
        let percent_exposure_short = (Decimal::ONE - (short_exit_px / position_short.entry_price))
            * leverage
            * Decimal::from(100);

        let percent_exposure = percent_exposure_long.max(percent_exposure_short);

        info!(
            "[MAIN] Calculate % exposure to liquidation, short_leg_percent={:?}, long_leg_percent={:?}",
            percent_exposure_short, percent_exposure_long
        );

        if percent_exposure > Decimal::from(49) {
            info!(
                "[MAIN][EXIT_CHECK] We are overexposed, change position, percent_exposure={}%",
                percent_exposure
            );
            change_position = true;
        } else if exit_arb.is_valid_after_fees {
            info!("[MAIN][EXIT_CHECK] Exit arbitrage is profitable, change position");
            change_position = true;
        } else if current_funding_diff > Decimal::from(25) {
            info!(
                "[MAIN][EXIT_CHECK] We have a good positions open, symbol={} funding_rate={}% dont change position",
                current_symbol.as_ref().unwrap(),
                current_funding_diff,
            );
            change_position = false;
        } else if pnl_cum > Decimal::ZERO {
            info!(
                "[MAIN][EXIT_CHECK] Pnl is profitable here, pnl={}, change position",
                pnl_cum
            );
            change_position = true;
        } else if current_funding_diff < Decimal::from(15) {
            info!(
                "[MAIN][EXIT_CHECK] Unfavourable funding rate on open position={}%, change position",
                current_funding_diff
            );
            change_position = true;
        } else {
            info!("[MAIN][EXIT_CHECK] Dont do anything funding positive or not negative yet, price not ready to exit, dont change position");
            change_position = false
        }

        info!(
            "[Main][EXIT_CHECK] Final decision, change_position={}",
            change_position
        );

        if change_position {
             exit(
                position_long,
                position_short,
                &exit_arb,
                long_market,
                short_market,
                &operator_map,
                &current_symbol.cloned(),
            )
            .await;

            let sleep_time = if env_config.mode == BotMode::Production {
                5 * 60
            } else {
                5 * 60
            };

            info!(
                "[MAIN] Sleeping to allow orders to fill, time={} seconds",
                sleep_time
            );
            info!(
                "[MAIN] Next trade scheduled at, time={:?}",
                Utc::now() + Duration::from_secs(sleep_time)
            );

            sleep(Duration::from_secs(sleep_time)).await;
        }
    }

    if !has_positions_open || change_position {
        enter(config, fetcher_a, fetcher_b, &operator_map).await
    }
}

async fn calc_pnl(
    exit_arb: &ExitArbitrage,
    position_long: &Position,
    position_short: &Position,
) -> Decimal {
    let spread_percent = (exit_arb.short_px_with_fees - exit_arb.long_px_with_fees)
        / exit_arb.short_px_with_fees
        * Decimal::from(100);

    let position_pnl_long_before_fees =
        exit_arb.long_px * position_long.size - position_long.entry_price * position_long.size;
    let position_pnl_short_before_fees =
        position_short.entry_price * position_short.size - exit_arb.short_px * position_short.size;
    let position_pnl_total_before_fees = position_pnl_long_before_fees + position_pnl_short_before_fees;
    info!("[PNL] Stats exit_spread_with_fees={:.4} pnl_total_before_fees={:.4}, pnl_long_before_fees={:.4}, pnl_short_before_fees={:.4}", spread_percent, position_pnl_total_before_fees, position_pnl_long_before_fees, position_pnl_short_before_fees);
    
    let position_pnl_long = exit_arb.long_px_with_fees * position_long.size
        - position_long.entry_price * position_long.size;
    let position_pnl_short = position_short.entry_price * position_short.size
        - exit_arb.short_px_with_fees * position_short.size;
    let position_pnl_total = position_pnl_long + position_pnl_short;

    info!(
        "[PNL] Position pnl: position_long={:.4}, position_short={:.4} position_total={:.4}",
         position_pnl_long, position_pnl_short, position_pnl_total
    );

    // Funding
    let funding_pnl_long = position_long.cum_funding.unwrap_or(Decimal::ZERO);
    let funding_pnl_short = position_short.cum_funding.unwrap_or(Decimal::ZERO);
    let funding_pnl_total = funding_pnl_long + funding_pnl_short;
    info!(
        "[PNL] Funding pnl: funding_long={:.4}, funding_short={:.4}, funding_total={:.4}",
        funding_pnl_long, funding_pnl_short, funding_pnl_total
    );

    let pnl_cum = position_pnl_total + funding_pnl_total;
    info!("[PNL] FINAL PNL + FUNDING pnl_cum={:.4}", pnl_cum);
    pnl_cum
}

async fn exit(
    position_a: &Position,
    position_b: &Position,
    exit_arb: &ExitArbitrage,
    long_market: &MarketInfo,
    short_market: &MarketInfo,
    operator_map: &HashMap<ExchangeName, &dyn Operator>,
    current_symbol: &Option<String>,
) {
    info!("Can exit position profitably, or funding is to negative and we kill");

    let long_exit_px = exit_arb.long_px;
    let short_exit_px = exit_arb.short_px;

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
        &position_a.size,
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
        &position_b.size,
        &short_exit_px,
        Some(true),
    )
    .await
    {
        Ok(order_result) => info!("Short order: {:?}", order_result),
        Err(e) => error!("Short order failed: {:?}", e),
    }
}

async fn enter(
    config: &BotJsonConfig,
    fetcher_a: &dyn Fetcher,
    fetcher_b: &dyn Fetcher,
    operator_map: &HashMap<ExchangeName, &dyn Operator>,
) {
    let mut min_available_balance = None;
    let mut opportunity = None;
    let max_attempts = 5;

    for _ in 0..max_attempts {
        let (_, _, balance, markets_a, markets_b) = get_trading_data(fetcher_a, fetcher_b).await;
        let arbitrage_opportunities =
            calculate_arbitrage_opportunities(&markets_a, &markets_b, &config).await;
        let op = get_valid_opportunity(&arbitrage_opportunities).await;

        min_available_balance = balance;
        opportunity = op.clone();

        if opportunity.is_some() {
            info!(
                "[MAIN] Arbitrage opportunity found, proceeding with trade, opportunity={:?}",
                opportunity
            );
            break;
        } else {
            info!("[MAIN] No valid arbitrage opportunities found, retrying after sleep...");
        }

        sleep(Duration::from_secs(30)).await;
    }

    if opportunity.is_some() {
        let op = opportunity.unwrap();
        info!(
            "[MAIN] Markets for arbitrage | Long on {} | Short on {}",
            op.long_market.exchange, op.short_market.exchange,
        );

        let long_operator = operator_map.get(&op.long_market.exchange).unwrap();
        let short_operator = operator_map.get(&op.short_market.exchange).unwrap();
        let qty = get_order_qty(
            &op.long_market,
            &op.short_market,
            min_available_balance.unwrap(),
        );

        if let Err(e) = set_same_leverage(
            &op.long_market,
            &op.short_market,
            *long_operator,
            *short_operator,
        )
        .await
        {
            error!(
                "[MAIN] Failed to set leverage before trading, error={:?}",
                e
            );
        }

        let entry_arbitrage = op.entry_arbitrage.as_ref().unwrap();
        let long_entry_px = entry_arbitrage.long_px;
        let short_entry_px = entry_arbitrage.short_px;

        info!(
            "[MAIN] Long entry, price={} exchange={}",
            long_entry_px,
            long_operator.get_exchange_info()
        );
        info!(
            "[MAIN] Short entry, price={} exchange={}",
            short_entry_px,
            short_operator.get_exchange_info()
        );
        info!(
            "[MAIN] Spread between prices, spread={}, %spread={}",
            long_entry_px - short_entry_px,
            (long_entry_px - short_entry_px) / short_entry_px * Decimal::from(100)
        );

        info!(
            "[MAIN] Estimated funding rate, funding_rate={}%",
            op.funding_diff
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
            Ok(order_result) => info!("[MAIN] Long order: {:?}", order_result),
            Err(e) => error!("[MAIN] Long order failed: {:?}", e),
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
            Ok(order_result) => info!("[MAIN] Short order: {:?}", order_result),
            Err(e) => error!("[MAIN] Short order failed: {:?}", e),
        }

        write_last_change(Utc::now().timestamp() as u64).unwrap_or_else(|e| {
            error!("[MAIN] Failed to write last change time: {:?}", e);
        });
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

    info!(
        "[MAIN] Maximum balance to trade, balance={:?}",
        min_available_balance
    );

    let markets_a = get_markets(fetcher_a).await;
    let markets_b = get_markets(fetcher_b).await;

    (
        positions_a,
        positions_b,
        min_available_balance,
        markets_a,
        markets_b,
    )
}

async fn get_account_data(fetcher: &dyn Fetcher) -> Option<AccountData> {
    fetcher
        .get_account_data()
        .await
        .map(Some)
        .unwrap_or_else(|e| {
            error!(
                "[MAIN] Failed to get account data, exchange={} error={:?}",
                fetcher.get_exchange_info(),
                e
            );
            None
        })
}

fn get_positions(account: &Option<AccountData>) -> Vec<Position> {
    account.as_ref().map_or(vec![], |a| a.positions.clone())
}

async fn get_markets(fetcher: &dyn Fetcher) -> Vec<MarketInfo> {
    let markets = fetcher.get_markets().await.unwrap_or_else(|e| {
        error!(
            "[MAIN] Failed to fetch markets, exchange={} error={:?}",
            fetcher.get_exchange_info(),
            e
        );
        vec![]
    });

    let markets: Vec<MarketInfo> = markets
        .into_iter()
        .filter(|m| !m.bid_price.is_zero() && !m.ask_price.is_zero())
        .collect();

    markets
}

async fn get_valid_opportunity(arb_ops: &[ArbitrageOpportunity]) -> Option<ArbitrageOpportunity> {
    if let Some(op) = arb_ops.first() {
        info!("[MAIN] First valid opportunity, opportunity={:?}", op);
    }

    let best_arbitrage_op = arb_ops
        .iter()
        .find(|op| {
            op.entry_arbitrage.as_ref().map_or(false, |ea| {
                ea.is_valid_after_fees && op.funding_diff > Decimal::from(25)
            })
        })
        .cloned();

    if let Some(ref op) = best_arbitrage_op {
        info!("[MAIN] First valid opportunity, opportunity={:?}", op);
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
            let funding_diff = (market_a.funding_rate - market_b.funding_rate).abs()
                * Decimal::from(24 * 365 * 100);

            let (long_market, short_market) = if market_a.funding_rate < market_b.funding_rate {
                (market_a, market_b)
            } else {
                (market_b, market_a)
            };

            let entry_arb = Some(get_entry_arb_data(
                long_market.clone(),
                short_market.clone(),
                config,
            ));

            let entry_arb = entry_arb.as_ref().unwrap();
            if entry_arb.is_valid_after_fees {
                info!(
                    "[MAIN] Found entry arbitrage opportunity, symbol={} long_exchange={} short_exchange={} funding_diff={}% long_px={} short_px={}",
                    market_a.symbol,
                    long_market.exchange,
                    short_market.exchange,
                    funding_diff,
                    entry_arb.long_px,
                    entry_arb.short_px
                );
            }

            opportunities.push(ArbitrageOpportunity {
                symbol: market_a.symbol.clone(),
                long_market: long_market.clone(),
                short_market: short_market.clone(),
                entry_arbitrage: Some(entry_arb.clone()),
                exit_arbitrage: None,
                funding_diff,
            });
        }
    }

    // Sort by best funding diff, descending
    opportunities.sort_by(|a, b| b.funding_diff.cmp(&a.funding_diff));

    info!(
        "[MAIN] Found arbitrage opportunities, count={}",
        opportunities.len()
    );

    opportunities
}

fn get_entry_arb_data(
    market_long: MarketInfo,
    market_short: MarketInfo,
    config: &BotJsonConfig,
) -> EntryArbitrage {
    let long_px = market_long.ask_price
        * (Decimal::ONE - BotJsonConfig::get_exit_offset(config, market_long.exchange));
    let short_px = market_short.bid_price
        * (Decimal::ONE + BotJsonConfig::get_exit_offset(config, market_short.exchange));

    let long_px_with_fees =
        long_px * (Decimal::ONE + BotJsonConfig::get_taker_fee(market_long.exchange));
    let short_px_with_fees =
        short_px * (Decimal::ONE - BotJsonConfig::get_taker_fee(market_short.exchange));

    let is_valid_before_fees = long_px < short_px;
    let is_valid_after_fees = long_px_with_fees < short_px_with_fees;

    EntryArbitrage {
        long_px,
        short_px,
        long_px_with_fees,
        short_px_with_fees,
        is_valid_before_fees,
        is_valid_after_fees,
    }
}

fn get_exit_arb_data(
    market_long: MarketInfo,
    market_short: MarketInfo,
    config: &BotJsonConfig,
) -> ExitArbitrage {
    let long_px = market_long.bid_price
        * (Decimal::ONE + BotJsonConfig::get_exit_offset(config, market_long.exchange));
    let short_px = market_short.ask_price
        * (Decimal::ONE - BotJsonConfig::get_exit_offset(config, market_short.exchange));

    let long_px_with_fees =
        long_px * (Decimal::ONE - BotJsonConfig::get_taker_fee(market_long.exchange));
    let short_px_with_fees =
        short_px * (Decimal::ONE + BotJsonConfig::get_taker_fee(market_short.exchange));

    let is_valid_before_fees = long_px > short_px;
    let is_valid_after_fees = long_px_with_fees > short_px_with_fees;

    ExitArbitrage {
        long_px,
        short_px,
        long_px_with_fees,
        short_px_with_fees,
        is_valid_before_fees,
        is_valid_after_fees,
    }
}

fn get_order_qty(
    long_market: &MarketInfo,
    short_market: &MarketInfo,
    min_available_balance: Decimal,
) -> Decimal {
    let leverage = long_market.leverage.min(short_market.leverage);

    let szi_decimals = long_market.sz_decimals.min(short_market.sz_decimals);
    let szi_increment = long_market
        .min_order_size_change
        .max(short_market.min_order_size_change);
    let szi_increment_dp = szi_increment.normalize().scale();

    let max_amount_long = (min_available_balance * leverage) / long_market.ask_price;
    let max_amount_short = (min_available_balance * leverage) / short_market.bid_price;
    let max_amount = max_amount_long.min(max_amount_short);

    let amount = max_amount
        .round_dp_with_strategy(szi_decimals.to_u32().unwrap_or(0), RoundingStrategy::ToZero);
    let increments = (amount / szi_increment).to_u128().unwrap_or(0);
    let quantized_amount = (Decimal::from_u128(increments).unwrap_or(Decimal::ZERO)
        * szi_increment)
        .round_dp(szi_increment_dp);

    info!(
        "Calculating quantized amount, szi_decimals={} szi_increment={:.8} max_amount={:.8} amount={:.8} quantized_amount={:.8}",
        szi_decimals,
        szi_increment,
        max_amount,
        amount,
        quantized_amount
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

    let res_long = long_operator
        .change_leverage(long_market.clone(), min_leverage)
        .await;
    let res_short = short_operator
        .change_leverage(short_market.clone(), min_leverage)
        .await;

    res_long.and(res_short).map_err(|e| e)
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
        "[MAIN] Placing order, exchange={} order={:?}",
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
) -> anyhow::Result<()> {
    let label = operator.get_exchange_info();

    if positions.is_empty() {
        info!("[MAIN] No positions to close, exchange={}", label);
        return Ok(());
    } else {
        info!(
            "[MAIN] Closing positions, count={} exchange={}",
            positions.len(),
            label
        );
    }

    for position in positions {
        let Some(market) = markets.iter().find(|m| m.symbol == position.symbol) else {
            error!(
                "[MAIN] Market data missing for symbol={} exchange={}",
                position.symbol, label
            );
            continue;
        };

        let (side, price) = if position.side == PositionSide::Long {
            (PositionSide::Short, market.bid_price)
        } else {
            (PositionSide::Long, market.ask_price)
        };

        if let Err(e) = create_order(
            operator,
            market,
            side,
            &position.size.abs(),
            &price,
            Some(true),
        )
        .await
        {
            error!(
                "[MAIN] Failed to close position, symbol={} exchange={} error={:?}",
                position.symbol, label, e
            );
        } else {
            info!(
                "[MAIN] Closed position, symbol={} exchange={}",
                position.symbol, label
            );
        }
    }

    Ok(())
}

fn get_market_for_position<'a>(
    position: &Position,
    markets: &'a [MarketInfo],
) -> Option<&'a MarketInfo> {
    markets.iter().find(|m| m.symbol == position.symbol)
}
