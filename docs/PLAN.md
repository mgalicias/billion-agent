# Crypto Trading Platform — Binance-First Architecture & Roadmap

## Context

`billion-agent` is a highly scalable, low-latency-oriented crypto trading
system, run solo, trading own capital, focused exclusively on **Binance** to
start. Forex is out of scope for now.

### Reality check on "HFT," scoped to Binance

- **True HFT (sub-millisecond, queue-position games) is not accessible to a
  solo developer** — that tier belongs to firms with FPGA/kernel-bypass
  networking and cross-connects inside exchange datacenters.
- **What's actually achievable on Binance as a retail/solo trader:**
  low-tens-of-milliseconds round trip by co-locating the bot in Binance's
  cloud region (AWS `ap-northeast-1`, Tokyo), using WebSocket market data +
  REST/WS order entry. That's enough to run competitively at:
  - market making on liquid pairs (spread capture, maker rebates),
  - funding-rate/basis arbitrage (spot vs. perpetual futures, both on Binance),
  - short-horizon signal-driven strategies (seconds-to-minutes holding).
- **Single-exchange scope removes an entire class of risk** (cross-venue
  settlement/withdrawal risk, multi-venue reconciliation) — good for a first
  build. Multi-exchange arbitrage is a natural Phase 3+ extension once the
  core platform is proven, not a day-one requirement.

## Phased Roadmap

**Phase 0 — Foundations (weeks 1-4)**
- Binance market data ingestion: WebSocket feed handler for order book
  (depth) + trades + (if trading futures) funding rate, normalized into an
  internal canonical schema.
- Historical data capture + event-driven backtesting engine that replays the
  same canonical schema the live system consumes.
- Paper-trading execution path end-to-end against Binance's **testnet**
  (spot testnet / futures testnet) — no real orders yet.
- Observability skeleton (metrics, logs, alerting) from day one.

**Phase 1 — First live strategy, small size (weeks 4-10)**
- One well-understood strategy class live with real but small capital —
  e.g. simple market making on a single liquid pair (BTC/USDT) or
  spot-futures basis capture.
- Risk engine v1: hard position limits, per-strategy kill switch, max
  daily-loss circuit breaker enforced *before* order submission.
- API key management: Binance API key scoped to trading only (no
  withdrawal permission), IP-allowlisted, stored in a secrets manager.

**Phase 2 — Harden execution & risk (weeks 10-16)**
- Order management system (OMS): order state machine, reconciliation
  against Binance's account/position/order endpoints, idempotent retries
  (using Binance's `newClientOrderId` for dedup), partial-fill handling.
- Handle Binance-specific realities: weight-based rate limits, order-rate
  limits, WebSocket reconnect/resync (order book checksum/resync per
  Binance's documented procedure), maintenance windows.
- Latency budget measurement (market-data-in → order-out); deploy in
  `ap-northeast-1` to minimize round trip to Binance's matching engine.
- Chaos/failure testing: feed disconnect, stale book, rate-limit backoff,
  duplicate-order prevention.

**Phase 3 — Scale out (weeks 16-24, once Phase 1-2 are profitable/stable)**
- Add a second strategy (uncorrelated with the first) on Binance, still
  single-venue, to build a small strategy portfolio.
- Only after this is solid: consider a second exchange or forex as a
  separate, slower-frequency track — explicitly a new phase, not a
  parallel effort.

**Phase 4 — Production hardening (ongoing)**
- Full DR/failover, infra-as-code, security review, key rotation.
- Formalize backtest → testnet → paper → canary (small size) → full-size
  promotion pipeline for every new strategy — never ship a strategy
  straight to full size.
- Only if you ever consider managing outside capital: revisit regulatory
  posture before taking a single external dollar — a legal gate, not an
  engineering one.

## Development Workflow: Testnet-First for Every Change

Binance provides both a **Spot Testnet** (`testnet.binance.vision`) and a
**Futures Testnet** (`testnet.binancefuture.com`) — separate environments
with their own API keys, real order matching, and fake funds. This is a
permanent gate in the workflow, not just a one-time Phase 0 step:

- **Every change that can affect order behavior — new strategy, risk-engine
  edit, OMS change, dependency bump on the hot path — is deployed to testnet
  and run there before it ever touches mainnet.** Standing rule, not
  optional per-change judgment.
- **Environment is a config switch, not a code fork.** The exact same binary/
  strategy code runs against testnet or mainnet based on config (base URLs +
  key pair swapped); this guarantees what was validated on testnet is
  byte-for-byte what runs live. Never maintain separate "testnet version" and
  "prod version" code paths. See `config/testnet.toml` / `config/mainnet.toml`.
- **Promotion pipeline for any change:**
  1. Backtest against historical data (validates strategy logic/PnL
     assumptions).
  2. Testnet, live for a soak period (validates order lifecycle, error
     handling, risk-engine behavior, reconnect/resync logic).
  3. Mainnet canary — smallest viable real size, tight risk limits.
  4. Mainnet full size, only after canary shows expected behavior.
- **What testnet does and doesn't prove:** testnet validates *correctness*
  (order placement/cancel/tracking, risk-engine enforcement, reconnect/resync)
  — its book depth/liquidity and latency don't match mainnet, so it cannot
  validate *strategy profitability*. Profitability is judged via backtest +
  the mainnet canary stage, never via testnet PnL.
- **CI integration:** the automated test suite includes a testnet-integration
  stage using dedicated testnet API keys stored the same way as production
  keys (secrets manager, never inline) — a change cannot merge without this
  stage passing.

## Architecture

Event-driven, single-venue (Binance) to start, built so each component can
scale independently later.

```
 Binance WS/REST ──▶ Feed Handler/Normalizer ──▶ Market Data Bus
   (depth, trades,                                    │
    funding rate)                ┌───────────────────┼────────────────────┐
                                 ▼                    ▼                    ▼
                          Strategy Engine(s)    Backtester/Sim      Monitoring/Metrics
                                 │
                                 ▼
                           Risk Engine (pre-trade checks, limits, kill switch)
                                 │
                                 ▼
                        Order Management System (OMS)
                                 │
                                 ▼
                       Binance Gateway (REST + WS order entry,
                             spot + USDT-M futures)

 Persistent stores: time-series DB (ticks/book snapshots), Postgres (orders,
 positions, accounting), object storage (raw feed archives for research)
```

**Key components:**
- **Feed Handler** — single process talking to Binance's market/user data
  WebSocket streams, converts venue-specific messages into an internal
  canonical schema (book deltas, trades, funding). Handles Binance's
  documented order-book resync procedure (snapshot + diff stream) so a
  dropped message never leaves the local book silently wrong.
- **Strategy Engine** — stateless-as-possible logic consuming the canonical
  market data bus, emitting order *intents* (not raw orders) to the risk
  engine. Same code path runs in backtest, testnet, and live modes.
- **Risk Engine** — the only component allowed to veto or shrink an order.
  Pre-trade limits (max position, max order size, max order rate), a global
  kill switch, and a daily-loss circuit breaker halting all trading. Hard
  dependency, never bypassable by a strategy.
- **OMS** — owns order lifecycle; source of truth for "what do we actually
  hold," continuously reconciled against Binance's account/position/order
  endpoints.
- **Backtester** — event-driven replay of the same canonical data format the
  live system consumes, so strategy code is identical between backtest and
  live.
- **Monitoring** — latency histograms per hop, real-time P&L, position
  drift vs. Binance truth, alerting on risk-limit breaches, feed
  disconnects, or rate-limit warnings.

## Recommended Tech Stack

| Layer | Recommendation | Why |
|---|---|---|
| Hot path (feed handler, risk engine, OMS core) | **Rust** | Predictable low-latency, no GC pauses. |
| Strategy research & backtesting | **Python** (pandas/polars, numpy) | Fastest iteration for quant research; ported to Rust once proven. |
| Non-hot-path services (config, reporting, admin API) | **Go** | Simple, fast enough, easy ops for glue services. |
| Market data storage | **QuestDB or ClickHouse** | Purpose-built for high-ingest tick/book data. |
| Transactional store (orders, positions, accounting) | **PostgreSQL** | ACID guarantees matter for money. |
| Messaging/bus | **NATS or Redis Streams** | Low-latency pub/sub; avoid Kafka's ops overhead at this scale. |
| Deployment | **Dedicated instance in AWS `ap-northeast-1` (Tokyo)** | Minimizes round trip to Binance's matching engine. |
| Observability | **Prometheus + Grafana + Loki** | Standard, self-hostable. |
| Secrets | **Vault or cloud KMS** | API keys are bearer credentials to funds — never in env files or code. |

## Key Risks & Mitigations

- **API key leak/over-permissioned** → trading-only key, no withdrawal
  scope, IP-allowlisted, rotated regularly, stored in a secrets manager.
- **A bug sends runaway orders** → risk engine hard limits + kill switch
  independent of strategy code; manual "arm" step required before any
  strategy trades real size.
- **Binance rate-limit ban** → track weight usage locally, back off before
  hitting limits, alert on warnings.
- **WebSocket disconnect leaves a stale/wrong local book** → implement
  Binance's documented resync procedure (REST snapshot + buffered diffs).
- **Backtest overfitting → losses live** → mandatory testnet + small-size
  canary period before full size; walk-forward validation.
- **Regulatory exposure creeps in unnoticed** → hard rule: no third-party
  funds without prior legal review; stay solo/own-capital unless
  deliberately revisited.
