//! Phase 0 feed handler: connects to a Binance spot combined stream
//! (depth + trade) for each configured symbol, maintains a locally synced
//! order book per docs/PLAN.md's resync procedure, and emits canonical
//! `common::MarketEvent` JSON lines on stdout — the same schema the
//! Python backtester replays, so capturing this output to a file is
//! Phase 0's historical-data capture.
//!
//! Usage (from repo root):
//!   cargo run --manifest-path rust/Cargo.toml -p feed-handler -- config/testnet.toml

mod binance;
mod orderbook;

use binance::{fetch_depth_snapshot, CombinedStreamEnvelope, RawEvent, RestDepthSnapshot};
use common::{AppConfig, MarketEvent};
use futures_util::StreamExt;
use orderbook::OrderBook;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio_tungstenite::tungstenite::Message;

enum Event {
    Ws(Message),
    Snapshot(anyhow::Result<RestDepthSnapshot>),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config_path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: feed-handler <config-path>"))?;
    let config = Arc::new(common::load_config(&config_path)?);

    tracing::info!(
        environment = %config.environment.name,
        symbols = ?config.market_data.symbols,
        "starting feed handler"
    );

    let mut handles = Vec::new();
    for symbol in config.market_data.symbols.clone() {
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            if let Err(err) = run_symbol_feed(symbol.clone(), config).await {
                tracing::error!(%symbol, ?err, "feed loop exited with error");
            }
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_symbol_feed(symbol: String, config: Arc<AppConfig>) -> anyhow::Result<()> {
    let speed = config.market_data.depth_update_speed_ms;
    let stream_url = format!(
        "{}?streams={symbol}@depth@{speed}ms/{symbol}@trade",
        config.binance.spot.stream_base_url
    );

    tracing::info!(%symbol, url = %stream_url, "connecting");
    let (ws_stream, _) = tokio_tungstenite::connect_async(&stream_url).await?;
    let (_write, mut read) = ws_stream.split();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    // Forward inbound WS frames into the shared event channel.
    let ws_tx = tx.clone();
    tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(msg) => {
                    if ws_tx.send(Event::Ws(msg)).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    tracing::warn!(?err, "websocket read error");
                    break;
                }
            }
        }
    });

    // Kick off the initial REST snapshot fetch; per Binance's documented
    // procedure this happens *after* the stream connection is open so no
    // update is missed between snapshot and stream start.
    spawn_snapshot_fetch(tx.clone(), config.clone(), symbol.clone());

    let mut book = OrderBook::new(symbol.clone());

    while let Some(event) = rx.recv().await {
        match event {
            Event::Snapshot(result) => match result {
                Ok(snapshot) => {
                    book.apply_snapshot(snapshot);
                    let (bids, asks) = book.export_levels();
                    tracing::info!(
                        %symbol,
                        last_update_id = book.last_update_id,
                        bids = bids.len(),
                        asks = asks.len(),
                        "order book synced"
                    );
                    emit(&MarketEvent::Snapshot(common::Snapshot {
                        symbol: symbol.clone(),
                        event_time_ms: now_ms(),
                        last_update_id: book.last_update_id,
                        bids,
                        asks,
                    }));
                }
                Err(err) => tracing::error!(%symbol, ?err, "snapshot fetch failed"),
            },
            Event::Ws(msg) => {
                if !msg.is_text() {
                    continue;
                }
                let text = msg.into_text()?;
                let envelope: CombinedStreamEnvelope = match serde_json::from_str(&text) {
                    Ok(e) => e,
                    Err(err) => {
                        tracing::warn!(?err, raw = %text, "failed to parse stream message");
                        continue;
                    }
                };
                match envelope.data {
                    RawEvent::DepthUpdate(raw) => match book.state {
                        orderbook::BookState::AwaitingSnapshot => book.buffer_event(raw),
                        orderbook::BookState::Synced => {
                            if let Err(gap) = book.apply_event(&raw) {
                                tracing::warn!(
                                    %symbol,
                                    expected = gap.expected_first_update_id,
                                    got = gap.got_first_update_id,
                                    "update-id gap detected, resyncing book"
                                );
                                book.reset_for_resync();
                                book.buffer_event(raw);
                                spawn_snapshot_fetch(tx.clone(), config.clone(), symbol.clone());
                            } else {
                                emit(&MarketEvent::Depth(raw.into_canonical()));
                            }
                        }
                    },
                    RawEvent::Trade(raw) => {
                        let (sym, trade) = raw.into_canonical();
                        emit(&MarketEvent::Trade { symbol: sym, trade });
                    }
                }
            }
        }
    }

    Ok(())
}

fn spawn_snapshot_fetch(tx: UnboundedSender<Event>, config: Arc<AppConfig>, symbol: String) {
    tokio::spawn(async move {
        let result =
            fetch_depth_snapshot(&config.binance.spot.rest_base_url, &symbol, 1000).await;
        let _ = tx.send(Event::Snapshot(result));
    });
}

fn emit(event: &MarketEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        println!("{json}");
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
