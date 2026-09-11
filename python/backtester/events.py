"""Canonical market-data event types, mirroring
rust/common/src/schema.rs::MarketEvent exactly (same `kind`-tagged JSON) so
the backtester can replay data captured from the live feed handler without
any translation step. If you change the Rust schema, change this file to
match in the same commit.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Union


@dataclass(frozen=True)
class PriceLevel:
    price: float
    quantity: float

    @staticmethod
    def from_json(obj: dict) -> "PriceLevel":
        return PriceLevel(price=float(obj["price"]), quantity=float(obj["quantity"]))


@dataclass(frozen=True)
class Snapshot:
    symbol: str
    event_time_ms: int
    last_update_id: int
    bids: list[PriceLevel]
    asks: list[PriceLevel]


@dataclass(frozen=True)
class DepthUpdate:
    symbol: str
    event_time_ms: int
    first_update_id: int
    final_update_id: int
    prev_final_update_id: int | None
    bids: list[PriceLevel]
    asks: list[PriceLevel]


@dataclass(frozen=True)
class Trade:
    trade_id: int
    event_time_ms: int
    price: float
    quantity: float
    is_buyer_maker: bool


@dataclass(frozen=True)
class TradeEvent:
    symbol: str
    trade: Trade


MarketEvent = Union[Snapshot, DepthUpdate, TradeEvent]


def _levels(obj: list[dict]) -> list[PriceLevel]:
    return [PriceLevel.from_json(l) for l in obj]


def parse_event(obj: dict) -> MarketEvent:
    """Parse one decoded JSON object (one line of a captured .jsonl file)
    into the matching canonical event, based on its `kind` tag."""
    kind = obj["kind"]
    if kind == "snapshot":
        return Snapshot(
            symbol=obj["symbol"],
            event_time_ms=obj["event_time_ms"],
            last_update_id=obj["last_update_id"],
            bids=_levels(obj["bids"]),
            asks=_levels(obj["asks"]),
        )
    if kind == "depth":
        return DepthUpdate(
            symbol=obj["symbol"],
            event_time_ms=obj["event_time_ms"],
            first_update_id=obj["first_update_id"],
            final_update_id=obj["final_update_id"],
            prev_final_update_id=obj.get("prev_final_update_id"),
            bids=_levels(obj["bids"]),
            asks=_levels(obj["asks"]),
        )
    if kind == "trade":
        t = obj["trade"]
        return TradeEvent(
            symbol=obj["symbol"],
            trade=Trade(
                trade_id=t["trade_id"],
                event_time_ms=t["event_time_ms"],
                price=float(t["price"]),
                quantity=float(t["quantity"]),
                is_buyer_maker=t["is_buyer_maker"],
            ),
        )
    raise ValueError(f"unknown event kind: {kind!r}")
