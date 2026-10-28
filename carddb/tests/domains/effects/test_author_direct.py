"""Direct admission checks actual lines and types without source-adapter proofs."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains.effects.author_direct import materialize
from sve_carddb.domains.effects.author_handwritten import Body, Handwritten
from sve_carddb.domains.effects.author_inputs import parse_document
from sve_carddb.domains.effects.author_sources import SourceError
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner

from ...support.direct_author_fixture import direct_author_input
from ..text_observations.test_effect_presence import page
from .test_author_current_absent import synthetic_current

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue


def test_direct_writer_selects_pending_text_without_machine_anchors(
    tmp_path: Path,
) -> None:
    selection = object_value(direct_author_input(tmp_path))
    accepted = object_value(array(selection["authors"])[0])
    assert accepted["status"] == "accepted"
    handoff = object_value(accepted["input"])
    assert handoff["admission"] == "direct"
    assert not (tmp_path / "authored/rules/effect-anchors").exists()


@pytest.mark.parametrize(
    "mutation",
    ["missing", "duplicate", "wrong_face", "bad_param", "disputed", "unknown_key"],
)
def test_direct_rejects_incomplete_or_invalid_cards(
    tmp_path: Path, mutation: str
) -> None:
    card: dict[str, JsonValue] = {
        "admission": "light",
        "meta": {
            "written_by": [
                {"actor": {"who": "test", "model": None}, "date": "2026-10-27"}
            ]
        },
        "abilities": [
            {"line": 1, "spell": {"do": [{"draw": {"who": "self", "n": 1}}]}}
        ],
    }
    values = array(card["abilities"])
    expected = "source_line_closure_mismatch"
    bodies = {}
    if mutation == "missing":
        values.clear()
    elif mutation == "duplicate":
        values.append(values[0])
        expected = "duplicate_source_ability"
    elif mutation == "wrong_face":
        object_value(values[0])["face"] = 2
        expected = "missing_face"
    elif mutation == "bad_param":
        values[0] = {"line": 1, "use": "draw", "params": {"n": True}}
        bodies["draw", 1] = Body(
            id="draw",
            params={"n": "Nat"},
            written_by=[
                {"actor": {"who": "test", "model": None}, "date": "2026-10-27"}
            ],
            body={"spell": {"do": [{"draw": {"who": "self", "n": {"param": "n"}}}]}},
        )
        expected = "param_type_mismatch"
    elif mutation == "unknown_key":
        card["deck_rules"] = []
        expected = "invalid_card_keys"
    else:
        object_value(card["meta"])["status"] = "disputed"
        expected = "source_unresolved_issue"
    handwritten = Handwritten(
        {"SYN-01": card}, bodies, {"SYN-01": parse_document(canonical(card))}
    )
    with (
        synthetic_current(
            tmp_path / "source",
            page("jp", '<div class="detail">Unknown wording.</div>'),
        ) as current,
        pytest.raises(SourceError, match=expected),
    ):
        materialize(current, "SYN-01", handwritten)


@pytest.mark.parametrize(
    ("kind", "text", "ignore", "accepted"),
    [
        ("unknown", "Synthetic reminder.", True, True),
        ("unknown", "Synthetic reminder.", False, False),
        ("unknown", "First line.\nSecond line.", True, False),
        ("token_definition", "Synthetic token.", True, False),
    ],
)
def test_unknown_section_requires_complete_explicit_reminder_coverage(
    tmp_path: Path, kind: str, text: str, *, ignore: bool, accepted: bool
) -> None:
    card: dict[str, JsonValue] = {
        "admission": "shared",
        "meta": {
            "written_by": [
                {"actor": {"who": "test", "model": None}, "date": "2026-10-27"}
            ]
        },
        "abilities": [{"line": 1, "fanfare": {"do": [{"draw": {"n": 1}}]}}],
        "ignored": [{"section": 1, "line": 1, "reason": "reminder"}] if ignore else [],
    }
    handwritten = Handwritten(
        {"SYN-01": card}, {}, {"SYN-01": parse_document(canonical(card))}
    )
    with synthetic_current(
        tmp_path, page("jp", '<div class="detail">Synthetic rule.</div>')
    ) as current:
        with current.db.transaction():
            unit = TextInterner(current.db).intern(LocalizedText(lang="ja", text=text))
            current.db.insert(
                "face_text_section",
                {
                    "revision_id": "revision",
                    "ordinal": 0,
                    "text_unit_id": unit,
                    "kind": kind,
                    "decision_id": None,
                },
            )
        if accepted:
            result = materialize(current, "SYN-01", handwritten)
            assert len(array(result["ignored"])) == 1
        else:
            with pytest.raises(SourceError, match="token_source_contract_unavailable"):
                materialize(current, "SYN-01", handwritten)


@pytest.mark.parametrize("kind", ["token_definition", "unknown"])
def test_token_section_links_a_shared_card_without_creating_a_mother_ability(
    tmp_path: Path, kind: str
) -> None:
    card: dict[str, JsonValue] = {
        "admission": "light",
        "meta": {
            "written_by": [
                {"actor": {"who": "test", "model": None}, "date": "2026-10-28"}
            ]
        },
        "abilities": [{"line": 1, "fanfare": {"do": [{"draw": {"n": 1}}]}}],
        "token_sources": [{"section": 1, "line": 1, "card": "TOKEN-01"}],
    }
    handwritten = Handwritten(
        {"SYN-01": card}, {}, {"SYN-01": parse_document(canonical(card))}
    )
    with synthetic_current(
        tmp_path, page("jp", '<div class="detail">Synthetic rule.</div>')
    ) as current:
        with current.db.transaction():
            unit = TextInterner(current.db).intern(
                LocalizedText(lang="ja", text="Synthetic token definition.")
            )
            current.db.insert(
                "face_text_section",
                {
                    "revision_id": "revision",
                    "ordinal": 0,
                    "text_unit_id": unit,
                    "kind": kind,
                    "decision_id": None,
                },
            )
        result = materialize(current, "SYN-01", handwritten)
        faces = object_value(result["faces"])
        assert (
            sum(len(array(object_value(face)["abilities"])) for face in faces.values())
            == 1
        )
        assert len(array(result["token_sources"])) == 1
        array(card["abilities"]).append(
            {"section": 1, "line": 1, "fanfare": {"do": [{"draw": {"n": 1}}]}}
        )
        with pytest.raises(SourceError, match="duplicate_source_ability"):
            materialize(current, "SYN-01", handwritten)
