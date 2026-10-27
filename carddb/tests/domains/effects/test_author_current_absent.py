"""Known absence requires the same frozen face, including for blank tokens."""

from contextlib import contextmanager
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json, create_database
from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.build.t1 import MINIMUM_CAPABILITIES
from sve_carddb.core.json import digest
from sve_carddb.core.provenance import BuildContext
from sve_carddb.domains.catalog.rules_names import populate_rules_names
from sve_carddb.domains.effects.author_current import CurrentAuthorSources
from sve_carddb.domains.effects.author_sources import SourceError
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations import Vocabulary
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.translations.inputs import Snapshot
from sve_carddb.domains.translations.recognition.rules import Rules
from sve_carddb.domains.translations.sources import Sources

from ...support.build_db_fixtures import rows
from ...support.compiled_schemas import compile_build
from ..text_observations.test_effect_presence import card_from_raw, page

if TYPE_CHECKING:
    from collections.abc import Generator
    from pathlib import Path

CARD = "c:" + "1" * 32
FACE = "f:" + "2" * 32


@contextmanager
def synthetic_current(
    root: Path,
    raw: bytes | None = None,
    *,
    kind: str = "follower",
    special: str | None = None,
    null_stats: bool = False,
) -> Generator[CurrentAuthorSources]:
    card = card_from_raw(root, page("jp") if raw is None else raw)
    source = card.source
    assert source.archive is not None
    schema = compile_build((*MINIMUM_CAPABILITIES, "translation_evidence"))
    with create_database(schema) as db:
        with db.transaction():
            insert_raw_sources(db, (source,))
            for lang in ("ja", "en", "zh-Hant"):
                db.insert(
                    "language",
                    {"code": lang, "display_name": lang, "fallback_order": Json([])},
                )
            texts = TextInterner(db)
            name = texts.intern(LocalizedText(lang="ja", text=card.faces[0].name))
            effect = texts.intern(
                LocalizedText(lang="ja", text=card.faces[0].effect or "")
            )
            db.insert(
                "product_family", rows()["product_family"] | {"name_unit_id": name}
            )
            for group, code in (("type", kind), ("class", "neutral")):
                db.insert(
                    "vocabulary",
                    {
                        "kind": group,
                        "code": code,
                        "label_unit_id": name,
                        "active": True,
                    },
                )
            db.insert("card", rows()["card"] | {"id": CARD})
            db.insert("face", rows()["face"] | {"id": FACE, "card_id": CARD})
            db.insert(
                "printing",
                rows()["printing"]
                | {"card_id": CARD, "card_no": "SYN-01", "source_id": source.id},
            )
            db.insert(
                "face_revision",
                rows()["face_revision"]
                | {
                    "face_id": FACE,
                    "name_unit_id": name,
                    "effect_unit_id": effect,
                    "type_code": kind,
                    "class_code": "neutral",
                    "cost": None if null_stats or special == "evolve" else 1,
                    "attack": None if null_stats else 1,
                    "defense": None if null_stats else 1,
                    "source_id": source.id,
                },
            )
            db.insert(
                "face_current",
                {
                    "face_id": FACE,
                    "region": "jp",
                    "revision_id": "revision",
                    "basis": "latest_observed_no_errata",
                },
            )
            if special is not None:
                db.insert(
                    "vocabulary",
                    {
                        "kind": "special_kind",
                        "code": special,
                        "label_unit_id": name,
                        "active": True,
                    },
                )
                db.insert(
                    "face_special_kind",
                    {"revision_id": "revision", "special_kind_code": special},
                )
            populate_rules_names(db)
        yield CurrentAuthorSources(
            db,
            Sources(
                {"test-store": root / "archive"},
                root,
                BuildContext.from_inputs("0" * 40, {}),
            ),
            Snapshot(()),
            Vocabulary(bindings=()),
            Rules(format=2, kind="template_parameter_rules", rules=()),
            source.archive.batch_id,
        )


@pytest.mark.parametrize("special", [None, "token", "evolve"])
def test_proven_absence_is_known_and_has_no_keyword_obligations(
    tmp_path: Path, special: str | None
) -> None:
    with synthetic_current(tmp_path, special=special) as current:
        card = current.card_payload(CARD)
        text = card.payload.faces[0].texts[1]
        assert text.state == "absent"
        assert text.source_hash == digest(b"").removeprefix("sha256:")
        assert current.keyword_obligations(CARD) == ()
        if special == "evolve":
            cost = card.payload.faces[0].attributes.cost
            assert cost.state == "known"
            assert cost.value is None


def test_unproven_empty_source_remains_unknown(tmp_path: Path) -> None:
    raw = page("jp").replace(b"</body></html>", b"")
    with synthetic_current(tmp_path, raw) as current:
        text = current.card_payload(CARD, inspect=True).payload.faces[0].texts[1]
        assert text.state == "unknown"
        assert text.source_hash is None
        with pytest.raises(SourceError, match="source_unknown"):
            current.keyword_obligations(CARD)


def test_absence_on_another_face_cannot_authorize_unknown_text(tmp_path: Path) -> None:
    raw = page("jp", double=True).replace(
        b'<div class="illustrator">',
        b'<div class="mystery"></div><div class="illustrator">',
        1,
    )
    with synthetic_current(tmp_path, raw) as current:
        text = current.card_payload(CARD, inspect=True).payload.faces[0].texts[1]
        assert text.state == "unknown"
        with pytest.raises(SourceError, match="source_unknown"):
            current.keyword_obligations(CARD)


def test_other_version_cannot_authorize_empty_database_text(tmp_path: Path) -> None:
    raw = page("jp", '<div class="detail">Different version.</div>')
    with synthetic_current(tmp_path, raw) as current:
        empty = current.db.select("text_unit", ("id",), where={"text": ""})
        assert not empty
        with current.db.transaction():
            unit = TextInterner(current.db).intern(LocalizedText(lang="ja", text=""))
            current.db.update(
                "face_revision", {"id": "revision"}, {"effect_unit_id": unit}
            )
        with pytest.raises(SourceError, match="source_reconstruction_failed"):
            current.card_payload(CARD, inspect=True)


@pytest.mark.parametrize("kind", ["leader", "follower"])
@pytest.mark.parametrize("dashes", [True, False])
def test_known_null_leader_stats_require_printed_dashes(
    tmp_path: Path, kind: str, dashes: bool
) -> None:
    raw = page("jp")
    if dashes:
        raw = raw.replace(b"</span>1</span>", b"</span>-</span>")
    with synthetic_current(tmp_path, raw, kind=kind, null_stats=True) as current:
        attributes = current.card_payload(CARD).payload.faces[0].attributes
        stats = (attributes.cost, attributes.attack, attributes.defense)
        assert all(s.value is None for s in stats)
        assert all(
            s.state == ("known" if kind == "leader" and dashes else "unknown")
            for s in stats
        )


def test_nonempty_effect_keeps_exact_hash_and_token_gate(tmp_path: Path) -> None:
    raw = page("jp", '<div class="detail">Synthetic rule.</div>')
    with synthetic_current(tmp_path, raw, special="token") as current:
        text = current.card_payload(CARD, inspect=True).payload.faces[0].texts[1]
        assert text.state == "present"
        assert text.source_hash == digest(b"Synthetic rule.").removeprefix("sha256:")
        with pytest.raises(SourceError):
            current.keyword_obligations(CARD)


def test_unknown_presence_does_not_hide_a_nonempty_effect(tmp_path: Path) -> None:
    raw = page("jp", '<div class="detail">Synthetic rule.</div>').replace(
        b"</body></html>", b""
    )
    with synthetic_current(tmp_path, raw) as current:
        text = current.card_payload(CARD, inspect=True).payload.faces[0].texts[1]
        assert text.state == "present"
        assert text.source_hash == digest(b"Synthetic rule.").removeprefix("sha256:")


def test_blank_main_text_does_not_hide_a_token_section(tmp_path: Path) -> None:
    raw = page("jp", '<div class="detail">-----<br>Synthetic rule.</div>')
    with synthetic_current(tmp_path, raw, special="token") as current:
        with current.db.transaction():
            unit = TextInterner(current.db).intern(
                LocalizedText(lang="ja", text="Synthetic rule.")
            )
            current.db.insert(
                "face_text_section",
                {
                    "revision_id": "revision",
                    "ordinal": 0,
                    "text_unit_id": unit,
                    "kind": "rule",
                },
            )
        card = current.card_payload(CARD, inspect=True)
        assert card.payload.faces[0].texts[1].state == "unknown"
        assert card.payload.faces[0].texts[2].state == "present"
        assert [field.source.descriptor.field for field in card.fields] == ["section"]
        with pytest.raises(SourceError):
            current.keyword_obligations(CARD)
