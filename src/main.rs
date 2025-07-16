use anyhow::Result;
use log::{info, error};
use rust_decimal::Decimal;
use std::str::FromStr;

use points_bot_rs::{
    BotConfig,
    operators::{Operator, OrderRequest, OrderResponse, OrderType, create_hyperliquid_operator, ExtendedOperator},
    fetchers::{HyperliquidFetcher, ExtendedFetcher, Fetcher},
    Side,
    PointsBotResult,
    ExchangeName
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
    
    let fetcher: Box<dyn Fetcher> = match exchange {
        ExchangeName::Hyperliquid => Box::new(HyperliquidFetcher::new()),
        ExchangeName::Extended => Box::new(ExtendedFetcher::new()),
    };
    
    match fetcher.get_account_data(&config.wallet_address).await {
        Ok(balance) => {
            info!("Account Value: ${:.2}", balance.account_value);
            info!("Available Balance: ${:.2}", balance.available_balance);
            info!("Total Margin Used: ${:.2}", balance.total_margin_used);
            info!("Total NTL Pos: ${:.2}", balance.total_ntl_pos);
            info!("Withdrawable: ${:.2}", balance.withdrawable);
            info!("Positions Count: {}", balance.positions_count);
        }
        Err(e) => {
            error!("Failed to get account data: {}", e);
        }
    }
    
    let private_key = config.hyperliquid_private_key.as_deref().expect("Missing hyperliquid_private_key in config");
    let wallet = LocalWallet::from_str(private_key)?;
    let operator: Box<dyn Operator> = match exchange {
        ExchangeName::Hyperliquid => create_hyperliquid_operator(wallet).await,
        ExchangeName::Extended => Box::new(ExtendedOperator::new().await),
    };
    
    match create_order(operator, "BTC-USD", Side::Buy, "0.0001", "100000").await {
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
            error!("Failed to create order: {}", e);
        }
    }  
    
    Ok(())
} 