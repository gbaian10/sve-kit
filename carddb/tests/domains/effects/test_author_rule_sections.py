"""Explicit rule sections require ability coverage without reclassifying sources."""

from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains.effects.author_direct import materialize
from sve_carddb.domains.effects.author_handwritten import Handwritten
from sve_carddb.domains.effects.author_inputs import (
    AuthorInputError,
    AuthorSchemas,
    parse_document,
)
from sve_carddb.domains.effects.author_sources import SourceError
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner

from ..text_observations.test_effect_presence import page
from .test_author_current_absent import synthetic_current

if TYPE_CHECKING:
    from pydantic import JsonValue


@pytest.fixture(scope="module")
def schemas() -> AuthorSchemas:
    return AuthorSchemas(Path(__file__).resolve().parents[4] / "dsl")


def rule_card() -> dict[str, JsonValue]:
    return {
        "admission": "light",
        "meta": {
            "written_by": [
                {"actor": {"who": "test", "model": None}, "date": "2026-10-29"}
            ]
        },
        "rule_sections": [{"section": 1}],
        "abilities": [
            {"line": 1, "fanfare": {"do": [{"draw": {"n": 1}}]}},
            {
                "line": 1,
                "section": 1,
                "covers": [1, 2],
                "fanfare": {"do": [{"draw": {"n": 1}}]},
            },
        ],
    }


@pytest.mark.parametrize(
    ("case", "expected"),
    [
        ("complete", None),
        ("missing_line", "source_line_closure_mismatch"),
        ("reminder_filler", "source_line_closure_mismatch"),
        ("unmarked", "token_source_contract_unavailable"),
        ("token_definition", "rule_section_kind_mismatch"),
        ("reminder", "rule_section_kind_mismatch"),
        ("duplicate", "duplicate_rule_section"),
        ("wrong_section", "source_location_mismatch"),
        ("wrong_face", "missing_face"),
        ("empty", "source_line_closure_mismatch"),
    ],
)
def test_rule_sections_require_complete_ability_coverage(
    tmp_path: Path, case: str, expected: str | None
) -> None:
    card = rule_card()
    abilities = array(card["abilities"])
    if case in {"missing_line", "reminder_filler"}:
        object_value(abilities[1])["covers"] = [1]
    if case == "reminder_filler":
        card["ignored"] = [{"section": 1, "line": 2, "reason": "reminder"}]
    if case == "unmarked":
        del card["rule_sections"]
    if case == "duplicate":
        array(card["rule_sections"]).append({"section": 1, "face": 1})
    if case == "wrong_section":
        card["rule_sections"] = [{"section": 2}]
    if case == "wrong_face":
        card["rule_sections"] = [{"section": 1, "face": 2}]
    if case == "empty":
        abilities.pop()
    handwritten = Handwritten(
        {"SYN-01": card}, {}, {"SYN-01": parse_document(canonical(card))}
    )
    with synthetic_current(
        tmp_path, page("jp", '<div class="detail">Synthetic main rule.</div>')
    ) as current:
        kind = case if case in {"token_definition", "reminder"} else "unknown"
        text = "\n \n" if case == "empty" else "First rule line.\nSecond rule line."
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
        if expected is not None:
            with pytest.raises(SourceError, match=expected):
                materialize(current, "SYN-01", handwritten)
            return
        result = materialize(current, "SYN-01", handwritten)
        face = object_value(next(iter(object_value(result["faces"]).values())))
        section = object_value(array(face["abilities"])[1])
        source = object_value(array(section["sources"])[0])
        assert source["field"] == "section"
        assert source["ordinal"] == 0
        assert source["covers"] == [1, 2]
        assert current.db.rows("face_text_section")[0].values["kind"] == "unknown"


@pytest.mark.parametrize(
    "declaration",
    [
        {"section": 0},
        {"section": True},
        {"section": "main"},
        {"section": 1, "face": 0},
        {"section": 1, "kind": "token_definition"},
    ],
)
def test_rule_section_schema_rejects_invalid_declarations(
    schemas: AuthorSchemas, declaration: dict[str, JsonValue]
) -> None:
    card = rule_card()
    card["rule_sections"] = [declaration]
    with pytest.raises(AuthorInputError) as rejected:
        schemas.storage_tree(
            canonical(
                {"format": 2, "kind": "author_effect_set", "cards": {"SYN-01": card}}
            ),
            "author_effect_set",
        )

    assert "rule_sections" in rejected.value.diagnostic.path


def test_rule_section_schema_retains_author_declaration(schemas: AuthorSchemas) -> None:
    card = rule_card()
    result = object_value(
        schemas.storage_tree(
            canonical(
                {"format": 2, "kind": "author_effect_set", "cards": {"SYN-01": card}}
            ),
            "author_effect_set",
        )
    )
    assert object_value(object_value(result["cards"])["SYN-01"])["rule_sections"] == [
        {"section": 1}
    ]
