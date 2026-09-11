"""Example strategy: a naive fixed-offset market maker. This exists to
prove the engine wiring end-to-end, not as a strategy that should be
traded — it has no inventory-risk handling, adverse-selection protection,
or connection to the risk-engine limits described in docs/PLAN.md.
Real Phase 1 strategy work replaces this.
"""

from __future__ import annotations

from dataclasses import dataclass

from .book import LocalBook
from .events import DepthUpdate, Snapshot, TradeEvent


@dataclass(frozen=True)
class OrderIntent:
    symbol: str
    side: str  # "buy" | "sell"
    price: float
    quantity: float


class NaiveMarketMaker:
    """Quotes a fixed offset around mid price whenever the observed spread
    exceeds `min_spread_bps`. Intents are only recorded here — there is no
    risk engine or OMS wired in at this phase (see docs/PLAN.md Phase 1)."""

    def __init__(self, quote_size: float = 0.001, min_spread_bps: float = 2.0) -> None:
        self.quote_size = quote_size
        self.min_spread_bps = min_spread_bps
        self.intents: list[OrderIntent] = []

    def on_snapshot(self, book: LocalBook, event: Snapshot) -> None:
        self._maybe_quote(book, event.symbol)

    def on_depth(self, book: LocalBook, event: DepthUpdate) -> None:
        self._maybe_quote(book, event.symbol)

    def on_trade(self, book: LocalBook, event: TradeEvent) -> None:
        pass  # A real strategy would use trade prints for signal/fill inference.

    def _maybe_quote(self, book: LocalBook, symbol: str) -> None:
        bid, ask = book.best_bid(), book.best_ask()
        mid = book.mid_price()
        if bid is None or ask is None or mid is None:
            return
        spread_bps = (ask.price - bid.price) / mid * 10_000
        if spread_bps < self.min_spread_bps:
            return
        offset = mid * (self.min_spread_bps / 2 / 10_000)
        self.intents.append(OrderIntent(symbol, "buy", round(mid - offset, 8), self.quote_size))
        self.intents.append(OrderIntent(symbol, "sell", round(mid + offset, 8), self.quote_size))
