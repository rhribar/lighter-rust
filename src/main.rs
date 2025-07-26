use anyhow::Result;
use log::{error, info};
use rust_decimal::{prelude::FromPrimitive, Decimal};

use points_bot_rs::{
    fetchers::{AccountData, Fetcher, FetcherExtended, FetcherHyperliquid, MarketInfo, Position},
    operators::{create_operator_hyperliquid, Operator, OperatorExtended, OrderRequest, OrderResponse, OrderType},
    BotConfig, PointsBotResult, PositionSide
};

#[tokio::main]
async fn main() -> Result<()> {
    use ethers::signers::LocalWallet;
    use std::str::FromStr;
    env_logger::init();

    let config = BotConfig::from_env().unwrap_or_else(|e| {
        panic!("Failed to load configuration: {}", e);
    });

    info!("Starting Points Bot - Creating single order");
    info!("Trading mode: {}", if config.trading_mode == "live" { "LIVE" } else { "SIMULATION" });
    info!("Wallet address: {}", config.wallet_address);

    let fetcher_hyperliquid = Box::new(FetcherHyperliquid::new());
    let fetcher_extended = Box::new(FetcherExtended::new());

    let private_key = config.hyperliquid_private_key.as_deref().expect("Missing hyperliquid_private_key in config");
    let wallet = LocalWallet::from_str(private_key).ok().expect("Failed to create wallet from private key");
    let operator_hyperliquid = create_operator_hyperliquid(wallet).await;
    let operator_extended = Box::new(OperatorExtended::new().await);

    let (hyperliquid_account, extended_account) = get_all_account_data(
        fetcher_hyperliquid.as_ref(),
        fetcher_extended.as_ref(),
        &config.wallet_address
    ).await;

    let min_available_balance = match (&hyperliquid_account, &extended_account) {
        (Some(hyper_account), Some(ext_account)) => {
            let min_balance = hyper_account.available_balance.min(ext_account.available_balance);
            info!("Minimum Available Balance: ${:.2}", min_balance);
            Some(min_balance)
        }
        _ => None,
    };

    let (hyperliquid_markets, extended_markets) = get_all_markets(
        fetcher_hyperliquid.as_ref(),
        fetcher_extended.as_ref()
    ).await;

    let hyperliquid_positions: Vec<Position> = hyperliquid_account.as_ref().map_or(vec![], |a| a.positions.clone());
    let extended_positions: Vec<Position> = extended_account.as_ref().map_or(vec![], |a| a.positions.clone());

    if !hyperliquid_positions.is_empty() || !extended_positions.is_empty() {
        close_all_open_positions(
            &hyperliquid_positions,
            &extended_positions,
            &hyperliquid_markets,
            &extended_markets,
            operator_hyperliquid.as_ref(),
            operator_extended.as_ref(),
        ).await;
    } else {
        info!("No open positions to close, must be first trade, continue.");
    }

    let arbitrage_opportunities = get_arbitrage_opportunities_from_markets(
        &hyperliquid_markets,
        &extended_markets,
    ).await;

    if let Some((ref symbol, hyper_rate, ext_rate, _, _, _)) = arbitrage_opportunities.first() {
        let hyper_market = hyperliquid_markets.iter().find(|m| m.symbol == *symbol).unwrap();
        let ext_market = extended_markets.iter().find(|m| m.symbol == *symbol).unwrap();

        let (amount, long_market, short_market, long_operator, short_operator) = calculate_trade_attributes(
            *hyper_rate,
            *ext_rate,
            hyper_market,
            ext_market,
            operator_hyperliquid.as_ref(),
            operator_extended.as_ref(),
            min_available_balance.unwrap()
        );

        set_same_leverage(
            symbol.to_string(),
            hyper_market,
            ext_market,
            operator_hyperliquid.as_ref(),
            operator_extended.as_ref(),
        )
        .await?;

        let (price_adjusted_long, _) = get_adjusted_price_and_side(long_market, &PositionSide::Long, false).await;

        info!("Adjusted price for long order: {}", price_adjusted_long);

        let (price_adjusted_short, _) = get_adjusted_price_and_side(short_market, &PositionSide::Short, false).await;

        info!("Adjusted price for short order: {}", price_adjusted_short);

        match create_order(short_operator, symbol, PositionSide::Short, &amount, &price_adjusted_short, Some(false)).await {
            Ok(order_result) => {
                info!("Order executed successfully!");
                info!("Order ID: {}", order_result.id);
                info!("Symbol: {}", order_result.symbol);
                info!("Side: {:?}", order_result.side);
                info!("Status: {}", order_result.status.as_str());
                info!("Filled Quantity: {}", order_result.filled_quantity);
                info!("Remaining Quantity: {}", order_result.remaining_quantity);

                if let Some(avg_price) = order_result.average_price {
                    info!("Average Price: {}", avg_price);
                }
            }
            Err(e) => {
                error!("Failed to place short order: {:?}", e);
            }
        } 

        match create_order(long_operator, symbol, PositionSide::Long, &amount, &price_adjusted_long, Some(false)).await {
            Ok(order_result) => {
                info!("Long order placed: {:?}", order_result);
                info!("Order ID: {}", order_result.id);
                info!("Symbol: {}", order_result.symbol);
                info!("Side: {:?}", order_result.side);
                info!("Status: {}", order_result.status.as_str());
                info!("Filled Quantity: {}", order_result.filled_quantity);
                info!("Remaining Quantity: {}", order_result.remaining_quantity);

                if let Some(avg_price) = order_result.average_price {
                    info!("Average Price: {}", avg_price);
                }
            }
            Err(e) => {
                error!("Failed to place long order: {:?}", e);
            }
        }
    } else {
        error!("There are no arbitrage opportunities available at the moment.");
    }

    Ok(())
}

async fn get_all_account_data(fetcher_hyperliquid: &dyn Fetcher, fetcher_extended: &dyn Fetcher, wallet_address: &str) -> (Option<AccountData>, Option<AccountData>) {
    let hyperliquid_result = fetcher_hyperliquid.get_account_data(wallet_address).await;
    let extended_result = fetcher_extended.get_account_data(wallet_address).await;

    match (hyperliquid_result, extended_result) {
        (Ok(hyperliquid_account), Ok(extended_account)) => {
            info!("Hyperliquid Positions: {:?}", hyperliquid_account.positions);
            info!("Extended Positions: {:?}", extended_account.positions);

            (Some(hyperliquid_account), Some(extended_account))
        }
        (Err(e), Ok(extended_account)) => {
            error!("Failed to get account data from Hyperliquid: {:?}", e);
            (None, Some(extended_account))
        }
        (Ok(hyperliquid_account), Err(e)) => {
            error!("Failed to get account data from Extended: {:?}", e);
            (Some(hyperliquid_account), None)
        }
        (Err(e1), Err(e2)) => {
            error!("Failed to get account data from both sources: {:?}, {:?}", e1, e2);
            (None, None)
        }
    }
}

async fn close_all_open_positions(
    hyperliquid_positions: &[Position],
    extended_positions: &[Position],
    hyperliquid_markets: &[MarketInfo],
    extended_markets: &[MarketInfo],
    operator_hyperliquid: &dyn Operator,
    operator_extended: &dyn Operator,
) {
    if !hyperliquid_positions.is_empty() || !extended_positions.is_empty() {
        log::info!("Closing existing positions...");

        for position in hyperliquid_positions {
            if let Some(market) = hyperliquid_markets.iter().find(|m: &&MarketInfo| m.symbol == position.symbol) {
                let (price_adjusted, side) = get_adjusted_price_and_side(market, &position.side, true).await;

                match create_order(operator_hyperliquid, &position.symbol, side, &position.size.abs(), &price_adjusted, Some(true)).await {
                    Ok(order_result) => {
                        log::info!("Closed Hyperliquid position: {:?}", order_result);
                    }
                    Err(e) => {
                        log::error!("Failed to close Hyperliquid position: {:?}", e);
                    }
                }
            } else {
                log::error!("Market data not found for symbol: {}", position.symbol);
            }
        }

        for position in extended_positions {
            if let Some(market) = extended_markets.iter().find(|m| m.symbol == position.symbol) {
                let (price_adjusted, side) = get_adjusted_price_and_side(market, &position.side, true).await;

                match create_order(operator_extended, &position.symbol, side, &position.size, &price_adjusted, Some(true)).await {
                    Ok(order_result) => {
                        log::info!("Closed Extended position: {:?}", order_result);
                    }
                    Err(e) => {
                        log::error!("Failed to close Extended position: {:?}", e);
                    }
                }
            } else {
                log::error!("Market data not found for symbol: {}", position.symbol);
            }
        }
    } else {
        log::info!("No positions available to close.");
    }
}

async fn create_order(
    operator: &dyn Operator,
    symbol: &str,
    side: PositionSide,
    quantity: &Decimal,
    price: &Decimal,
    reduce_only: Option<bool>
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

    info!("Creating order: {:?}", order_request);
    info!("Using exchange: {}", operator.get_exchange_name().as_str());

    let result = operator.create_order(order_request).await?;

    info!("Order created successfully: {:?}", result);

    Ok(result)
}

async fn get_all_markets(fetcher_hyperliquid: &dyn Fetcher, fetcher_extended: &dyn Fetcher) -> (Vec<MarketInfo>, Vec<MarketInfo>) {
    let hyperliquid_markets = fetcher_hyperliquid.get_markets().await.unwrap_or_else(|e| {
        log::error!("Failed to fetch Hyperliquid markets: {:?}", e);
        vec![]
    });

    let extended_markets = fetcher_extended.get_markets().await.unwrap_or_else(|e| {
        log::error!("Failed to fetch Extended markets: {:?}", e);
        vec![]
    });

    (hyperliquid_markets, extended_markets)
}

async fn get_arbitrage_opportunities_from_markets(
    hyperliquid_markets: &[MarketInfo],
    extended_markets: &[MarketInfo],
) -> Vec<(String, Decimal, Decimal, Decimal, Decimal, Decimal)> {
    let mut arbitrage_opportunities = Vec::new();

    for hyper_market in hyperliquid_markets {
        if let Some(ext_market) = extended_markets.iter().find(|r| r.symbol == hyper_market.symbol) {
            let diff = (hyper_market.funding_rate - ext_market.funding_rate).abs();
            arbitrage_opportunities.push((
                hyper_market.symbol.clone(),
                hyper_market.funding_rate,
                ext_market.funding_rate,
                diff,
                hyper_market.leverage,
                ext_market.leverage,
            ));
        }
    }

    arbitrage_opportunities.sort_by(|a, b| b.3.cmp(&a.3));

    info!("Found {} arbitrage opportunities", arbitrage_opportunities.len());

    if let Some((symbol, hyper_rate, ext_rate, diff, hyper_leverage, ext_leverage)) = arbitrage_opportunities.first() {
        info!(
            "Symbol: {}, Hyperliquid Rate: {}, Extended Rate: {}, Difference: {}, Hyper Leverage: {}, Extended Leverage: {}",
            symbol, hyper_rate, ext_rate, diff * Decimal::from(24 * 365 * 100), hyper_leverage, ext_leverage
        );
    }

    /*  info!("Arbitrage Opportunities:");
        for (symbol, hyper_rate, ext_rate, diff, hyper_leverage, ext_leverage) in &arbitrage_opportunities {
            info!(
                "Symbol: {}, Hyperliquid Rate: {}, Extended Rate: {}, Difference: {}, Hyper Leverage: {}, Extended Leverage: {}",
                symbol, hyper_rate, ext_rate, diff * Decimal::from(24 * 365 * 100), hyper_leverage, ext_leverage
            );
        } */

    /*  info!("All Hyperliquid Rates: {:?}", hyperliquid_rates);
        info!("All Extended Rates: {:?}", extended_rates);
        info!("All Arbitrage Opportunities: {:?}", arbitrage_opportunities); */

    arbitrage_opportunities
}

async fn get_adjusted_price_and_side(
    market: &MarketInfo,
    side: &PositionSide,
    close: bool,
) -> (Decimal, PositionSide) {
    let bips_offset = Decimal::from_f64(0.005).unwrap(); // 50bips
    let scale_ask = market.ask_price.scale();
    let scale_bid = market.bid_price.scale();

    match side {
        PositionSide::Long => {
            if close {
                ((market.ask_price * (Decimal::ONE + bips_offset)).round_dp(scale_ask), PositionSide::Short)
            } else {
                ((market.ask_price * (Decimal::ONE - bips_offset)).round_dp(scale_ask), PositionSide::Long)
            }
        }
        PositionSide::Short => {
            if close {
                ((market.bid_price * (Decimal::ONE - bips_offset)).round_dp(scale_bid), PositionSide::Long)
            } else {
                ((market.bid_price * (Decimal::ONE + bips_offset)).round_dp(scale_bid), PositionSide::Short)
            }
        }
    }
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
        operator_hyperliquid
            .change_leverage(symbol, min_leverage)
            .await
            .map_err(|e| e)
    } else {
        operator_extended
            .change_leverage(symbol, min_leverage)
            .await
            .map_err(|e| e)
    }
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
    let (long_market, short_market, long_operator, short_operator) = if hyper_rate > ext_rate {
        (hyper_market, ext_market, operator_hyperliquid, operator_extended)
    } else {
        (ext_market, hyper_market, operator_extended, operator_hyperliquid)
    };

    let amount_long = min_available_balance / long_market.ask_price;
    let amount_short = min_available_balance / short_market.bid_price;
    let min_order_size = long_market.min_order_size.unwrap_or(Decimal::ZERO).max(short_market.min_order_size.unwrap_or(Decimal::ZERO));
    let amount = (amount_long.min(amount_short).max(min_order_size) / min_order_size).floor() * min_order_size;

    (amount, long_market, short_market, long_operator, short_operator)
}