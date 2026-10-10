"""Only selected adopted labels supply presentation, category and quality dependencies."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    GlossaryReference,
    VocabularyReference,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_labels import labels

from .test_four_layer_storage import compiled as compiled  # ruff: ignore[useless-import-alias] -- shared actual SQLite fixture
from .test_four_layer_storage import stored as stored  # ruff: ignore[useless-import-alias] -- shared adopted parent rows

if TYPE_CHECKING:
    from sve_carddb.build import Database, Value
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding


@pytest.mark.parametrize(
    ("category", "emphasis", "expected"),
    [
        ("card_name", None, True),
        ("keyword", None, True),
        ("ability", None, True),
        ("trait", None, True),
        ("rule_term", None, None),
        ("rule_term", False, False),
        ("rule_term", True, True),
    ],
)
def test_glossary_category_and_rule_emphasis_set_bold_without_changing_quality(
    stored: tuple[Database, Frame, SourceBinding],
    category: str,
    emphasis: bool | None,
    expected: bool | None,
) -> None:
    db, _, _ = stored
    with db.transaction():
        db.insert(
            "glossary_term",
            {
                "id": "term:synthetic.label",
                "category": category,
                "concept_key": "synthetic.label",
                "source_ja": "自編詞",
                "emphasis": emphasis,
                "authored_source_id": "source",
                "record_key": "synthetic-term",
                "origin": "project",
                "low_confidence": True,
            },
        )
        db.insert(
            "glossary_translation",
            {
                "term_id": "term:synthetic.label",
                "lang": "zh-Hant",
                "text": "合成譯詞😀",
                "source_id": None,
                "authored_source_id": "source",
                "record_key": "synthetic-choice",
                "origin": "machine",
                "low_confidence": False,
            },
        )
    reference = (
        CardNameReference(kind="card_name", term_id="term:synthetic.label")
        if category == "card_name"
        else GlossaryReference(kind="glossary", key="term:synthetic.label")
    )
    selected = labels(db, "zh-Hant")[
        canonical(reference.model_dump(mode="json")), "zh-Hant"
    ]
    assert selected.text == "合成譯詞😀"
    assert selected.bold is expected
    assert selected.origin == "machine"
    assert selected.low_confidence is True


def test_vocabulary_label_requires_a_selection_or_the_requested_source_language(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, _ = stored
    reference = VocabularyReference(kind="vocabulary", key=("type", "follower"))
    key = canonical(reference.model_dump(mode="json")), "zh-Hant"
    assert key not in labels(db, "zh-Hant")
    original = labels(db, "ja")[key[0], "ja"]
    assert original.bold is True
    with db.transaction():
        db.insert(
            "translation_context",
            {
                "id": "label-context",
                "source_unit_id": "text",
                "semantic_variant": "label-context",
            },
        )
        use: dict[str, Value] = dict.fromkeys(db.columns("translation_use"))
        use.update(
            id="label-use",
            context_id="label-context",
            field="label",
            vocabulary_kind="type",
            vocabulary_code="follower",
        )
        db.insert("translation_use", use)
        unit = db.select("text_unit", db.columns("text_unit"), where={"id": "text"})[
            0
        ].values
        db.insert(
            "translation",
            {
                "id": "label-translation",
                "context_id": "label-context",
                "target_lang": "zh-Hant",
                "revision": 0,
                "text": "自編卡種",
                "tokens": None,
                "origin": "project",
                "authority": "unofficial",
                "low_confidence": True,
                "source_hash": unit["content_hash"],
                "source_id": None,
            },
        )
        db.insert(
            "translation_selection",
            {
                "context_id": "label-context",
                "target_lang": "zh-Hant",
                "translation_id": "label-translation",
            },
        )
    selected = labels(db, "zh-Hant")[key]
    assert selected.text == "自編卡種"
    assert selected.bold is True
    assert selected.low_confidence is True
    with db.transaction():
        db.update("vocabulary", {"kind": "type", "code": "follower"}, {"active": False})
    assert key not in labels(db, "zh-Hant")
