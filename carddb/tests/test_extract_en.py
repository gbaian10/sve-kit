"""Each legacy constraint has an independent synthetic oracle or counterexample."""

import hashlib
import json
from typing import TYPE_CHECKING, cast

import pytest
from typer.testing import CliRunner

from sve_carddb import cli
from sve_carddb.extract import compare_en, jsonl, official_en
from sve_carddb.fetch.validate import ValidationError
from sve_carddb.fetch.writer import LocalState
from sve_carddb.html import MissingElementError, parse, require_one
from sve_carddb.manifest import Region
from sve_carddb.registry.inputs import Card
from sve_carddb.registry.review import observation
from sve_carddb.sources import official_en as en

from .en_extract_fixtures import face as make_face
from .en_extract_fixtures import page

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.manifest import Manifest


FULL_TEXT = "First {synthetic.badge|[badge]}\nNext\n-----\nAuxiliary\n------\nLast"


def test_every_face_raw_value_and_page_hint_is_preserved() -> None:
    record = official_en.extract_card(page(double=True), number="SYNⓈ-01aEN")
    assert record.number == "SYNⓈ-01aEN"
    assert len(record.faces) == 2
    assert [face.name for face in record.faces] == ["Synthetic front", "Synthetic back"]
    for face in record.faces:
        assert face.info == {
            "Class": "Synthetic class",
            "Card Type": "Synthetic type",
            "Trait": "Alpha / Beta",
            "Rarity": "LG",
            "Card Set": "Set\nEdition",
            "Universe": "Synthetic universe",
        }
        assert face.stats == {"cost": "-", "power": "02", "hp": "X"}
        assert face.trait_raw == "Alpha / Beta"
        assert face.traits == ["Alpha", "Beta"]
        assert face.text == "First {synthetic.badge|[badge]}\nNext"
        assert face.sections == ["Auxiliary", "Last"]
        assert face.raw_text == FULL_TEXT
        assert face.speech == "Voice {synthetic.voice|voice}"
        assert face.illustrator == "Synthetic artist"
        assert face.image == "../exact/SYNⓈ-01aEN.png?v=7"
    assert record.release_date == "Synthetic release date"
    assert record.errata_url == "/errata/exact?x=1"
    assert record.notes == ["Notice"]
    assert record.qa[0].title == "Synthetic QA title"
    assert record.qa[0].question == "Question"
    assert record.qa[0].answer == "Answer\nMore"
    assert record.products[0].name == "Synthetic product"
    assert record.products[0].date == "Synthetic release date"
    assert record.products[0].links == ["/products/exact"]
    assert record.related_cards[0].href == "/cards/?cardno=EXACTaEN"
    assert record.related_cards[0].label == "Related"


def test_legacy_projection_matches_independently_written_full_card() -> None:
    record = official_en.extract_card(page(), number="SYNⓈ-01aEN")
    expected = Card.model_validate(
        {
            "number": "SYNⓈ-01aEN",
            "faces": [
                {
                    "name": "Synthetic front",
                    "info": {
                        "Class": "Synthetic class",
                        "Card Type": "Synthetic type",
                        "Trait": "Alpha / Beta",
                        "Rarity": "LG",
                        "Card Set": "Set\nEdition",
                        "Universe": "Synthetic universe",
                    },
                    "stats": {"cost": "-", "power": "02", "hp": "X"},
                    "text": FULL_TEXT,
                    "speech": "Voice {synthetic.voice|voice}",
                    "image": "../exact/SYNⓈ-01aEN.png?v=7",
                }
            ],
        }
    )
    actual = official_en.legacy_projection(record)
    assert actual == expected
    assert observation(actual, "en") == observation(expected, "en")


@pytest.mark.parametrize(
    ("before", "after", "error"),
    [
        (b"<dl><dt>Class</dt><dd>Synthetic class</dd></dl>", b"", ValidationError),
        (b"<dl><dt>Card Type</dt><dd>Synthetic type</dd></dl>", b"", ValidationError),
        (b"<dl><dt>Rarity</dt><dd>LG</dd></dl>", b"", ValidationError),
        (b"<dl><dt>Trait</dt><dd>Alpha / Beta</dd></dl>", b"", ValidationError),
        (b"<dd>Synthetic class</dd>", b"<dd></dd>", ValidationError),
        (b"<dt>Universe</dt>", b"<dt>Class</dt>", ValidationError),
        (b"<dt>Universe</dt>", b"<dt></dt>", ValidationError),
        (b">Synthetic front</h1>", b"></h1>", ValidationError),
        ('src="../exact/SYNⓈ-01aEN.png?v=7"'.encode(), b"", ValidationError),
        (
            b'<span class="heading">Cost</span>-</div>',
            b'<span class="heading">Cost</span>-</div><div class="status-Item status-Item-Cost"><span class="heading">Cost</span>8</div>',
            ValidationError,
        ),
        (b"status-Item-Power", b"unrecognized-stat", ValidationError),
        (b"status-Item-Power", b"status-Item-Power status-Item-Cost", ValidationError),
        (b">02</div>", b"></div>", ValidationError),
        (b">Power</span>", b"></span>", ValidationError),
        (b'<span class="heading">HP</span>', b"", MissingElementError),
        (b'src="/icons/synthetic.badge.png?v=1"', b"", ValidationError),
        (b'alt="[badge]"', b"", ValidationError),
        (b'src="/icons/synthetic.badge.png?v=1"', b'src="/"', ValidationError),
    ],
    ids=[
        "missing-class",
        "missing-type",
        "missing-rarity",
        "missing-trait",
        "empty-class",
        "duplicate-label",
        "empty-label",
        "empty-name",
        "missing-image",
        "duplicate-stat",
        "missing-stat",
        "ambiguous-stat",
        "empty-stat",
        "empty-heading",
        "missing-heading",
        "icon-missing-src",
        "icon-missing-alt",
        "icon-missing-stem",
    ],
)
def test_missing_duplicate_and_ambiguous_values_fail_closed(
    before: bytes, after: bytes, error: type[Exception]
) -> None:
    raw = page()
    assert before in raw
    with pytest.raises(error):
        official_en.extract_card(raw.replace(before, after), number="SYNⓈ-01aEN")


def test_exact_number_is_validated_without_suffix_inference() -> None:
    with pytest.raises(ValidationError):
        official_en.extract_card(page("SYN-001EN"), number="SYN-001")


def test_more_than_two_faces_is_rejected() -> None:
    raw = page(double=True).replace(
        b'<div class="illustrator"><a',
        make_face("SYNⓈ-01aEN", back=True).encode() + b'<div class="illustrator"><a',
        1,
    )
    with pytest.raises(ValidationError):
        official_en.extract_card(raw, number="SYNⓈ-01aEN")


@pytest.mark.parametrize(
    ("replacement", "text", "speech"),
    [
        (b"", None, None),
        (b'<div class="detail"></div><div class="speech"></div>', "", ""),
    ],
    ids=["missing", "empty"],
)
def test_missing_and_empty_text_are_distinct(
    replacement: bytes, text: str | None, speech: str | None
) -> None:
    raw = page()
    start = raw.index(b'<div class="detail">')
    end = raw.index(b'<div class="illustrator">')
    record = official_en.extract_card(
        raw[:start] + replacement + raw[end:], number="SYNⓈ-01aEN"
    )
    assert record.faces[0].raw_text == text
    assert record.faces[0].text == text
    assert record.faces[0].sections == []
    assert record.faces[0].speech == speech
    assert official_en.legacy_projection(record).faces[0].text == text


def test_unknown_info_and_empty_auxiliary_section_are_retained() -> None:
    raw = (
        page()
        .replace(b"<dt>Universe</dt>", b"<dt>Future label</dt>")
        .replace(b"Auxiliary", b"")
    )
    record = official_en.extract_card(raw, number="SYNⓈ-01aEN")
    assert record.faces[0].info["Future label"] == "Synthetic universe"
    assert record.faces[0].sections == ["", "Last"]
    assert "Future label" in official_en.legacy_projection(record).faces[0].info


def test_markup_rendering_handles_nested_breaks_whitespace_and_exact_stem() -> None:
    tree = parse(
        '<div><span>A  \n B</span><br><p>C\t<img src="/x/a.multi.svg?z=1#fragment" alt="δ"></p><div>D</div></div>'
    )
    assert official_en.render(require_one(tree, "div")) == "A B\nC {a.multi|δ}\nD"


def test_measurement_explicitly_selects_production_adapter() -> None:
    card = compare_en.parse_legacy_card(page(), "SYNⓈ-01aEN")
    assert card.faces[0].text == FULL_TEXT
    baseline = compare_en.parse_card(page(), "SYNⓈ-01aEN")
    assert baseline.faces[0].text != card.faces[0].text


def test_en_jsonl_uses_exact_region_urls_and_serializes_record(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    def numbers(_manifest: Manifest, *, region: Region) -> list[str]:
        assert region is Region.EN
        return ["SYNⓈ-01aEN", "MISSING", "BAD"]

    class Reader:
        def local_state(self, url: str) -> LocalState:
            return (
                LocalState.MISSING
                if url == en.card_url("MISSING")
                else LocalState.TRUSTED
            )

        def read(self, url: str) -> bytes:
            assert url in {en.card_url("SYNⓈ-01aEN"), en.card_url("BAD")}
            return page() if url == en.card_url("SYNⓈ-01aEN") else b"invalid"

    monkeypatch.setattr(jsonl, "card_numbers", numbers)
    out = tmp_path / "output.jsonl"
    report = jsonl.extract_cards(
        cast("Manifest", object()), Reader(), out, region=Region.EN
    )
    assert report.written == 1
    assert report.missing == ["MISSING"]
    assert report.failed == {"BAD": "ValidationError"}
    record = json.loads(out.read_text())
    assert record["faces"][0]["raw_text"] == FULL_TEXT
    assert record["faces"][0]["sections"] == ["Auxiliary", "Last"]
    assert record["number"] == "SYNⓈ-01aEN"


@pytest.mark.parametrize("region", [Region.SV1, Region.SVWB])
def test_digital_regions_rejected_before_writing(
    tmp_path: Path, region: Region
) -> None:
    out = tmp_path / "output.jsonl"
    with pytest.raises(ValueError, match="only JP and EN"):
        jsonl.extract_cards(
            cast("Manifest", object()),
            cast("jsonl.RawReader", object()),
            out,
            region=region,
        )
    assert not out.exists()
    result = CliRunner().invoke(
        cli.app,
        [
            "archive",
            "extract-cards",
            str(tmp_path / "store"),
            "--store-id",
            "s",
            "sha256:" + "0" * 64,
            str(out),
            "--region",
            region.value,
        ],
    )
    assert result.exit_code == 1
    assert "only JP and EN" in result.stdout


def test_legacy_checksum_precedes_parsing_and_binds_the_same_bytes(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = tmp_path / "old.jsonl"
    raw = b"not-json: synthetic private input\n"
    path.write_bytes(raw)

    def forbidden(_raw: bytes) -> Card:
        raise AssertionError("Legacy parser must not run before checksum verification")

    monkeypatch.setattr(Card, "model_validate_json", forbidden)
    with pytest.raises(ValueError, match="Legacy input hash mismatch"):
        compare_en.read_legacy(path, "sha256:" + "0" * 64)
    assert path.read_bytes() == raw
    assert "sha256:" + hashlib.sha256(raw).hexdigest() != "sha256:" + "0" * 64


def test_legacy_validation_errors_do_not_echo_input_and_duplicates_fail(
    tmp_path: Path,
) -> None:
    path = tmp_path / "old.jsonl"
    path.write_bytes(b'{"text":"synthetic private input"}\n')
    with pytest.raises(ValueError, match="Invalid legacy EN input") as failure:
        compare_en.read_legacy(path, None)
    assert "synthetic private input" not in str(failure.value)
    card = compare_en.parse_legacy_card(page(), "SYNⓈ-01aEN")
    raw = card.model_dump_json().encode() + b"\n"
    path.write_bytes(raw)
    expected_hash = "sha256:" + hashlib.sha256(raw).hexdigest()
    found, checksum = compare_en.read_legacy(path, expected_hash)
    assert found == {"SYNⓈ-01aEN": card}
    assert checksum == expected_hash
    path.write_bytes(raw + raw)
    with pytest.raises(ValueError, match="Duplicate legacy EN card number"):
        compare_en.read_legacy(path, None)


def test_renderer_mismatch_stays_mismatch_in_candidate_report_and_legacy_is_exact() -> (
    None
):
    old = compare_en.parse_card(page(), "SYNⓈ-01aEN")
    old.faces[0].text = FULL_TEXT
    old.faces[0].speech = "Voice {synthetic.voice|voice}"
    expected = observation(old, "en")

    class Reader:
        def local_state(self, _url: str) -> LocalState:
            return LocalState.TRUSTED

        def read(self, _url: str) -> bytes:
            return page()

    selected: list[tuple[str, str, str, str, str | None]] = [
        (
            old.number,
            "synthetic-printing",
            str(expected["observation_hash"]),
            str(expected["rules_hash"]),
            None,
        )
    ]
    candidate = compare_en.measure(selected, {old.number: old}, Reader())
    legacy = compare_en.measure(
        selected, {old.number: old}, Reader(), parser=compare_en.parse_legacy_card
    )
    assert candidate["counts"] == {
        "exact": 0,
        "mismatch": 1,
        "missing_raw": 0,
        "parse_failed": 0,
        "no_corresponding_input": 0,
    }
    assert legacy["counts"] == {
        "exact": 1,
        "mismatch": 0,
        "missing_raw": 0,
        "parse_failed": 0,
        "no_corresponding_input": 0,
    }
    assert "First" not in json.dumps(candidate)
    assert "Voice" not in json.dumps(legacy)


def test_explicit_empty_trait_retains_empty_value_instead_of_failing_or_inventing_dash() -> (
    None
):
    raw = page().replace(b"<dd>Alpha / Beta</dd>", b"<dd></dd>")
    empty_value = ""
    record = official_en.extract_card(raw, number="SYNⓈ-01aEN")
    assert record.faces[0].trait_raw == empty_value
    assert record.faces[0].traits == []
    assert record.faces[0].info["Trait"] == empty_value
    assert official_en.legacy_projection(record).faces[0].info["Trait"] == empty_value
