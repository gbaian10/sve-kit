"""Shards split a real xdist run into disjoint slices that together run every test."""

from pathlib import Path

import pytest

from sve_carddb.core.json import integer, object_value, parse

from ..support.sharding import assign, parse_shard

pytest_plugins = ("pytester",)

SHARDS = 3
CASES = 60


def run_shard(
    pytester: pytest.Pytester, index: int, report: Path, distribution: str
) -> tuple[set[str], dict[str, int]]:
    events = pytester.path / "events.txt"
    events.unlink(missing_ok=True)
    pytester.makeconftest(f"""
        from pathlib import Path

        def pytest_configure(config):
            global is_worker
            is_worker = hasattr(config, "workerinput")

        def pytest_runtest_logreport(report):
            if report.when == "call" and not is_worker:
                with Path({str(events)!r}).open("a") as stream:
                    stream.write(report.nodeid + "\\n")
    """)
    result = pytester.runpytest_subprocess(
        "-p",
        "xdist.plugin",
        "-p",
        "tests.support.sharding",
        "-p",
        "tests.support.scheduling",
        f"--dist={distribution}",
        "-n",
        "2",
        "-q",
        f"--shard={index}/{SHARDS}",
        f"--shard-report={report}",
    )
    counts = object_value(parse(report.read_bytes()))
    reported = events.read_text().splitlines() if events.exists() else []
    ran = set(reported)
    assert len(reported) == len(ran)
    result.assert_outcomes(passed=len(ran))
    return ran, {key: integer(value) for key, value in counts.items()}


@pytest.mark.parametrize("distribution", ["load", "loadgroup"])
def test_shards_are_disjoint_complete_and_stable(
    pytester: pytest.Pytester, monkeypatch: pytest.MonkeyPatch, distribution: str
) -> None:
    monkeypatch.setenv("PYTHONPATH", str(Path(__file__).resolve().parents[2]))
    monkeypatch.setenv("PYTEST_DISABLE_PLUGIN_AUTOLOAD", "1")
    monkeypatch.delenv("PYTEST_ADDOPTS", raising=False)
    monkeypatch.setenv("PYTHONHASHSEED", "1")
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
    first: set[str] = set()
    sizes: list[int] = []
    for index in range(1, SHARDS + 1):
        ran, counts = run_shard(
            pytester, index, pytester.path / f"shard-{index}.json", distribution
        )
        if index == 1:
            first = ran
        assert not union & ran
        union |= ran
        sizes.append(len(ran))
        assert counts == {
            "index": index,
            "total": SHARDS,
            "collected": CASES + 1,
            "selected": len(ran),
        }
    assert len(union) == CASES + 1
    # A file's tests are dealt out evenly, so the shards differ by at most one test.
    assert max(sizes) - min(sizes) <= 1
    monkeypatch.setenv("PYTHONHASHSEED", "2")
    repeated, _ = run_shard(pytester, 1, pytester.path / "repeat.json", distribution)
    assert repeated == first


def test_shard_arguments_are_validated() -> None:
    assert parse_shard("2/3") == (2, 3)
    for value in ("0/3", "4/3", "1/0", "a/3", "1/3/1"):
        with pytest.raises(pytest.UsageError):
            parse_shard(value)


def test_slow_tests_of_one_file_go_to_different_shards() -> None:
    """Three minute-long tests in one file must not all land in the same shard."""
    slow = [f"tests/workflows/test_authoring.py::test_{name}" for name in "abc"]
    shards = assign([*slow, "tests/test_other.py::test_fast"], SHARDS)
    assert sorted(shards[nodeid] for nodeid in slow) == [1, 2, 3]
    assert set(shards) == {*slow, "tests/test_other.py::test_fast"}
