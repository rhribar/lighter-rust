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
    entry_arb_valid_after_fees_and_funding_ok: bool,
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
        // 0 20 * * * * *

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

    let mut operator_map: HashMap<ExchangeName, &dyn Operator> = HashMap::new();
    operator_map.insert(operator_a.get_exchange_info(), operator_a);
    operator_map.insert(operator_b.get_exchange_info(), operator_b);

    let (positions_a, positions_b, _, markets_a, markets_b) = get_trading_data(fetcher_a, fetcher_b).await;

    let has_positions_open = positions_a.len() > 0 || positions_b.len() > 0;
    let mut change_position = true;

    if has_positions_open {
        let current_symbol = if positions_a.len() > 0 {
            positions_a.first().map(|p| &p.symbol)
        } else {
            positions_b.first().map(|p| &p.symbol)
        };

        let position_a = positions_a.iter().find(|p| p.symbol == *current_symbol.unwrap())
            .expect("Could not find position_a for symbol");
        let position_b = positions_b.iter().find(|p| p.symbol == *current_symbol.unwrap())
            .expect("Could not find position_b for symbol");

        let (position_long, position_short, markets_long, markets_short) =
            if position_a.side == PositionSide::Long {
                (position_a, position_b, &markets_a, &markets_b)
            } else {
                (position_b, position_a, &markets_b, &markets_a)
            };

        // smth wrong kill all, circuit out
        if positions_a.len() != 1 || positions_b.len() != 1 {
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

        let long_market = get_market_for_position(position_long, &markets_long).unwrap();
        let short_market = get_market_for_position(position_short, &markets_short).unwrap();

        let arbitrage_opportunities = calculate_arbitrage_opportunities(&markets_a, &markets_b, config).await;

        let current_op_data = arbitrage_opportunities.iter().find(|op| &op.symbol == current_symbol.unwrap()).unwrap();
        let current_funding_diff_ann = if long_market.exchange == current_op_data.long_market.exchange {
            current_op_data.funding_diff * Decimal::from(24 * 365 * 100)
        } else {
            -current_op_data.funding_diff * Decimal::from(24 * 365 * 100)
        };

        info!(
            "Current open position symbol: {:?}, funding diff ann: {:?}%",
            current_symbol, current_funding_diff_ann
        );

        if let Some(new_op) = get_first_op(&arbitrage_opportunities).await {
            let new_symbol = &new_op.symbol;
            let new_funding_diff_ann = new_op.funding_diff * Decimal::from(24 * 365 * 100);

            info!(
                "New ops exists symbol: {:?}, funding diff ann: {:?}%",
                new_symbol, new_funding_diff_ann
            );
        }

        let exit_arb = Some(get_exit_arb_data(long_market.clone(), short_market.clone(), config));
        let exit_arb = exit_arb.as_ref().unwrap();

        let pnl_cum = calc_pnl(exit_arb, position_long, position_short).await;

        let leverage = current_op_data.long_market.leverage.min(current_op_data.short_market.leverage);

        let long_exit_px = exit_arb.long_exit_px;
        let short_exit_px = exit_arb.short_exit_px;

        info!(
            "Current symbol exit arbitrage data: Long Exit Px: {}, Short Exit Px: {}",
            long_exit_px, short_exit_px
        );

        info!("Leverage: {}", leverage);

        let percent_exposure_long = ((long_exit_px / position_long.entry_price) - Decimal::ONE) * leverage * Decimal::from(100); // in percent
        let percent_exposure_short = (Decimal::ONE - (short_exit_px / position_short.entry_price)) * leverage * Decimal::from(100);

        let percent_exposure = percent_exposure_long.max(percent_exposure_short);

        info!("Long entry price: {} | Current long exit px: {} | Percent exposure long: {}", position_long.entry_price, long_exit_px, percent_exposure_long);
        info!("Short entry price: {} | Current short exit px: {} | Percent exposure short: {}", position_short.entry_price, short_exit_px, percent_exposure_short);

        info!("Overall percent exposure to liquidation: {}%", percent_exposure);

        if percent_exposure > Decimal::from(49) {
            info!("EXIT CHECK: Position is too close to liquidation: {}%, change position", percent_exposure);
            change_position = true;
        } else if exit_arb.exit_arb_valid_after_fees {
            info!("EXIT CHECK: Our exit is profitable after fees, change position");
            change_position = true;
        } else if current_funding_diff_ann > Decimal::from(25) {
            info!(
                "EXIT CHECK: We are still getting paid funding {} on {}: dont change position",
                current_funding_diff_ann,
                current_symbol.as_ref().unwrap()
            );
            change_position = false;
        } else if pnl_cum > Decimal::ZERO {
            info!("Calculated PnL including funding is positive: {}", pnl_cum);
            change_position = true;
        } else if current_funding_diff_ann < Decimal::from(-10) {
            info!(
                "EXIT CHECK: Current position funding ann is very negative {}: change position",
                current_funding_diff_ann
            );
            change_position = true;
        } else {
            info!("EXIT CHECK: Dont do anything funding positive, price not ready to exit, dont change position");
            change_position = false
        }

        info!(
            "Trade Execution | Has Positions Open: {} | Change Position: {}",
            has_positions_open, change_position
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

            info!("Sleeping for {} seconds to allow orders to fill and close", sleep_time);
            info!(
                "Next trade scheduled at: {:?}",
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
    // when we exit a short we buy, so we would rather buy lower, so when we come here, we know that price is not good, and we know this will be negative, thats why buy is first (or short exit)
    // this includes spread and taker fees here

    // check cum funding, call for each exchange
    // check unrealized pnl for each position

    let spread_percent = (exit_arb.short_exit_px_wfees - exit_arb.long_exit_px_wfees)
        / exit_arb.short_exit_px_wfees
        * Decimal::from(100);

    info!("Exit Spread Percent: {}", spread_percent);

    let position_long_1 = position_long;
    let position_short_1 = position_short;
    let pnl_long =
        exit_arb.long_exit_px_wfees * position_long_1.size - position_long_1.entry_price * position_long_1.size;

    let pnl_short =
        position_short_1.entry_price * position_short_1.size - exit_arb.short_exit_px_wfees * position_short_1.size;

    let pnl_long_before_fees =
        exit_arb.long_exit_px * position_long_1.size - position_long_1.entry_price * position_long_1.size;

    let pnl_short_before_fees =
        position_short_1.entry_price * position_short_1.size - exit_arb.short_exit_px * position_short_1.size;

    let pnl_calc_before_fees = pnl_long_before_fees + pnl_short_before_fees;

    info!(
        "Calculated PnL before fees: {} pnl_long_before_fees {} pnl_short_before_fees {}",
        pnl_calc_before_fees, pnl_long_before_fees, pnl_short_before_fees
    );

    let pnl_calc = pnl_long + pnl_short;
    info!(
        "Calculated PnL: {} pnl_long {} pnl_short {}",
        pnl_calc, pnl_long, pnl_short
    );

    let funding_1 = position_long_1.cum_funding.unwrap_or(Decimal::ZERO);
    let funding_2 = position_short_1.cum_funding.unwrap_or(Decimal::ZERO);

    info!("Funding long: {} Funding short: {}", funding_1, funding_2);

    let pnl_funding = funding_1 + funding_2;

    info!("Calculated funding: {}", pnl_funding);

    let pnl_cum = pnl_calc + pnl_funding;
    info!("Calculated Cum PnL: {}", pnl_cum);

    return pnl_cum;
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
    let mut first_op = None;
    let max_attempts = 5;

    for _ in 0..max_attempts {
        let (_, _, balance, markets_a, markets_b) = get_trading_data(fetcher_a, fetcher_b).await;
        let arbitrage_opportunities = calculate_arbitrage_opportunities(&markets_a, &markets_b, &config).await;
        let op = get_first_op(&arbitrage_opportunities).await;

        min_available_balance = balance;
        first_op = op.clone();

        if first_op.is_some() {
            info!("Arbitrage opportunity found, proceeding with trade {:?}", first_op);
            break;
        } else {
            info!("No arbitrage opportunities found, retrying...");
        }

        info!("Waiting before next arb check...");
        sleep(Duration::from_secs(30)).await;
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

        if let Err(e) = set_same_leverage(&op.long_market, &op.short_market, *long_operator, *short_operator).await {
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

        info!("Estimated funding diff ann: {}%", op.funding_diff * Decimal::from(24 * 365 * 100));

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
            "First funding op: Symbol: {}, Long Exchange: {}, Short Exchange: {}, Funding Diff: {}, Long Entry Px (w/fees): {}, Short Entry Px (w/fees): {}, Arb Valid Before Fees: {}, Arb Valid After Fees: {}, Arb Valid After Fees and Funding OK: {}",
            op.symbol,
            op.long_market.exchange,
            op.short_market.exchange,
            op.funding_diff * Decimal::from(24 * 365 * 100),
            entry_arbitrage.long_entry_px_wfees,
            entry_arbitrage.short_entry_px_wfees,
            entry_arbitrage.entry_arb_valid_before_fees,
            entry_arbitrage.entry_arb_valid_after_fees,
            entry_arbitrage.entry_arb_valid_after_fees_and_funding_ok
        );
    }

    let best_arbitrage_op = arb_ops
        .iter()
        .find(|op| {
            op.entry_arbitrage
                .as_ref()
                .unwrap()
                .entry_arb_valid_after_fees_and_funding_ok
        })
        .cloned();

    if let Some(op) = best_arbitrage_op.clone() {
        let entry_arbitrage = op.entry_arbitrage.unwrap();

        info!(
            "First valid arbitrage op: Symbol: {}, Long Exchange: {}, Short Exchange: {}, Funding Diff: {}, Long Entry Px (w/fees): {}, Short Entry Px (w/fees): {}, Arb Valid Before Fees: {}, Arb Valid After Fees: {}, Arb Valid After Fees and Funding OK: {}",
            op.symbol,
            op.long_market.exchange,
            op.short_market.exchange,
            op.funding_diff * Decimal::from(24 * 365 * 100),
            entry_arbitrage.long_entry_px_wfees,
            entry_arbitrage.short_entry_px_wfees,
            entry_arbitrage.entry_arb_valid_before_fees,
            entry_arbitrage.entry_arb_valid_after_fees,
            entry_arbitrage.entry_arb_valid_after_fees_and_funding_ok
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

    let long_entry_px = long_entry_px * (Decimal::ONE - BotJsonConfig::get_exit_offset(config, market_long.exchange)); // long entry we want to buy cheaper
    let short_entry_px =
        short_entry_px * (Decimal::ONE + BotJsonConfig::get_exit_offset(config, market_short.exchange)); // short entry we want to sell higher

    let long_entry_px_wfees = long_entry_px * (Decimal::ONE + BotJsonConfig::get_taker_fee(market_long.exchange));
    let short_entry_px_wfees = short_entry_px * (Decimal::ONE - BotJsonConfig::get_taker_fee(market_short.exchange));

    let entry_arb_valid_before_fees = long_entry_px < short_entry_px;

    let entry_arb_valid_after_fees = long_entry_px_wfees < short_entry_px_wfees;

    let entry_funding_diff =
        (market_long.funding_rate - market_short.funding_rate).abs() * Decimal::from(24 * 365 * 100);

    let entry_arb_valid_after_fees_and_funding_ok =
        entry_arb_valid_after_fees && entry_funding_diff > Decimal::from(20);
    if entry_arb_valid_before_fees {
        info!(
            "ENTRY ARB VALID BEFORE FEES: {} | Long Market: {} | Short Market: {} | Long Entry Price: {} | Short Entry Price: {} | spread {} | spread% {}% | my check: {}  my check with fees: {} long_entry_px_wfees {} short_entry_px_wfees {} entry_funding_diff {} entry_arb_valid_after_fees_and_funding_ok {}",
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
            short_entry_px_wfees,
            entry_funding_diff,
            entry_arb_valid_after_fees_and_funding_ok
        );
    }

    EntryArbitrage {
        long_entry_px,
        short_entry_px,
        long_entry_px_wfees,
        short_entry_px_wfees,
        entry_arb_valid_before_fees,
        entry_arb_valid_after_fees,
        entry_arb_valid_after_fees_and_funding_ok,
    }
}

fn get_exit_arb_data(market_long: MarketInfo, market_short: MarketInfo, config: &BotJsonConfig) -> ExitArbitrage {
    let long_exit_px = market_long.bid_price; // this is not mid, this is next price a seller is willing to buy from us
    let short_exit_px = market_short.ask_price; // this is not mid, this is next price a buyer is willing to sell to us

    let long_exit_px = long_exit_px * (Decimal::ONE + BotJsonConfig::get_exit_offset(config, market_long.exchange)); // long exit we want to sell higher
    let short_exit_px = short_exit_px * (Decimal::ONE - BotJsonConfig::get_exit_offset(config, market_short.exchange)); // short exit we want to buy cheaper, because its better for us (for pnl)

    let long_exit_px_wfees = long_exit_px * (Decimal::ONE - BotJsonConfig::get_taker_fee(market_long.exchange));
    let short_exit_px_wfees = short_exit_px * (Decimal::ONE + BotJsonConfig::get_taker_fee(market_short.exchange));

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

    let res_long = long_operator.change_leverage(long_market.clone(), min_leverage).await;
    let res_short = short_operator.change_leverage(short_market.clone(), min_leverage).await;

    res_long.and(res_short).map_err(|e| e)
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
