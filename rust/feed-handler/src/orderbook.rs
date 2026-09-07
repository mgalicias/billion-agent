//! Local order-book maintenance following Binance's documented procedure
//! for spot depth streams:
//! <https://binance-docs.github.io/apidocs/spot/en/#how-to-manage-a-local-order-book-correctly>
//!
//! 1. Buffer diff events arriving on the WebSocket before a snapshot exists.
//! 2. Fetch a REST snapshot (`lastUpdateId`, full bids/asks).
//! 3. Drop any buffered event whose `u` <= `lastUpdateId`.
//! 4. The first event applied must satisfy `U <= lastUpdateId+1 <= u`.
//! 5. Once synced, each new event's `U` must equal the previous event's
//!    `u`+1 — any other value means a message was missed and the book must
//!    be considered invalid until resynced from a fresh snapshot.
//!
//! This is the concrete mitigation for the "WebSocket disconnect leaves a
//! stale/wrong local book" risk called out in docs/PLAN.md.

use crate::binance::{RawDepthUpdate, RestDepthSnapshot};
use common::PriceLevel;
use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq)]
pub enum BookState {
    AwaitingSnapshot,
    Synced,
}

pub struct OrderBook {
    pub symbol: String,
    pub state: BookState,
    pub last_update_id: u64,
    bids: HashMap<u64, PriceLevel>,
    asks: HashMap<u64, PriceLevel>,
    buffer: Vec<RawDepthUpdate>,
}

/// Returned when a gap is detected in the update-id sequence: the caller
/// must discard local state and resync from a fresh REST snapshot.
#[derive(Debug)]
pub struct GapDetected {
    pub expected_first_update_id: u64,
    pub got_first_update_id: u64,
}

impl OrderBook {
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            state: BookState::AwaitingSnapshot,
            last_update_id: 0,
            bids: HashMap::new(),
            asks: HashMap::new(),
            buffer: Vec::new(),
        }
    }

    pub fn reset_for_resync(&mut self) {
        self.state = BookState::AwaitingSnapshot;
        self.bids.clear();
        self.asks.clear();
        self.buffer.clear();
    }

    /// Call while `state == AwaitingSnapshot`; events are held until a
    /// snapshot arrives.
    pub fn buffer_event(&mut self, event: RawDepthUpdate) {
        self.buffer.push(event);
    }

    /// Applies a REST snapshot plus any compatible buffered events,
    /// transitioning the book into `Synced`.
    pub fn apply_snapshot(&mut self, snapshot: RestDepthSnapshot) {
        self.last_update_id = snapshot.last_update_id;
        self.bids = index_levels(snapshot.bids());
        self.asks = index_levels(snapshot.asks());

        let buffered = std::mem::take(&mut self.buffer);
        // Drop events already reflected in the snapshot.
        let relevant: Vec<_> = buffered
            .into_iter()
            .filter(|e| e.final_update_id > self.last_update_id)
            .collect();

        let mut started = false;
        for event in relevant {
            if !started {
                if event.first_update_id <= self.last_update_id + 1
                    && event.final_update_id >= self.last_update_id + 1
                {
                    started = true;
                } else {
                    // Still older than (or not adjacent to) the snapshot;
                    // wait for the next live event instead.
                    continue;
                }
            }
            self.apply_levels(&event);
            self.last_update_id = event.final_update_id;
        }

        self.state = BookState::Synced;
    }

    /// Call while `state == Synced` for each new live event.
    pub fn apply_event(&mut self, event: &RawDepthUpdate) -> Result<(), GapDetected> {
        debug_assert_eq!(self.state, BookState::Synced);
        if event.first_update_id != self.last_update_id + 1 {
            return Err(GapDetected {
                expected_first_update_id: self.last_update_id + 1,
                got_first_update_id: event.first_update_id,
            });
        }
        self.apply_levels(event);
        self.last_update_id = event.final_update_id;
        Ok(())
    }

    fn apply_levels(&mut self, event: &RawDepthUpdate) {
        apply_side(&mut self.bids, &event.bids);
        apply_side(&mut self.asks, &event.asks);
    }

    pub fn best_bid(&self) -> Option<PriceLevel> {
        self.bids.values().copied().reduce(|a, b| if a.price > b.price { a } else { b })
    }

    pub fn best_ask(&self) -> Option<PriceLevel> {
        self.asks.values().copied().reduce(|a, b| if a.price < b.price { a } else { b })
    }

    pub fn depth(&self) -> (usize, usize) {
        (self.bids.len(), self.asks.len())
    }

    /// Full current book state, for emitting a `Snapshot` event right after
    /// a resync completes (see `common::schema::Snapshot`).
    pub fn export_levels(&self) -> (Vec<PriceLevel>, Vec<PriceLevel>) {
        (
            self.bids.values().copied().collect(),
            self.asks.values().copied().collect(),
        )
    }
}

fn index_levels(levels: Vec<PriceLevel>) -> HashMap<u64, PriceLevel> {
    levels.into_iter().map(|l| (l.price.to_bits(), l)).collect()
}

fn apply_side(book_side: &mut HashMap<u64, PriceLevel>, updates: &[[String; 2]]) {
    for [price_str, qty_str] in updates {
        let (Ok(price), Ok(quantity)) = (price_str.parse::<f64>(), qty_str.parse::<f64>()) else {
            continue;
        };
        if quantity == 0.0 {
            book_side.remove(&price.to_bits());
        } else {
            book_side.insert(price.to_bits(), PriceLevel { price, quantity });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(price: &str, qty: &str) -> [String; 2] {
        [price.to_string(), qty.to_string()]
    }

    fn diff(first: u64, final_: u64, bids: Vec<[String; 2]>, asks: Vec<[String; 2]>) -> RawDepthUpdate {
        RawDepthUpdate {
            event_time_ms: 0,
            symbol: "BTCUSDT".to_string(),
            first_update_id: first,
            final_update_id: final_,
            bids,
            asks,
        }
    }

    fn snapshot(last_update_id: u64) -> RestDepthSnapshot {
        RestDepthSnapshot {
            last_update_id,
            bids: vec![level("100.0", "1.0")],
            asks: vec![level("101.0", "1.0")],
        }
    }

    #[test]
    fn buffers_events_until_snapshot_arrives() {
        let mut book = OrderBook::new("btcusdt");
        assert_eq!(book.state, BookState::AwaitingSnapshot);
        book.buffer_event(diff(101, 105, vec![], vec![]));
        // Still awaiting: nothing applied yet.
        assert_eq!(book.depth(), (0, 0));
    }

    #[test]
    fn drops_buffered_events_older_than_snapshot_then_applies_the_rest() {
        let mut book = OrderBook::new("btcusdt");
        // Snapshot will report lastUpdateId = 100.
        book.buffer_event(diff(50, 90, vec![level("99.0", "5.0")], vec![])); // stale, must be dropped
        book.buffer_event(diff(95, 102, vec![level("100.5", "2.0")], vec![])); // straddles snapshot: U<=101<=u
        book.buffer_event(diff(103, 104, vec![level("100.6", "3.0")], vec![])); // contiguous after

        book.apply_snapshot(snapshot(100));

        assert_eq!(book.state, BookState::Synced);
        assert_eq!(book.last_update_id, 104);
        // Snapshot level + the two applied diffs = 3 bid levels.
        assert_eq!(book.depth(), (3, 1));
    }

    #[test]
    fn synced_book_applies_contiguous_events() {
        let mut book = OrderBook::new("btcusdt");
        book.apply_snapshot(snapshot(100));

        let result = book.apply_event(&diff(101, 103, vec![level("100.5", "1.0")], vec![]));
        assert!(result.is_ok());
        assert_eq!(book.last_update_id, 103);
    }

    #[test]
    fn detects_gap_and_reports_expected_vs_actual() {
        let mut book = OrderBook::new("btcusdt");
        book.apply_snapshot(snapshot(100));

        // Expected next U is 101, but this event starts at 110: a gap.
        let err = book
            .apply_event(&diff(110, 115, vec![], vec![]))
            .unwrap_err();
        assert_eq!(err.expected_first_update_id, 101);
        assert_eq!(err.got_first_update_id, 110);
    }

    #[test]
    fn zero_quantity_removes_the_price_level() {
        let mut book = OrderBook::new("btcusdt");
        book.apply_snapshot(snapshot(100));
        assert_eq!(book.depth(), (1, 1));

        book.apply_event(&diff(101, 102, vec![level("100.0", "0")], vec![]))
            .unwrap();
        assert_eq!(book.depth(), (0, 1));
    }

    #[test]
    fn reset_for_resync_clears_state_back_to_awaiting_snapshot() {
        let mut book = OrderBook::new("btcusdt");
        book.apply_snapshot(snapshot(100));
        book.reset_for_resync();
        assert_eq!(book.state, BookState::AwaitingSnapshot);
        assert_eq!(book.depth(), (0, 0));
    }
}
