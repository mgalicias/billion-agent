"""Local order book reconstruction from the canonical event stream — the
Python-side mirror of rust/feed-handler/src/orderbook.rs's book-state
handling (minus the resync logic, which already happened live; here we
only ever see clean `Snapshot` + contiguous `DepthUpdate` events)."""

from __future__ import annotations

from .events import DepthUpdate, PriceLevel, Snapshot


class LocalBook:
    def __init__(self) -> None:
        self.last_update_id: int | None = None
        self._bids: dict[float, float] = {}
        self._asks: dict[float, float] = {}

    def apply_snapshot(self, snapshot: Snapshot) -> None:
        self.last_update_id = snapshot.last_update_id
        self._bids = {l.price: l.quantity for l in snapshot.bids}
        self._asks = {l.price: l.quantity for l in snapshot.asks}

    def apply_depth(self, update: DepthUpdate) -> None:
        if self.last_update_id is None:
            # No snapshot seen yet for this symbol; nothing to apply against.
            return
        _apply_side(self._bids, update.bids)
        _apply_side(self._asks, update.asks)
        self.last_update_id = update.final_update_id

    def best_bid(self) -> PriceLevel | None:
        if not self._bids:
            return None
        price = max(self._bids)
        return PriceLevel(price=price, quantity=self._bids[price])

    def best_ask(self) -> PriceLevel | None:
        if not self._asks:
            return None
        price = min(self._asks)
        return PriceLevel(price=price, quantity=self._asks[price])

    def mid_price(self) -> float | None:
        bid, ask = self.best_bid(), self.best_ask()
        if bid is None or ask is None:
            return None
        return (bid.price + ask.price) / 2


def _apply_side(side: dict[float, float], levels: list[PriceLevel]) -> None:
    for level in levels:
        if level.quantity == 0.0:
            side.pop(level.price, None)
        else:
            side[level.price] = level.quantity
