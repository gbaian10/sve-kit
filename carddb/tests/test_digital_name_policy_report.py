"""Explicit offline CLI, private output boundaries and nonpublication diagnostics."""

import re
from dataclasses import replace
from pathlib import Path

import pytest
from pydantic import JsonValue
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.digital_name_policies.commands import report_command
from sve_carddb.digital_name_policies.report import generate
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse

from .digital_name_policy_fixtures import PolicyFixture, make_policy_fixture


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> PolicyFixture:
    return make_policy_fixture(tmp_path_factory.mktemp("name-policy-report"))


@pytest.fixture(scope="module")
def report(baseline: PolicyFixture) -> dict[str, JsonValue]:
    return generate(baseline.snapshot(), baseline.digital.sources(), None)


def test_report_contains_no_names_and_cannot_claim_publication(
    report: dict[str, JsonValue],
) -> None:
    raw = canonical(report)
    assert b"Synthetic card" not in raw
    assert "合成測試名".encode() not in raw
    summary = object_value(report["summary"])
    assert summary["owners_eligible"] == 2
    assert summary["rule_link_plans"] == 2
    assert summary["published_links"] == 0
    assert summary["published_translations"] == 0
    assert summary["historical_warnings_unavailable"] is True
    assert summary["coverage_adopted"] is False
    unsigned = dict(report)
    checksum = unsigned.pop("report_hash")
    assert checksum == digest(canonical(unsigned))


def test_explicit_baseline_detects_new_owners(
    baseline: PolicyFixture, report: dict[str, JsonValue]
) -> None:
    after = generate(baseline.snapshot(), baseline.digital.sources(), canonical(report))
    assert object_value(after["summary"])["new_owners"] == 0
    assert after["baseline_hash"] == digest(canonical(report))


def test_comparison_does_not_replace_pinned_catalogue(
    baseline: PolicyFixture, report: dict[str, JsonValue]
) -> None:
    after = generate(
        baseline.snapshot(),
        baseline.digital.sources(),
        canonical(report),
        baseline.digital.sources(),
    )
    assert after["policy_inputs"] == report["policy_inputs"]
    assert (
        object_value(after["newer_catalogue"])["activation"]
        == "report_only_continuation_format_unsupported"
    )
    assert object_value(after["newer_catalogue"])["changes"] == []


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("hash", "Digital-name report baseline recipe or hash mismatch"),
        ("keys", "Digital-name report baseline owner keys are invalid"),
        (
            "context",
            "Digital-name report context must contain only source configuration",
        ),
        ("runtime", "Digital-name policy runtime dependency closure mismatch"),
    ],
)
def test_diagnostic_refusals(
    baseline: PolicyFixture, report: dict[str, JsonValue], fault: str, message: str
) -> None:
    sources = baseline.digital.sources()
    raw = object_value(parse(canonical(report)))
    if fault == "hash":
        raw["report_hash"] = digest(b"wrong")
    elif fault == "keys":
        rows = array(raw["owners"])
        rows.append(rows[0])
        raw.pop("report_hash")
        raw["report_hash"] = digest(canonical(raw))
    elif fault == "context":
        config = object_value(parse(sources.build.configuration.encode()))
        config["unreviewed_extra"] = True
        sources.build = sources.build.model_copy(
            update={"configuration": canonical(config).decode()}
        )
    else:
        sources.build = sources.build.model_copy(
            update={
                "dependencies": tuple(
                    p
                    for p in sources.build.dependencies
                    if not p.name.endswith("digital_name_policies/evaluate.py")
                )
            }
        )
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        generate(baseline.snapshot(), sources, canonical(raw))


def cli_args(baseline: PolicyFixture, context: Path, output: Path) -> list[str]:
    return [
        "digital-name-policies",
        "report",
        "--authored",
        str(baseline.root / "authored"),
        "--authored-revision",
        baseline.authored,
        "--context",
        str(context),
        "--repository",
        str(baseline.root),
        "--store",
        "test-store=" + str(baseline.digital.store),
        "--output",
        str(output),
    ]


def test_cli_color_and_repeated_report_are_deterministic(
    baseline: PolicyFixture, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("FORCE_COLOR", "1")
    monkeypatch.setenv("TERM", "xterm-256color")
    monkeypatch.setenv("NO_COLOR", "1")
    context = tmp_path / "context.json"
    output = tmp_path / "report.json"
    context.write_text(baseline.digital.build.model_dump_json())
    before = baseline.snapshot().pins()
    runner = CliRunner()
    result = runner.invoke(
        app, [*cli_args(baseline, context, output), "--baseline-empty"]
    )
    assert result.exit_code == 0, result.exception
    assert object_value(parse(result.stdout.encode()))["rule_link_plans"] == 2
    first = output.read_bytes()
    assert (
        runner.invoke(
            app, [*cli_args(baseline, context, output), "--baseline-empty"]
        ).exit_code
        == 0
    )
    assert output.read_bytes() == first
    assert baseline.snapshot().pins() == before


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("store", "Policy report store must be unique id=absolute_non_symlink_path"),
        ("baseline", "Policy report requires exactly one explicit baseline"),
        ("relative", "Candidate output must be absolute and not symlinked"),
        ("overlap", "Candidate output overlaps protected inputs"),
    ],
)
def test_cli_rejects_before_loading_any_input(
    tmp_path: Path, fault: str, message: str
) -> None:
    output = tmp_path / "report.json"
    stores = ["synthetic=" + str(tmp_path / "archive")]
    empty = True
    if fault == "store":
        stores = ["synthetic=relative"]
    elif fault == "baseline":
        empty = False
    elif fault == "relative":
        output = Path("relative.json")
    else:
        output = tmp_path / "authored/report.json"
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        report_command(
            tmp_path / "authored",
            "0" * 40,
            tmp_path / "unread.json",
            tmp_path / "repo",
            stores,
            output,
            baseline_empty=empty,
        )
    assert not output.exists()


def test_newer_translation_change_is_reported_without_activation(
    baseline: PolicyFixture,
) -> None:
    comparing = baseline.digital.sources()
    from sve_carddb.digital_links.catalogue import complete_inventory  # ruff: ignore[import-outside-top-level] -- only this diagnostic uses a deliberately changed detached projection
    from sve_carddb.digital_links.importer import review_context  # ruff: ignore[import-outside-top-level] -- same explicit context as the comparison source

    complete_inventory(comparing, review_context(comparing), "sv1")
    for (_, _, parser), (lang, document, _) in comparing.cache.items():
        if parser == "translation-sv1-v1" and lang == "zh-Hant":
            cards = array(object_value(object_value(document)["data"])["cards"])
            object_value(cards[0])["card_name"] = "另一個合成譯名"
    result = generate(baseline.snapshot(), baseline.digital.sources(), None, comparing)
    changes = object_value(result["newer_catalogue"])
    assert len(array(changes["changes"])) == 2
    assert all(
        object_value(row)["translation_changed"]
        for row in array(changes["owner_changes"])
    )
    assert object_value(result["summary"])["published_translations"] == 0


def test_name_and_link_policies_keep_separate_catalogue_pins(
    baseline: PolicyFixture,
) -> None:
    from sve_carddb.digital_name_policies.loader import LoadedPolicy  # ruff: ignore[import-outside-top-level] -- the detached metadata change simulates independently approved catalogue provenance

    snapshot = baseline.snapshot()
    links = snapshot.effective("links")
    raw = links.document().model_dump(mode="json")
    object_value(object_value(raw["content"])["catalogue_pins"])[
        "private_name_list_hash"
    ] = digest(b"different private diagnostic metadata")
    different = LoadedPolicy(canonical(raw), links.approval, links.exclusions)
    snapshot = replace(
        snapshot,
        policies=tuple(different if p is links else p for p in snapshot.policies),
    )
    result = generate(snapshot, baseline.digital.sources(), None)
    assert len(array(result["policy_catalogue_input_records"])) == 2
