"""Exercise real xdist assignment when expensive files contain few cases."""

from collections import defaultdict
from pathlib import Path

import pytest

from sve_carddb.core.json import object_value, parse, string

pytest_plugins = ("pytester",)


@pytest.mark.parametrize("prioritize", [False, True])
def test_expensive_files_get_separate_initial_workers(
    pytester: pytest.Pytester, monkeypatch: pytest.MonkeyPatch, prioritize: bool
) -> None:
    monkeypatch.setenv("PYTHONPATH", str(Path(__file__).resolve().parents[2]))
    monkeypatch.setenv("PYTEST_DISABLE_PLUGIN_AUTOLOAD", "1")
    monkeypatch.delenv("PYTEST_ADDOPTS", raising=False)
    pytester.makeini("[pytest]\naddopts=\n")
    events = pytester.path / "events.jsonl"
    pytester.makeconftest(f"""
        import json
        from pathlib import Path

        def pytest_runtest_logreport(report):
            if report.when == "call" and hasattr(report, "worker_id"):
                with Path({str(events)!r}).open("a") as stream:
                    stream.write(json.dumps({{
                        "nodeid": report.nodeid, "worker": report.worker_id,
                    }}) + "\\n")
    """)
    files = []
    for name, count in (
        ("tests/test_bulk.py", 100),
        ("tests/test_other_a.py", 50),
        ("tests/test_other_b.py", 40),
        ("tests/test_other_c.py", 30),
        ("tests/architecture/test_module_imports.py", 1),
        ("tests/workflows/test_authoring.py", 1),
        ("tests/workflows/test_authoring_cli.py", 1),
    ):
        path = pytester.path / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            "import pytest\n"
            f"@pytest.mark.parametrize('case', range({count}))\n"
            "def test_case(case): pass\n"
        )
        files.append(str(path))
    options = (
        ["-p", "tests.support.scheduling", "--no-loadscope-reorder"]
        if prioritize
        else []
    )
    result = pytester.runpytest_subprocess(
        "-p", "xdist.plugin", "-n", "4", "--dist=loadfile", "-q", *options, *files
    )
    result.assert_outcomes(passed=223)
    workers: dict[str, set[str]] = defaultdict(set)
    first_files: dict[str, str] = {}
    for line in events.read_bytes().splitlines():
        event = object_value(parse(line))
        name = string(event["nodeid"]).split("::")[0]
        worker = string(event["worker"])
        workers[name].add(worker)
        first_files.setdefault(worker, name)
    assert all(len(assigned) == 1 for assigned in workers.values())
    assert set(first_files.values()) == (
        {
            "tests/architecture/test_module_imports.py",
            "tests/workflows/test_authoring.py",
            "tests/workflows/test_authoring_cli.py",
            "tests/test_bulk.py",
        }
        if prioritize
        else {
            "tests/test_bulk.py",
            "tests/test_other_a.py",
            "tests/test_other_b.py",
            "tests/test_other_c.py",
        }
    )
