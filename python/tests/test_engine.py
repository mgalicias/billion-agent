from pathlib import Path

from backtester import BacktestEngine, NaiveMarketMaker

SAMPLE = Path(__file__).parents[2] / "data" / "sample" / "sample_events.jsonl"


def test_strategy_quotes_on_every_book_update_when_spread_is_wide_enough():
    strategy = NaiveMarketMaker(quote_size=0.001, min_spread_bps=2.0)
    engine = BacktestEngine(strategy)
    engine.run_file(SAMPLE)

    # snapshot + 2 depth updates = 3 book-changing events, 2 intents (buy+sell) each.
    assert len(strategy.intents) == 6
    sides = {intent.side for intent in strategy.intents}
    assert sides == {"buy", "sell"}
    for intent in strategy.intents:
        assert intent.symbol == "btcusdt"
        assert intent.quantity == 0.001


def test_strategy_stays_quiet_when_spread_is_too_tight():
    strategy = NaiveMarketMaker(quote_size=0.001, min_spread_bps=1_000.0)
    engine = BacktestEngine(strategy)
    engine.run_file(SAMPLE)

    assert strategy.intents == []
