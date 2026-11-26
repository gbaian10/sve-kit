"""Build a small current JP source database for direct author tests."""

from contextlib import contextmanager
from typing import TYPE_CHECKING

from sve_carddb.build import Json, create_database
from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.build.t1 import MINIMUM_CAPABILITIES
from sve_carddb.domains.catalog.rules_names import populate_rules_names
from sve_carddb.domains.effects.author_current import CurrentAuthorSources
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner

from ..domains.text_observations.test_effect_presence import card_from_raw, page
from .build_db_fixtures import rows
from .compiled_schemas import compile_build

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
    capabilities: tuple[str, ...] = (),
) -> Generator[CurrentAuthorSources]:
    card = card_from_raw(root, page("jp") if raw is None else raw)
    source = card.source
    assert source.archive is not None
    schema = compile_build(
        (*MINIMUM_CAPABILITIES, "translation_evidence", *capabilities)
    )
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
                    "cost": 1,
                    "attack": 1,
                    "defense": 1,
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
            populate_rules_names(db)
        yield CurrentAuthorSources(db)
