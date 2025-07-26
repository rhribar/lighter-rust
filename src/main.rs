use anyhow::Result;
use log::{error, info};
use rust_decimal::{prelude::FromPrimitive, Decimal};

use points_bot_rs::{
    fetchers::{Fetcher, FetcherExtended, FetcherHyperliquid, MarketInfo, Position},
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

    let mut min_available_balance = None;
    let mut arbitrage_opportunities = Vec::new();

    // Fetch account data
    let hyperliquid_result = fetcher_hyperliquid.get_account_data(&config.wallet_address).await;
    let extended_result = fetcher_extended.get_account_data(&config.wallet_address).await;

    let (hyperliquid_account, extended_account) = match (hyperliquid_result, extended_result) {
        (Ok(hyperliquid_account), Ok(extended_account)) => {
            min_available_balance = Some(hyperliquid_account.available_balance.min(extended_account.available_balance));
            info!("Minimum Available Balance: ${:.2}", min_available_balance.unwrap());
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
    };

    let mut hyperliquid_markets: Vec<MarketInfo> = Vec::new();
    let mut extended_markets: Vec<MarketInfo> = Vec::new();

    // Fetch funding rates
    match (fetcher_hyperliquid.get_markets().await, fetcher_extended.get_markets().await) {
        (Ok(hyperliquid_m), Ok(extended_m)) => {
            info!("Fetched {} funding rates from Hyperliquid", hyperliquid_m.len());
            info!("Fetched {} funding rates from Extended", extended_m.len());
            hyperliquid_markets = hyperliquid_m.clone();
            extended_markets = extended_m.clone();

            let hyperliquid_positions: Vec<Position> = hyperliquid_account.as_ref().map_or(vec![], |a| a.positions.clone());
            let extended_positions: Vec<Position> = extended_account.as_ref().map_or(vec![], |a| a.positions.clone());

            if !hyperliquid_positions.is_empty() || !extended_positions.is_empty() {
                info!("Closing existing positions...");

                for position in &hyperliquid_positions {
                    if let Some(market) = hyperliquid_markets.iter().find(|m| m.symbol == position.symbol) {
                        let bips_offset = Decimal::from_f64(0.005).unwrap();
                        let (price_adjusted, side) = match position.side {
                            PositionSide::Long => (market.ask_price * (Decimal::ONE + bips_offset), PositionSide::Short),
                            PositionSide::Short => (market.bid_price * (Decimal::ONE - bips_offset), PositionSide::Long),
                        };

                        // Limit the number of decimals to match the original price
                        let original_decimals = market.ask_price.scale();
                        let price_adjusted = price_adjusted.round_dp(original_decimals);

                        match create_order(operator_hyperliquid.as_ref(), &position.symbol, side, &position.size.abs(), &price_adjusted, Some(true)).await {
                            Ok(order_result) => {
                                info!("Closed Hyperliquid position: {:?}", order_result);
                            }
                            Err(e) => {
                                error!("Failed to close Hyperliquid position: {:?}", e);
                            }
                        }
                    } else {
                        error!("Market data not found for symbol: {}", position.symbol);
                    }
                }

                for position in &extended_positions {
                    if let Some(market) = extended_markets.iter().find(|m| m.symbol == position.symbol) {
                        let bips_offset = Decimal::from_f64(0.005).unwrap();
                        let (price_adjusted, side) = match position.side {
                            PositionSide::Long => (market.ask_price * (Decimal::ONE + bips_offset), PositionSide::Short),
                            PositionSide::Short => (market.bid_price * (Decimal::ONE - bips_offset), PositionSide::Long),
                        };

                        let original_decimals = market.ask_price.scale();
                        let price_adjusted = price_adjusted.round_dp(original_decimals);

                        match create_order(operator_extended.as_ref(), &position.symbol, side, &position.size, &price_adjusted, Some(true)).await {
                            Ok(order_result) => {
                                info!("Closed Extended position: {:?}", order_result);
                            }
                            Err(e) => {
                                error!("Failed to close Extended position: {:?}", e);
                            }
                        }
                    } else {
                        error!("Market data not found for symbol: {}", position.symbol);
                    }
                }
            } else {
                info!("No positions available to close.");
            }

            for hyper_market in &hyperliquid_m {
                if let Some(ext_market) = extended_m.iter().find(|r| r.symbol == hyper_market.symbol) {
                    let diff = (hyper_market.funding_rate - ext_market.funding_rate).abs();
                    arbitrage_opportunities.push((hyper_market.symbol.clone(), hyper_market.funding_rate, ext_market.funding_rate, diff, hyper_market.leverage, ext_market.leverage));
                }
            }

            arbitrage_opportunities.sort_by(|a, b| b.3.cmp(&a.3));

            info!("Found {} arbitrage opportunities", arbitrage_opportunities.len());

            arbitrage_opportunities.first().map(|(symbol, hyper_rate, ext_rate, diff, hyper_leverage, ext_leverage)| {
                info!(
                    "Symbol: {}, Hyperliquid Rate: {}, Extended Rate: {}, Difference: {}, Hyper Leverage: {}, Extended Leverage: {}",
                    symbol, hyper_rate, ext_rate, diff * Decimal::from(24 * 365 * 100), hyper_leverage, ext_leverage
                );
            });
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
        }
        (Err(e), _) => {
            error!("Failed to get funding rates from Hyperliquid: {:?}", e);
        }
        (_, Err(e)) => {
            error!("Failed to get funding rates from Extended: {:?}", e);
        }
    }

    if let (Some(min_balance), Some(&(ref symbol, hyper_rate, ext_rate, _, _, _))) = (
        min_available_balance,
        arbitrage_opportunities.first(),
    ) {
        if let (Some(hyper_market), Some(ext_market)) = (
            hyperliquid_markets.iter().find(|m| m.symbol == *symbol),
            extended_markets.iter().find(|m| m.symbol == *symbol),
        ) {
            // Calculate the amount based on ask prices and min_available_balance
            let amount_hyper = min_balance / hyper_market.ask_price;
            let amount_ext = min_balance / ext_market.ask_price;
            let min_order_size = hyper_market.min_order_size.unwrap_or(Decimal::ZERO).max(ext_market.min_order_size.unwrap_or(Decimal::ZERO));
            if min_order_size.is_zero() {
                info!("No minimum order size specified for either market. Proceeding with the order.");
            }
            let amount = (amount_hyper.min(amount_ext).max(min_order_size) / min_order_size).floor() * min_order_size;

            // Take the minimum leverage
            let min_leverage = hyper_market.leverage.min(ext_market.leverage);

            if hyper_market.leverage > min_leverage {
                operator_hyperliquid.change_leverage(symbol.clone(), min_leverage).await.map_err(|e| {
                    error!("Failed to update leverage for Hyperliquid market {}: {:?}", symbol, e);
                    e
                })?;
            } else {
                operator_extended.change_leverage(symbol.clone(), min_leverage).await.map_err(|e| {
                    error!("Failed to update leverage for Extended market {}: {:?}", symbol, e);
                    e
                })?;
            }

            let (long_operator, short_operator, short_side, long_side): (Box<dyn Operator>, Box<dyn Operator>, PositionSide, PositionSide) = if hyper_rate > ext_rate {
                (operator_extended, operator_hyperliquid, PositionSide::Short, PositionSide::Long)
            } else {
                (operator_hyperliquid, operator_extended, PositionSide::Short, PositionSide::Long)
            };

            print!("[INFO] Placing orders for symbol: {}", symbol);
            info!("Short Operator: {}, Long Operator: {}", short_operator.get_exchange_name().as_str(), long_operator.get_exchange_name().as_str());
            info!("Amount: {}, Min Leverage: {}", amount, min_leverage);

            /* match create_order(short_operator.as_ref(), symbol, short_side, &amount, &hyper_market.ask_price, Some(false)).await {
                Ok(order_result) => {
                    info!("Order executed successfully!");
                    info!("Order ID: {}", order_result.order_id);
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

            let bips_offset = Decimal::from_f64(0.010).unwrap();
            let price_adjusted = (ext_market.ask_price * (Decimal::ONE + bips_offset)).round_dp(4);

            match create_order(long_operator.as_ref(), symbol, long_side, &amount, &price_adjusted, Some(false)).await {
                Ok(order_result) => {
                    info!("Long order placed: {:?}", order_result);
                            info!("Order ID: {}", order_result.order_id);
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
            }*/
        } else {
            error!("Failed to find matching markets for symbol: {}", symbol);
        }
    }

    Ok(())
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