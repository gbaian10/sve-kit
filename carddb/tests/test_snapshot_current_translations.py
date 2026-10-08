"""Current translations cross the public boundary without legacy review status."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.core.json import array, object_value
from sve_carddb.snapshot.export import Batch, Ownership, export_snapshot
from sve_carddb.snapshot.media import prepare_media
from sve_carddb.snapshot.project import DisplayCheck, project
from sve_carddb.snapshot.reader import read_snapshot, read_text_all
from sve_carddb.translations.bindings import DisplayBinding

from .snapshot_project_fixtures import SETTINGS, TEXT, decisions, populate, schema
from .test_snapshot_project import EN_TEXT, dual_project, dual_region, one, projected

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build_db import CompiledSchema, Database
    from sve_carddb.snapshot.project import Decisions


@pytest.fixture(scope="module")
def current_schema() -> CompiledSchema:
    return schema()


@pytest.fixture
def db(current_schema: CompiledSchema) -> Iterator[Database]:
    with create_database(current_schema) as database:
        with database.transaction():
            populate(database)
        yield database


def shared(*, checked: bool = False) -> Decisions:
    return replace(
        decisions(),
        display_bindings=(
            DisplayBinding(
                "use",
                ("face_revision", "revision-en"),
                "zh-Hant",
                "shared_jp" if checked else "shared_jp_unchecked",
            ),
        ),
        display_checks=(
            DisplayCheck("use", ("face_revision", "revision-en"), TEXT, EN_TEXT),
        )
        if checked
        else (),
    )


@pytest.mark.parametrize("regions", [("en",), ("en", "jp")])
def test_unchecked_machine_translation_keeps_quality_and_source_closure(
    db: Database, regions: tuple[str, ...]
) -> None:
    dual_region(db)
    if regions == ("en",):
        with db.transaction():
            db.delete("ruling_evidence", {"id": "evidence"})
            db.delete("route_override", {"route_key": "TEST-001%E2%93%88a"})
    with db.transaction():
        db.update(
            "translation",
            {"id": "translation"},
            {
                "origin": "machine",
                "low_confidence": True,
            },
        )
    result = project(
        db, regions=regions, as_of="2026-10-01", settings=SETTINGS, decisions=shared()
    )
    translation = one(result, "translation")
    assert translation["origin"] == "machine"
    assert translation["low_confidence"] is True
    assert translation["source_unit_id"] == TEXT
    binding = object_value(
        array(one(result, "face_revision", "revision-en")["translations"])[0]
    )
    assert binding["basis"] == "shared_jp_unchecked"
    assert any(row["id"] == TEXT for row in result.tables["text_unit"])
    assert "Synthetic translation" in {
        row["text"] for row in result.tables["text_unit"]
    }
    result = replace(
        result,
        tables=result.tables
        | {
            "printing_image": [
                row | {"availability": "unfetched", "publication_state": "pending"}
                for row in result.tables["printing_image"]
            ],
            "image_variant": [],
        },
    )
    result = prepare_media(result, None, revision=1).projection
    exported = export_snapshot(
        result,
        Ownership.from_database(db, result),
        Batch("preview-20261005T010203Z-0001", "2026-10-05T01:02:03Z", regions),
    )
    payloads = {key: blob.raw for key, blob in exported.payloads.items()}
    assert read_snapshot(exported.manifest, payloads) == result.tables
    attachments = {
        key: value
        for key, value in payloads.items()
        if key == "programs" or key.startswith("images/")
    }
    assert (
        read_text_all(exported.manifest, exported.text_all.raw, attachments)
        == result.tables
    )


@pytest.mark.parametrize("field", ["name", "effect", "section"])
def test_unequal_section_counts_only_block_unchecked_effect_and_sections(
    db: Database, field: str
) -> None:
    dual_region(db)
    with db.transaction():
        db.update(
            "translation_use",
            {"id": "use"},
            {
                "field": field,
                "ordinal": 0 if field == "section" else None,
            },
        )
        if field == "section":
            db.insert(
                "face_text_section",
                dict(db.rows("face_text_section")[0].values)
                | {"revision_id": "revision-en"},
            )
            db.insert(
                "face_text_section",
                dict(db.rows("face_text_section")[0].values)
                | {"revision_id": "revision-en", "ordinal": 1},
            )
    bindings = one(dual_project(db, shared()), "face_revision", "revision-en")[
        "translations"
    ]
    assert bool(bindings) is (field == "name")


@pytest.mark.parametrize("basis", ["shared_jp", "shared_jp_unchecked"])
def test_known_name_divergence_blocks_both_sharing_modes(
    db: Database, basis: str
) -> None:
    dual_region(db)
    with db.transaction():
        db.update(
            "region_divergence",
            {"card_id": "card", "region": "en", "field_scope": "name"},
            {"resolved": False},
        )
    result = dual_project(db, shared(checked=basis == "shared_jp"))
    assert one(result, "face_revision", "revision-en")["translations"] == []


def test_completed_display_check_requires_checked_basis(db: Database) -> None:
    dual_region(db)
    chosen = replace(shared(), display_checks=shared(checked=True).display_checks)
    with pytest.raises(
        ValueError, match=r"^Checked JP translation must use shared_jp$"
    ):
        dual_project(db, chosen)
    checked = dual_project(db, shared(checked=True))
    assert (
        object_value(
            array(one(checked, "face_revision", "revision-en")["translations"])[0]
        )["basis"]
        == "shared_jp"
    )


@pytest.mark.parametrize("side", ["source_unit_id", "counterpart_unit_id"])
def test_display_checks_are_bound_to_each_current_source(
    db: Database, side: str
) -> None:
    dual_region(db)
    original = shared(checked=True).display_checks[0]
    check = (
        replace(original, source_unit_id="t:stale")
        if side == "source_unit_id"
        else replace(original, counterpart_unit_id="t:stale")
    )
    with pytest.raises(
        ValueError, match=r"^Translation display check source mismatch$"
    ):
        dual_project(db, replace(shared(checked=True), display_checks=(check,)))


def test_stale_source_hash_cannot_be_recast_as_low_confidence(db: Database) -> None:
    with db.transaction():
        db.update(
            "translation",
            {"id": "translation"},
            {
                "source_hash": "sha256:" + "a" * 64,
                "low_confidence": True,
            },
        )
    with pytest.raises(
        ValueError, match=r"^Translation source hash differs from its current context$"
    ):
        projected(db)


@pytest.mark.parametrize("authority", ["digital_official", "unofficial"])
def test_official_counterpart_requires_physical_official_authority(
    db: Database, authority: str
) -> None:
    dual_region(db)
    with db.transaction():
        db.delete(
            "translation_selection", {"context_id": "context", "target_lang": "zh-Hant"}
        )
        db.update(
            "translation",
            {"id": "translation"},
            {
                "origin": "official",
                "text": "Synthetic official",
                "authority": authority,
                "target_lang": "en",
            },
        )
    chosen = replace(
        decisions(),
        display_checks=(
            DisplayCheck("use", ("face_revision", "revision-en"), TEXT, EN_TEXT, True),
        ),
        display_bindings=(
            DisplayBinding(
                "use",
                ("face_revision", "revision"),
                "en",
                "official_counterpart",
                "translation",
            ),
        ),
    )
    with pytest.raises(
        ValueError, match=r"^Official counterpart origin/authority mismatch$"
    ):
        projected(db, chosen)


@pytest.mark.parametrize("lang", ["en", "ja"])
def test_unchecked_shared_translation_does_not_borrow_other_languages(
    db: Database, lang: str
) -> None:
    dual_region(db)
    binding = replace(shared().display_bindings[0], target_lang=lang)
    with pytest.raises(
        ValueError, match=r"^Shared JP translation region/language mismatch$"
    ):
        dual_project(db, replace(shared(), display_bindings=(binding,)))


def test_counterpart_does_not_borrow_an_unchecked_text(db: Database) -> None:
    dual_region(db)
    with db.transaction():
        db.delete(
            "translation_selection", {"context_id": "context", "target_lang": "zh-Hant"}
        )
        db.update(
            "translation",
            {"id": "translation"},
            {
                "origin": "official",
                "authority": "sve_official",
                "target_lang": "en",
            },
        )
    chosen = replace(
        decisions(),
        display_checks=(
            DisplayCheck("use", ("face_revision", "revision-en"), TEXT, EN_TEXT, True),
        ),
        display_bindings=(
            DisplayBinding(
                "use",
                ("face_revision", "revision"),
                "en",
                "official_counterpart",
                "translation",
            ),
        ),
    )
    with pytest.raises(
        ValueError, match=r"^Official counterpart text differs from its checked source$"
    ):
        projected(db, chosen)
