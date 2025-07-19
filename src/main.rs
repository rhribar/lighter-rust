use anyhow::Result;
use log::{error, info};
use rust_decimal::{prelude::FromPrimitive, Decimal};
use std::str::FromStr;

use points_bot_rs::{
    fetchers::{Fetcher, FetcherExtended, FetcherHyperliquid, MarketInfo},
    operators::{create_operator_hyperliquid, Operator, OperatorExtended, OrderRequest, OrderResponse, OrderType},
    BotConfig, ExchangeName, PointsBotResult, Side,
};

async fn create_order(
    operator: Box<dyn Operator>,
    symbol: &str,
    side: Side,
    quantity: &str,
    price: &str,
) -> PointsBotResult<OrderResponse> {
    let order_request = OrderRequest {
        symbol: symbol.to_string(),
        side,
        order_type: OrderType::Limit,
        quantity: Decimal::from_str(quantity)?,
        price: Some(Decimal::from_str(price)?),
        stop_price: None,
        time_in_force: Some("GTC".to_string()),
    };

    info!("Creating order: {:?}", order_request);
    info!("Using exchange: {}", operator.exchange_name().as_str());

    let result = operator.create_order(order_request).await?;

    info!("Order created successfully: {:?}", result);

    Ok(result)
}

#[tokio::main]
async fn main() -> Result<()> {
    use ethers::signers::LocalWallet;
    use std::str::FromStr;
    env_logger::init();

    let config = BotConfig::from_env().unwrap_or_else(|e| {
        panic!("Failed to load configuration: {}", e);
    });

    info!("Starting Points Bot - Creating single order");
    info!("Trading mode: {}", if config.trading_mode { "LIVE" } else { "SIMULATION" });
    info!("Wallet address: {}", config.wallet_address);

    let exchange = ExchangeName::Hyperliquid;

    let fetcher_hyperliquid = Box::new(FetcherHyperliquid::new());
    let fetcher_extended = Box::new(FetcherExtended::new());

    let private_key = config.hyperliquid_private_key.as_deref().expect("Missing hyperliquid_private_key in config");
    let wallet = LocalWallet::from_str(private_key)?;
    let operator_hyperliquid = create_operator_hyperliquid(wallet).await;
    let operator_extended = Box::new(OperatorExtended::new().await);

    let mut min_available_balance = None;
    let mut arbitrage_opportunities = Vec::new();

    // Fetch account data
    match (fetcher_hyperliquid.get_account_data(&config.wallet_address).await, fetcher_extended.get_account_data(&config.wallet_address).await) {
        (Ok(hyperliquid_account), Ok(extended_account)) => {
            min_available_balance = Some(hyperliquid_account.available_balance.min(extended_account.available_balance));
            info!("Minimum Available Balance: ${:.2}", min_available_balance.unwrap());

            // Log positions from Hyperliquid
            info!("Hyperliquid Positions: {:?}", hyperliquid_account);

            // Log positions from Extended
            info!("Extended Positions: {:?}", extended_account.positions);
        }
        (Err(e), _) => {
            error!("Failed to get account data from Hyperliquid: {:?}", e);
        }
        (_, Err(e)) => {
            error!("Failed to get account data from Extended: {:?}", e);
        }
    }

    let mut hyperliquid_markets: Vec<MarketInfo> = Vec::new();
    let mut extended_markets: Vec<MarketInfo> = Vec::new();

    // Fetch funding rates
    match (fetcher_hyperliquid.get_markets().await, fetcher_extended.get_markets().await) {
        (Ok(hyperliquid_m), Ok(extended_m)) => {
            info!("Fetched {} funding rates from Hyperliquid", hyperliquid_m.len());
            info!("Fetched {} funding rates from Extended", extended_m.len());

            hyperliquid_markets = hyperliquid_m.clone();
            extended_markets = extended_m.clone();

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

    /* if let (Some(min_balance), Some(&(ref symbol, hyper_rate, ext_rate, _, _, _))) = (
        min_available_balance,
        arbitrage_opportunities.first(),
    ) {
        if let (Some(hyper_market), Some(ext_market)) = (
            hyperliquid_markets.iter().find(|m| m.symbol == *symbol),
            extended_markets.iter().find(|m| m.symbol == *symbol),
        ) {
            // Calculate the amount based on ask prices and min_available_balance
            let amount_hyper = Decimal::from_f64(min_balance).unwrap() / hyper_market.ask_price;
            let amount_ext = Decimal::from_f64(min_balance).unwrap() / ext_market.ask_price;
            let min_order_size = hyper_market.min_order_size.unwrap_or(Decimal::ZERO).max(ext_market.min_order_size.unwrap_or(Decimal::ZERO));
            if min_order_size.is_zero() {
                info!("No minimum order size specified for either market. Proceeding with the order.");
            }
            let amount = (amount_hyper.min(amount_ext).max(min_order_size) / min_order_size).floor() * min_order_size;

            // Take the minimum leverage
            let min_leverage = hyper_market.leverage.min(ext_market.leverage);

            let amount_str = amount.to_string();

            let (long_operator, short_operator, short_side, long_side): (Box<dyn Operator>, Box<dyn Operator>, Side, Side) = if hyper_rate > ext_rate {
                (operator_extended, operator_hyperliquid, Side::Sell, Side::Buy)
            } else {
                (operator_hyperliquid, operator_extended, Side::Sell, Side::Buy)
            };

            print!("[INFO] Placing orders for symbol: {}", symbol);
            info!("Short Operator: {}, Long Operator: {}", short_operator.exchange_name().as_str(), long_operator.exchange_name().as_str());
            info!("Amount: {}, Min Leverage: {}", amount_str, min_leverage);

            match create_order(short_operator, symbol, short_side, &amount_str, &hyper_market.ask_price.to_string()).await {
                Ok(order_result) => {
                    info!("Order executed successfully!");
                    info!("Order ID: {}", order_result.order_id);
                    info!("Symbol: {}", order_result.symbol);
                    info!("Side: {:?}", order_result.side);
                    info!("Status: {}", order_result.status);
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

            match create_order(long_operator, symbol, long_side, &amount_str, &ext_market.ask_price.to_string()).await {
                Ok(order_result) => {
                    info!("Long order placed: {:?}", order_result);
                            info!("Order ID: {}", order_result.order_id);
                    info!("Symbol: {}", order_result.symbol);
                    info!("Side: {:?}", order_result.side);
                    info!("Status: {}", order_result.status);
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
            error!("Failed to find matching markets for symbol: {}", symbol);
        }
    }
     */
    Ok(())
}