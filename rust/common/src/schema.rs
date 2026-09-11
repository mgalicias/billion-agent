//! Canonical market-data schema shared by every consumer: the live feed
//! handler, the backtester replay, and (later) strategy/risk/OMS crates.
//! Binance-specific wire formats are translated into these types at the
//! edge (feed handler) so nothing downstream depends on venue quirks.

use serde::{Deserialize, Serialize};

pub type Symbol = String;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: f64,
    pub quantity: f64,
}

/// A book delta. `first_update_id`/`final_update_id`/`prev_final_update_id`
/// mirror Binance's `U`/`u`/`pu` fields, which are exactly what a consumer
/// needs to detect a gap and trigger a resync (see Binance's documented
/// depth-stream procedure: fetch a REST snapshot, then only apply diffs
/// whose `U`/`pu` chains onto it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DepthUpdate {
    pub symbol: Symbol,
    pub event_time_ms: u64,
    pub first_update_id: u64,
    pub final_update_id: u64,
    pub prev_final_update_id: Option<u64>,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub trade_id: u64,
    pub event_time_ms: u64,
    pub price: f64,
    pub quantity: f64,
    pub is_buyer_maker: bool,
}

/// Absolute book state at the moment a resync completed. `DepthUpdate`s are
/// deltas, so a consumer (e.g. the Python backtester) that only sees diffs
/// has no way to know the starting state; this event is emitted once per
/// resync so replaying `Snapshot` then subsequent `Depth` events reproduces
/// the same book the live system had.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub symbol: Symbol,
    pub event_time_ms: u64,
    pub last_update_id: u64,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MarketEvent {
    Snapshot(Snapshot),
    Depth(DepthUpdate),
    Trade { symbol: Symbol, trade: Trade },
}

impl MarketEvent {
    pub fn symbol(&self) -> &str {
        match self {
            MarketEvent::Snapshot(s) => &s.symbol,
            MarketEvent::Depth(d) => &d.symbol,
            MarketEvent::Trade { symbol, .. } => symbol,
        }
    }

    pub fn event_time_ms(&self) -> u64 {
        match self {
            MarketEvent::Snapshot(s) => s.event_time_ms,
            MarketEvent::Depth(d) => d.event_time_ms,
            MarketEvent::Trade { trade, .. } => trade.event_time_ms,
        }
    }
}
