"""Event-driven backtest engine. Reads the exact canonical JSONL format the
live Rust feed handler emits (see rust/feed-handler, docs/PLAN.md), replays
it through a per-symbol `LocalBook`, and dispatches each event to a
strategy. Strategy code written against this engine's `Strategy` protocol
is meant to be portable to testnet/live with no logic changes — only the
event source changes.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Iterable, Iterator, Protocol

from .book import LocalBook
from .events import DepthUpdate, MarketEvent, Snapshot, TradeEvent, parse_event


class Strategy(Protocol):
    def on_snapshot(self, book: LocalBook, event: Snapshot) -> None: ...
    def on_depth(self, book: LocalBook, event: DepthUpdate) -> None: ...
    def on_trade(self, book: LocalBook, event: TradeEvent) -> None: ...


def read_events(path: str | Path) -> Iterator[MarketEvent]:
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            yield parse_event(json.loads(line))


class BacktestEngine:
    def __init__(self, strategy: Strategy) -> None:
        self.strategy = strategy
        self.books: dict[str, LocalBook] = {}

    def book_for(self, symbol: str) -> LocalBook:
        return self.books.setdefault(symbol, LocalBook())

    def run(self, events: Iterable[MarketEvent]) -> None:
        for event in events:
            if isinstance(event, Snapshot):
                book = self.book_for(event.symbol)
                book.apply_snapshot(event)
                self.strategy.on_snapshot(book, event)
            elif isinstance(event, DepthUpdate):
                book = self.book_for(event.symbol)
                book.apply_depth(event)
                self.strategy.on_depth(book, event)
            elif isinstance(event, TradeEvent):
                book = self.book_for(event.symbol)
                self.strategy.on_trade(book, event)
            else:
                raise TypeError(f"unhandled event type: {type(event)!r}")

    def run_file(self, path: str | Path) -> None:
        self.run(read_events(path))
