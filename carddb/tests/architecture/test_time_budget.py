"""Exercise the time budget as a pytest plugin in isolated child runs."""

from pathlib import Path

import pytest

pytest_plugins = ("pytester",)


@pytest.fixture
def budget_run(
    pytester: pytest.Pytester, monkeypatch: pytest.MonkeyPatch
) -> pytest.Pytester:
    monkeypatch.setenv("PYTHONPATH", str(Path(__file__).resolve().parents[2]))
    monkeypatch.setenv("PYTEST_DISABLE_PLUGIN_AUTOLOAD", "1")
    monkeypatch.delenv("PYTEST_ADDOPTS", raising=False)
    monkeypatch.setenv("CI", "true")
    monkeypatch.setenv("SVE_TEST_TIME_LIMIT", "0.1")
    pytester.makeini("[pytest]\naddopts=\n")
    pytester.makeconftest('pytest_plugins = ("tests.support.time_budget",)')
    return pytester


def test_setup_call_and_teardown_share_one_budget(budget_run: pytest.Pytester) -> None:
    budget_run.makepyfile("""
        import time
        import pytest

        @pytest.fixture
        def prepared():
            time.sleep(0.04)
            yield
            time.sleep(0.04)

        def test_too_slow(prepared):
            time.sleep(0.04)
    """)
    result = budget_run.runpytest_subprocess("-q")
    assert result.ret == pytest.ExitCode.TESTS_FAILED
    result.stdout.fnmatch_lines(["*Test time budget exceeded:*test_too_slow*"])
    assert "setup + call + teardown" in result.stdout.str()
    assert '@pytest.mark.slow(reason="concrete reason")' in result.stdout.str()


def test_slow_reason_exempts_a_test(budget_run: pytest.Pytester) -> None:
    budget_run.makepyfile("""
        import time
        import pytest

        @pytest.mark.slow(reason="Real public-profile round trip")
        def test_slow():
            time.sleep(0.2)
    """)
    result = budget_run.runpytest_subprocess("-q")
    result.assert_outcomes(passed=1)
    assert "Real public-profile round trip" in result.stdout.str()


@pytest.mark.parametrize("marker", ["slow", 'slow(reason="  ")', "slow(reason=1)"])
def test_missing_or_invalid_reason_is_an_error(
    budget_run: pytest.Pytester, marker: str
) -> None:
    budget_run.makepyfile(
        f"import pytest\n@pytest.mark.{marker}\ndef test_case(): pass"
    )
    result = budget_run.runpytest_subprocess("-q")
    assert result.ret == pytest.ExitCode.USAGE_ERROR
    assert "requires a non-empty reason string" in result.stderr.str()


def test_unmarked_shared_preparation_is_charged_once(
    budget_run: pytest.Pytester,
) -> None:
    budget_run.makepyfile("""
        import time
        import pytest

        @pytest.fixture(scope="session")
        def shared():
            time.sleep(0.2)

        def test_first(shared): pass
        def test_second(shared): pass
    """)
    result = budget_run.runpytest_subprocess("-q")
    result.assert_outcomes(passed=2, errors=1)
    result.stdout.fnmatch_lines(["*Test time budget exceeded:*test_first*"])
    assert (
        "Test time budget exceeded: test_unmarked_shared_preparation_is_charged_once.py::test_second"
        not in result.stdout.str()
    )


def test_marked_shared_setup_and_finalizer_leave_consumer_budget(
    budget_run: pytest.Pytester,
) -> None:
    budget_run.makepyfile("""
        import time
        import pytest
        from tests.support.time_budget import slow_fixture

        @pytest.fixture(scope="session")
        @slow_fixture(reason="Build a real shared source database once")
        def shared():
            time.sleep(0.2)
            yield
            time.sleep(0.2)

        def test_first(shared): pass
        def test_second(shared): pass
        def test_consumer_still_has_a_budget(shared): time.sleep(0.2)
    """)
    result = budget_run.runpytest_subprocess("-q")
    result.assert_outcomes(passed=3, errors=1)
    result.stdout.fnmatch_lines(
        ["*Test time budget exceeded:*test_consumer_still_has_a_budget*"]
    )
    assert "Build a real shared source database once" in result.stdout.str()


def test_local_over_budget_warns(
    budget_run: pytest.Pytester, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("CI", "false")
    budget_run.makepyfile("""
        import time
        def test_local(): time.sleep(0.2)
    """)
    result = budget_run.runpytest_subprocess("-q")
    result.assert_outcomes(passed=1)
    assert "PytestWarning: Test time budget exceeded" in result.stdout.str()


@pytest.mark.parametrize("limit", ["0", "-1", "nan", "inf", "invalid"])
def test_invalid_limit_is_an_error(
    budget_run: pytest.Pytester, monkeypatch: pytest.MonkeyPatch, limit: str
) -> None:
    monkeypatch.setenv("SVE_TEST_TIME_LIMIT", limit)
    budget_run.makepyfile("def test_case(): pass")
    result = budget_run.runpytest_subprocess("-q")
    assert result.ret == pytest.ExitCode.USAGE_ERROR
    assert "positive finite number" in result.stderr.str()


def test_shared_fixture_requires_a_reason(budget_run: pytest.Pytester) -> None:
    budget_run.makepyfile("""
        import pytest
        from tests.support.time_budget import slow_fixture

        @pytest.fixture(scope="session")
        @slow_fixture(reason=" ")
        def shared(): pass

        def test_case(shared): pass
    """)
    result = budget_run.runpytest_subprocess("-q")
    assert result.ret != pytest.ExitCode.OK
    assert "slow_fixture requires a non-empty reason string" in result.stdout.str()
