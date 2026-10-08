"""Each legacy constraint has an independent synthetic oracle or counterexample."""

import json
from pathlib import Path
from typing import TYPE_CHECKING, cast

import pytest
from typer.testing import CliRunner

from sve_carddb import cli
from sve_carddb.core.regions import SourceRegion as Region
from sve_carddb.domains.registry.inputs import Card
from sve_carddb.domains.registry.parser_adapters.official_en import (
    legacy_projection as legacy_en_projection,
)
from sve_carddb.domains.registry.review import observation
from sve_carddb.ingest.http.validate import ValidationError
from sve_carddb.ingest.http.writer import LocalState
from sve_carddb.parse.html import MissingElementError, parse, require_one
from sve_carddb.parse.pages import extract_en as official_en
from sve_carddb.parse.pages import official_en as en
from sve_carddb.workflows import extract as jsonl

from .en_extract_fixtures import face as make_face
from .en_extract_fixtures import page

if TYPE_CHECKING:
    from sve_carddb.ingest.archive.manifest import Manifest


FULL_TEXT = "First {synthetic.badge|[badge]}\nNext\n-----\nAuxiliary\n------\nLast"


@pytest.mark.parametrize(
    ("case", "number", "expected"),
    [
        (
            "F01",
            "SYN01-001EN",
            {
                "class": "Forestcraft",
                "type": "Follower",
                "rarity": "Legendary",
                "cost": "3",
                "text": "{synthetic|[fanfare]} SVE-KIT 合成F01段落020。\n"
                "{synthetic|[act]}{synthetic|[engage]}SVE-KIT 合成F01段落021。",
                "speech": "{synthetic|[forestcraft]}{synthetic|[cost02]} "
                "SVE-KIT 合成F01段落022。",
            },
        ),
        (
            "F02",
            "SYN01-SP01EN",
            {
                "class": "Abysscraft",
                "type": "Follower / Evolved",
                "rarity": "Special",
                "cost": "-",
                "text": "SVE-KIT 合成F02段落020。\nSVE-KIT 合成F02段落021。",
                "speech": None,
            },
        ),
    ],
    ids=["F01", "F02"],
)
def test_static_synthetic_page_has_an_independent_full_record(
    case: str, number: str, expected: dict[str, str | None]
) -> None:
    raw = (
        Path(__file__).parent / "fixtures" / "synthetic_en" / f"{case}-card.html"
    ).read_bytes()
    record = official_en.extract_card(raw, number=number)
    info = {
        "Format": "Any",
        "Class": expected["class"],
        "Card Type": expected["type"],
        "Trait": "SyntheticAlpha/SyntheticBeta",
        "Rarity": expected["rarity"],
        "Card Set": f"SVE-KIT 合成商品 {case}",
    }
    name = f"SVE-KIT 合成測試卡 {case}-01"
    image = f"/wordpress/wp-content/images/cardlist/synthetic/{number}-1.png"
    stats = {"cost": expected["cost"], "power": "3", "hp": "3"}
    [face] = record.faces
    assert record.number == number
    assert face.name == name
    assert face.info == info
    assert face.stats == stats
    assert face.image == image
    assert face.illustrator == f"SVE-KIT 合成插畫者 {case}-01"
    assert face.trait_raw == "SyntheticAlpha/SyntheticBeta"
    assert face.traits == ["SyntheticAlpha/SyntheticBeta"]
    assert face.text == face.raw_text == expected["text"]
    assert face.sections == []
    assert face.speech == expected["speech"]
    assert record.release_date is None
    assert record.errata_url is None
    assert record.notes == []
    assert record.qa == []
    assert record.products == []
    assert record.related_cards == []
    assert legacy_en_projection(record) == Card.model_validate(
        {
            "number": number,
            "faces": [
                {
                    "name": name,
                    "info": info,
                    "stats": stats,
                    "image": image,
                    "text": expected["text"],
                    "speech": expected["speech"],
                }
            ],
        }
    )


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
    actual = legacy_en_projection(record)
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
    assert legacy_en_projection(record).faces[0].text == text


def test_unknown_info_and_empty_auxiliary_section_are_retained() -> None:
    raw = (
        page()
        .replace(b"<dt>Universe</dt>", b"<dt>Future label</dt>")
        .replace(b"Auxiliary", b"")
    )
    record = official_en.extract_card(raw, number="SYNⓈ-01aEN")
    assert record.faces[0].info["Future label"] == "Synthetic universe"
    assert record.faces[0].sections == ["", "Last"]
    assert "Future label" in legacy_en_projection(record).faces[0].info


def test_markup_rendering_handles_nested_breaks_whitespace_and_exact_stem() -> None:
    tree = parse(
        '<div><span>A  \n B</span><br><p>C\t<img src="/x/a.multi.svg?z=1#fragment" alt="δ"></p><div>D</div></div>'
    )
    assert official_en.render(require_one(tree, "div")) == "A B\nC {a.multi|δ}\nD"


def test_icon_stem_preserves_case() -> None:
    tree = parse(
        '<div><img src="/icons/Synthetic.BadGe.svg?version=1#fragment" alt="badge"></div>'
    )
    assert official_en.render(require_one(tree, "div")) == "{Synthetic.BadGe|badge}"


@pytest.mark.parametrize("symbol", ["―", "─", "ー", "-"])
@pytest.mark.parametrize("length", [4, 5])
def test_section_separator_requires_at_least_five_symbols(
    symbol: str, length: int
) -> None:
    separator = symbol * length
    raw = page().replace(
        b"-----<br>Auxiliary<br>------<br>Last",
        separator.encode() + b"<br>Auxiliary",
    )
    record = official_en.extract_card(raw, number="SYNⓈ-01aEN")
    text = "First {synthetic.badge|[badge]}\nNext"
    complete = text + "\n" + separator + "\nAuxiliary"
    assert record.faces[0].raw_text == complete
    assert legacy_en_projection(record).faces[0].text == complete
    assert record.faces[0].text == (text if length == 5 else complete)
    assert record.faces[0].sections == (["Auxiliary"] if length == 5 else [])


def test_empty_back_face_name_fails_after_valid_front_face() -> None:
    raw = page(double=True).replace(b">Synthetic back</h1>", b"></h1>")
    en.parse_card(raw, expected_number="SYNⓈ-01aEN")
    with pytest.raises(ValidationError, match="Empty EN card name"):
        official_en.extract_card(raw, number="SYNⓈ-01aEN")


def test_back_face_image_without_src_fails_after_valid_front_face() -> None:
    raw = b"".join(
        page(double=True).rsplit('src="../exact/SYNⓈ-01aEN.png?v=7"'.encode(), 1)
    )
    en.parse_card(raw, expected_number="SYNⓈ-01aEN")
    with pytest.raises(ValidationError, match="EN card image has no src"):
        official_en.extract_card(raw, number="SYNⓈ-01aEN")


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


def test_explicit_empty_trait_retains_empty_value_instead_of_failing_or_inventing_dash() -> (
    None
):
    raw = page().replace(b"<dd>Alpha / Beta</dd>", b"<dd></dd>")
    empty_value = ""
    record = official_en.extract_card(raw, number="SYNⓈ-01aEN")
    assert record.faces[0].trait_raw == empty_value
    assert record.faces[0].traits == []
    assert record.faces[0].info["Trait"] == empty_value
    assert legacy_en_projection(record).faces[0].info["Trait"] == empty_value
