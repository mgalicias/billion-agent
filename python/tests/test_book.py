from pathlib import Path

from backtester import BacktestEngine, NaiveMarketMaker, PriceLevel, read_events

SAMPLE = Path(__file__).parents[2] / "data" / "sample" / "sample_events.jsonl"


def test_local_book_reconstructs_state_from_snapshot_plus_diffs():
    engine = BacktestEngine(NaiveMarketMaker())
    engine.run_file(SAMPLE)

    book = engine.book_for("btcusdt")
    assert book.last_update_id == 104
    assert book.best_bid() == PriceLevel(price=100.00, quantity=0.5)
    assert book.best_ask() == PriceLevel(price=100.20, quantity=2.5)


def test_read_events_parses_every_kind_in_the_sample_file():
    events = list(read_events(SAMPLE))
    kinds = [type(e).__name__ for e in events]
    assert kinds == ["Snapshot", "DepthUpdate", "TradeEvent", "DepthUpdate"]
