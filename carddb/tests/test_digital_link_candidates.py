"""Complete frozen candidate checks and a color-independent offline CLI."""

import re
from pathlib import Path

import pytest
from pydantic import JsonValue
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.digital_links.candidates import CLASSES, generate
from sve_carddb.digital_links.commands import output_path
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse

from .digital_link_import_fixtures import Fixture, copied, current_api, make_fixture


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> Fixture:
    return make_fixture(tmp_path_factory.mktemp("digital-link-candidates"))


def draft(**updates: JsonValue) -> bytes:
    row: dict[str, JsonValue] = {
        "ja": "Synthetic card",
        "needs_decision": False,
        "relation": "same_card",
        "sve_numbers": ["SYN-001"],
        "relation_by_source": {"svwb": "same_card"},
        "sources": [{"source": "svwb", "card_id": 22345678, "zh": "合成測試名"}],
        "confidence": "high",
    }
    row.update(updates)
    return canonical([row])


def test_mechanical_tier_never_creates_adoption_or_leaks_names(
    baseline: Fixture,
) -> None:
    report = generate(draft(), baseline.sources())
    summary = object_value(report["summary"])
    assert summary["tier1"] == 1
    assert summary["tier2"] == 0
    unsigned = dict(report)
    unsigned.pop("result_hash")
    unsigned.pop("adoption_background")
    assert digest(canonical(unsigned)) == report["result_hash"]
    background = object_value(object_value(report["adoption_background"])["context"])
    config = object_value(parse(str(background["configuration"]).encode()))
    assert (
        object_value(config["digital_candidate_report"])["result_hash"]
        == report["result_hash"]
    )
    assert "digital_link_authored" not in config
    content = canonical(report)
    assert b"Synthetic card" not in content
    assert "合成測試名".encode() not in content
    assert b"sample_ids" not in content
    assert b"record_key" not in content
    assert (
        object_value(array(report["candidates"])[0])["phase_assignment"]
        == "requires_human_review"
    )


@pytest.mark.parametrize(
    ("change", "failure"),
    [
        ({"needs_decision": True}, "previous_pending"),
        ({"relation": "same_character"}, "not_same_card"),
        ({"relation": "mixed"}, "not_same_card"),
        ({"ja": "Synthetic different"}, "sve_exact_name_mismatch"),
        ({"sve_numbers": ["SYN-002"]}, "missing_sve_source"),
        (
            {"sources": [{"source": "svwb", "card_id": 22345679, "zh": "合成測試名"}]},
            "missing_digital_source",
        ),
        (
            {"sources": [{"source": "svwb", "card_id": 22345678, "zh": "不同測試名"}]},
            "draft_target_mismatch",
        ),
    ],
)
def test_pending_or_mixed_draft_remains_second_layer(
    baseline: Fixture, change: dict[str, JsonValue], failure: str
) -> None:
    report = generate(draft(**change), baseline.sources())
    candidate = object_value(array(report["candidates"])[0])
    assert candidate["tier"] == 2
    assert failure in array(candidate["failures"])


@pytest.mark.parametrize(
    "change",
    [
        "different_translation",
        "missing_translation",
        "class",
        "unknown_class",
        "type",
        "digital_name",
        "page_gap",
    ],
)
def test_frozen_catalogue_outside_selected_targets_is_checked(
    baseline: Fixture, tmp_path: Path, change: str
) -> None:
    def transform(data: dict[str, JsonValue], lang: str) -> None:
        details = object_value(data["card_details"])
        item = object_value(details["22345678"])
        common = object_value(item["common"])
        if change in {"different_translation", "missing_translation"}:
            other = object_value(parse(canonical(item)))
            other_common = object_value(other["common"])
            other_common.update(card_id=22345679, base_card_id=22345679)
            if lang == "zh-Hant":
                other_common["name"] = (
                    "另一個譯名" if change == "different_translation" else ""
                )
            details["22345679"] = other
            data["count"] = 2
        elif change in {"class", "unknown_class"}:
            common["class"] = 5 if change == "class" else 99
        elif change == "type":
            common["type"] = 4
        elif change == "digital_name" and lang == "ja":
            common["name"] = "Synthetix card"
        elif change == "page_gap":
            data["count"] = 31

    fixture = current_api(copied(baseline, tmp_path / "repo"), transform)
    if change == "page_gap":
        with pytest.raises(
            ValueError,
            match=r"^Digital candidate catalogue page closure is incomplete$",
        ):
            generate(draft(), fixture.sources())
        return
    report = generate(draft(), fixture.sources())
    flags = array(object_value(array(report["candidates"])[0])["failures"])
    expected = {
        "different_translation": "nonunique_or_missing_zh",
        "missing_translation": "nonunique_or_missing_zh",
        "class": "class_mismatch_or_unknown",
        "unknown_class": "class_mismatch_or_unknown",
        "type": "type_mismatch_or_unknown",
        "digital_name": "digital_exact_name_mismatch",
    }[change]
    assert expected in flags
    assert object_value(report["summary"])["tier1"] == 0


@pytest.mark.parametrize(
    ("game", "code", "expected"),
    [
        ("sv1", 5, "nightmare"),
        ("sv1", 6, "nightmare"),
        ("sv1", 8, None),
        ("svwb", 5, "nightmare"),
        ("svwb", 7, None),
        ("sv1", 99, None),
        ("svwb", 99, None),
    ],
)
def test_maintainer_enum_is_explicit(
    game: str, code: int, expected: str | None
) -> None:
    assert CLASSES[game].get(code) == expected


def test_cli_writes_only_private_report_under_color(
    baseline: Fixture, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("NO_COLOR", "1")
    monkeypatch.setenv("TERM", "dumb")
    private_draft = tmp_path / "draft.json"
    private_context = tmp_path / "context.json"
    output = tmp_path / "report.json"
    private_draft.write_bytes(draft())
    private_context.write_text(baseline.build.model_dump_json())
    before = baseline.inputs().load().pins()
    result = CliRunner().invoke(
        app,
        [
            "digital-links",
            "candidates",
            "--draft",
            str(private_draft),
            "--context",
            str(private_context),
            "--repository",
            str(baseline.root),
            "--store",
            "test-store=" + str(baseline.store),
            "--output",
            str(output),
        ],
    )
    assert result.exit_code == 0, result.exception
    assert object_value(object_value(parse(result.stdout.encode()))["by_game"])["svwb"]
    assert output.is_file()
    assert (
        object_value(object_value(parse(output.read_bytes()))["summary"])["tier1"] == 1
    )
    assert baseline.inputs().load().pins() == before


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("relative", "Candidate output must be absolute and not symlinked"),
        ("symlink", "Candidate output must be absolute and not symlinked"),
        ("protected", "Candidate output overlaps protected inputs"),
    ],
)
def test_output_refusal(tmp_path: Path, fault: str, message: str) -> None:
    protected = tmp_path / "protected"
    protected.mkdir()
    output = tmp_path / "report.json"
    if fault == "relative":
        output = Path("relative.json")
    elif fault == "symlink":
        output.symlink_to(protected / "report.json")
    elif fault == "protected":
        output = protected / "report.json"
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        output_path(output, (protected,))
