//! Shared config loading. The environment (testnet vs mainnet) is selected
//! purely by which config file is loaded — the same binary and strategy
//! code run against either. See docs/PLAN.md, "Testnet-First for Every
//! Change." Credentials are never read from these files: they come from
//! env vars (or a secrets manager in later phases).

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub environment: EnvironmentSection,
    pub binance: BinanceConfig,
    pub market_data: MarketDataConfig,
    pub risk: RiskConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnvironmentSection {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinanceConfig {
    pub spot: BinanceSpotConfig,
    pub futures: BinanceFuturesConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinanceSpotConfig {
    pub rest_base_url: String,
    pub ws_base_url: String,
    pub stream_base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinanceFuturesConfig {
    pub rest_base_url: String,
    pub ws_base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarketDataConfig {
    pub symbols: Vec<String>,
    pub depth_levels: u32,
    pub depth_update_speed_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    pub max_position_usd: f64,
    pub max_order_usd: f64,
    pub max_orders_per_minute: u32,
    pub daily_loss_limit_usd: f64,
}

pub fn load(path: impl AsRef<Path>) -> anyhow::Result<AppConfig> {
    let raw = std::fs::read_to_string(path.as_ref()).map_err(|e| {
        anyhow::anyhow!("failed to read config file {:?}: {e}", path.as_ref())
    })?;
    let config: AppConfig = toml::from_str(&raw)?;
    Ok(config)
}

/// Credentials are deliberately kept out of the TOML files (see module
/// docs) so a config file can never accidentally leak a key into version
/// control. Phase 1+ should replace this with a secrets-manager fetch.
pub struct Credentials {
    pub api_key: String,
    pub api_secret: String,
}

impl Credentials {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            api_key: std::env::var("BINANCE_API_KEY")
                .map_err(|_| anyhow::anyhow!("BINANCE_API_KEY not set"))?,
            api_secret: std::env::var("BINANCE_API_SECRET")
                .map_err(|_| anyhow::anyhow!("BINANCE_API_SECRET not set"))?,
        })
    }
}
