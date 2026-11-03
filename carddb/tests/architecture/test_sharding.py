"""Shards split a real xdist run into disjoint slices that together run every test."""

from pathlib import Path

import pytest

from sve_carddb.core.json import integer, object_value, parse

from ..support.sharding import parse_shard, shard_of

pytest_plugins = ("pytester",)

SHARDS = 3
CASES = 60


def run_shard(
    pytester: pytest.Pytester, index: int, report: Path
) -> tuple[set[str], dict[str, int]]:
    events = pytester.path / f"events-{index}.txt"
    pytester.makeconftest(f"""
        from pathlib import Path

        def pytest_runtest_logreport(report):
            if report.when == "call":
                with Path({str(events)!r}).open("a") as stream:
                    stream.write(report.nodeid + "\\n")
    """)
    result = pytester.runpytest_subprocess(
        "-p",
        "xdist.plugin",
        "-p",
        "tests.support.sharding",
        "-n",
        "2",
        "-q",
        f"--shard={index}/{SHARDS}",
        f"--shard-report={report}",
    )
    counts = object_value(parse(report.read_bytes()))
    ran = set(events.read_text().splitlines()) if events.exists() else set()
    result.assert_outcomes(passed=len(ran))
    return ran, {key: integer(value) for key, value in counts.items()}


def test_shards_are_disjoint_and_complete(
    pytester: pytest.Pytester, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("PYTHONPATH", str(Path(__file__).resolve().parents[2]))
    monkeypatch.setenv("PYTEST_DISABLE_PLUGIN_AUTOLOAD", "1")
    monkeypatch.delenv("PYTEST_ADDOPTS", raising=False)
    pytester.makeini("[pytest]\naddopts=\n")
    pytester.makepyfile(
        test_alpha=f"""
            import pytest

            @pytest.mark.parametrize("case", range({CASES}))
            def test_case(case):
                pass
        """,
        test_beta="def test_single():\n    pass\n",
    )
    union: set[str] = set()
    for index in range(1, SHARDS + 1):
        ran, counts = run_shard(pytester, index, pytester.path / f"shard-{index}.json")
        assert not union & ran
        union |= ran
        assert counts == {
            "index": index,
            "total": SHARDS,
            "collected": CASES + 1,
            "selected": len(ran),
        }
    assert len(union) == CASES + 1


def test_assignment_is_stable_and_validated() -> None:
    # Fixed values guard against switching to Python's per-process salted hash.
    assert shard_of("tests/test_alpha.py::test_case[0]", SHARDS) == 2
    assert shard_of("tests/test_alpha.py::test_case[1]", 7) == 1
    assert {shard_of(f"t::{n}", SHARDS) for n in range(50)} == {1, 2, 3}
    assert parse_shard("2/3") == (2, 3)
    for value in ("0/3", "4/3", "1/0", "a/3", "1/3/1"):
        with pytest.raises(pytest.UsageError):
            parse_shard(value)
