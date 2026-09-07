//! Raw Binance wire types (spot combined-stream + REST depth snapshot) and
//! their conversion into `common::schema` canonical types. Keeping the
//! Binance-specific field names (`e`, `E`, `s`, `U`, `u`, ...) isolated to
//! this one module is what lets everything downstream (strategy, risk,
//! OMS, backtester) stay venue-agnostic.

use common::{DepthUpdate, PriceLevel, Symbol, Trade};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CombinedStreamEnvelope {
    pub stream: String,
    pub data: RawEvent,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "e")]
pub enum RawEvent {
    #[serde(rename = "depthUpdate")]
    DepthUpdate(RawDepthUpdate),
    #[serde(rename = "trade")]
    Trade(RawTrade),
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawDepthUpdate {
    #[serde(rename = "E")]
    pub event_time_ms: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "U")]
    pub first_update_id: u64,
    #[serde(rename = "u")]
    pub final_update_id: u64,
    #[serde(rename = "b")]
    pub bids: Vec<[String; 2]>,
    #[serde(rename = "a")]
    pub asks: Vec<[String; 2]>,
}

#[derive(Debug, Deserialize)]
pub struct RawTrade {
    #[serde(rename = "E")]
    pub event_time_ms: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "t")]
    pub trade_id: u64,
    #[serde(rename = "p")]
    pub price: String,
    #[serde(rename = "q")]
    pub quantity: String,
    #[serde(rename = "m")]
    pub is_buyer_maker: bool,
}

#[derive(Debug, Deserialize)]
pub struct RestDepthSnapshot {
    #[serde(rename = "lastUpdateId")]
    pub last_update_id: u64,
    pub bids: Vec<[String; 2]>,
    pub asks: Vec<[String; 2]>,
}

fn parse_levels(levels: &[[String; 2]]) -> Vec<PriceLevel> {
    levels
        .iter()
        .filter_map(|[p, q]| {
            let price: f64 = p.parse().ok()?;
            let quantity: f64 = q.parse().ok()?;
            Some(PriceLevel { price, quantity })
        })
        .collect()
}

impl RawDepthUpdate {
    /// `prev_final_update_id` is left `None` here — spot diff-depth events
    /// don't carry a `pu` field (that's futures-only); gap detection for
    /// spot instead compares this event's `U` to the previous event's `u`,
    /// which is handled in `orderbook::OrderBook`.
    pub fn into_canonical(&self) -> DepthUpdate {
        DepthUpdate {
            symbol: self.symbol.to_lowercase(),
            event_time_ms: self.event_time_ms,
            first_update_id: self.first_update_id,
            final_update_id: self.final_update_id,
            prev_final_update_id: None,
            bids: parse_levels(&self.bids),
            asks: parse_levels(&self.asks),
        }
    }
}

impl RawTrade {
    pub fn into_canonical(&self) -> (Symbol, Trade) {
        (
            self.symbol.to_lowercase(),
            Trade {
                trade_id: self.trade_id,
                event_time_ms: self.event_time_ms,
                price: self.price.parse().unwrap_or_default(),
                quantity: self.quantity.parse().unwrap_or_default(),
                is_buyer_maker: self.is_buyer_maker,
            },
        )
    }
}

impl RestDepthSnapshot {
    pub fn bids(&self) -> Vec<PriceLevel> {
        parse_levels(&self.bids)
    }

    pub fn asks(&self) -> Vec<PriceLevel> {
        parse_levels(&self.asks)
    }
}

pub async fn fetch_depth_snapshot(
    rest_base_url: &str,
    symbol: &str,
    limit: u32,
) -> anyhow::Result<RestDepthSnapshot> {
    let url = format!(
        "{rest_base_url}/api/v3/depth?symbol={}&limit={limit}",
        symbol.to_uppercase()
    );
    let snapshot = reqwest::get(&url).await?.json::<RestDepthSnapshot>().await?;
    Ok(snapshot)
}
