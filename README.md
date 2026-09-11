# billion-agent

A Binance-first crypto trading platform, built out phase by phase per
[`docs/PLAN.md`](docs/PLAN.md). This is Phase 0: market data ingestion,
book reconstruction, and a backtesting skeleton — no order placement yet.

## Layout

```
rust/                  Hot-path components (feed handler; risk engine/OMS
                        land in later phases)
  common/               Canonical market-data schema + config loading,
                        shared across every Rust crate
  feed-handler/         Connects to Binance's spot combined WS stream,
                        maintains a resynced local order book, emits
                        canonical JSON-line events to stdout
python/
  backtester/           Event-driven replay of the same canonical schema
                         the feed handler emits, plus an example strategy
  tests/                pytest suite
config/
  testnet.toml           Binance Testnet endpoints + default risk limits
  mainnet.toml           Binance mainnet endpoints (see the testnet-first
                          workflow below before ever pointing here)
data/sample/            Small handcrafted event file for tests/local runs
                          without a network connection
docs/PLAN.md             Full architecture, phasing, and risk plan
```

## Testnet-first workflow

Every change that can affect order behavior is validated against Binance's
Testnet before touching mainnet — this is a standing rule, not a one-time
setup step. See `docs/PLAN.md`'s "Development Workflow: Testnet-First for
Every Change" for the full promotion pipeline. Practically, this means: the
environment is picked by which config file you load (`config/testnet.toml`
vs `config/mainnet.toml`), never by a code fork.

## Running the feed handler

Requires Rust (`cargo`/`rustc`). No API key is needed for public market
data streams.

```bash
cargo run --manifest-path rust/Cargo.toml -p feed-handler -- config/testnet.toml
```

This connects to Binance Testnet's combined stream for the symbols listed
in the config, maintains a locally synced order book (buffering diffs
until a REST snapshot arrives, then applying them per Binance's documented
procedure — see `rust/feed-handler/src/orderbook.rs`), and prints one JSON
line per event to stdout: a `snapshot` event whenever the book (re)syncs,
then `depth`/`trade` events. Capture it to a file to build a dataset for
the backtester:

```bash
cargo run --manifest-path rust/Cargo.toml -p feed-handler -- config/testnet.toml \
  > data/captures/btcusdt-$(date +%Y%m%d-%H%M).jsonl
```

Run the Rust test suite (covers the resync state machine — buffering,
snapshot application, gap detection):

```bash
cargo test --manifest-path rust/Cargo.toml
```

## Running the backtester

Requires Python 3.11+.

```bash
cd python
pip install -e ".[dev]"
pytest
```

```python
from backtester import BacktestEngine, NaiveMarketMaker

strategy = NaiveMarketMaker(quote_size=0.001, min_spread_bps=2.0)
engine = BacktestEngine(strategy)
engine.run_file("../data/sample/sample_events.jsonl")
print(strategy.intents)
```

`NaiveMarketMaker` is a placeholder to prove the engine wiring end-to-end —
see its docstring. Point `run_file` at a `.jsonl` capture from the feed
handler to backtest against real (testnet or mainnet) market data.

## What's next

Phase 1 (see `docs/PLAN.md`): a risk engine with hard position/order/rate
limits and a kill switch, an OMS with Binance order-lifecycle
reconciliation, and the first strategy trading small real size — always
soaked on testnet first.
