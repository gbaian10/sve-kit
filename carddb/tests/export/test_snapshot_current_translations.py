"""Current translations cross the public boundary without legacy review status."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.core.json import array, object_value
from sve_carddb.domains.translations.jp_sources import effect_bindings
from sve_carddb.domains.translations.names.bindings import DisplayBinding
from sve_carddb.export.media import prepare_media
from sve_carddb.export.project import DisplayCheck, project
from sve_carddb.export.reader import read_snapshot, read_text_all
from sve_carddb.export.transport import Batch, Ownership, export_snapshot

from ..support.snapshot_project_fixtures import (
    SETTINGS,
    TEXT,
    decisions,
    populate,
    schema,
)
from .test_snapshot_project import EN_TEXT, dual_project, dual_region, one, projected

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build import CompiledSchema, Database
    from sve_carddb.export.project import Decisions


@pytest.fixture(scope="module")
def current_schema() -> CompiledSchema:
    return schema()


@pytest.fixture
def db(current_schema: CompiledSchema) -> Iterator[Database]:
    with create_database(current_schema) as database:
        with database.transaction():
            populate(database)
        yield database


def jp_source(*, checked: bool = False) -> Decisions:
    return replace(
        decisions(),
        display_bindings=(
            DisplayBinding(
                "use",
                ("face_revision", "revision-en"),
                "zh-Hant",
                "jp_source",
            ),
        ),
        display_checks=(
            DisplayCheck("use", ("face_revision", "revision-en"), TEXT, EN_TEXT),
        )
        if checked
        else (),
    )


@pytest.mark.parametrize("resolved", [False, True])
def test_current_jp_effect_bindings_do_not_require_alignment(
    db: Database, resolved: bool
) -> None:
    dual_region(db)
    with db.transaction():
        db.update("translation_use", {"id": "use"}, {"field": "effect"})
        db.update(
            "region_divergence",
            {"card_id": "card", "region": "en", "field_scope": "name"},
            {"resolved": resolved},
        )
    bindings = effect_bindings(db)
    assert bindings == (
        DisplayBinding("use", ("face_revision", "revision-en"), "zh-Hant", "jp_source"),
    )
    result = dual_project(db, replace(decisions(), display_bindings=bindings))
    field = object_value(
        array(one(result, "face_revision", "revision-en")["translations"])[0]
    )
    assert field["field"] == "effect"
    assert field["basis"] == "jp_source"
    assert field["source"] == {
        "owner": {"kind": "face_revision", "id": "revision"},
        "field": "effect",
        "ordinal": None,
    }


def test_jp_effect_bindings_require_current_donor_and_existing_receiver(
    db: Database,
) -> None:
    dual_region(db)
    with db.transaction():
        db.update("translation_use", {"id": "use"}, {"field": "effect"})
        db.delete("face_current", {"face_id": "face", "region": "jp"})
    assert effect_bindings(db) == ()


@pytest.mark.parametrize("regions", [("en", "jp")])
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
        db,
        regions=regions,
        as_of="2026-10-01",
        settings=SETTINGS,
        decisions=jp_source(),
    )
    translation = one(result, "translation")
    assert translation["origin"] == "machine"
    assert translation["low_confidence"] is True
    assert translation["source_unit_id"] == TEXT
    binding = object_value(
        array(one(result, "face_revision", "revision-en")["translations"])[0]
    )
    assert binding["basis"] == "jp_source"
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
def test_unequal_sections_keep_complete_jp_effect_and_no_jp_sections(
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
    bindings = one(dual_project(db, jp_source()), "face_revision", "revision-en")[
        "translations"
    ]
    assert bool(bindings) is (field in {"name", "effect"})


@pytest.mark.parametrize("checked", [False, True])
def test_known_name_divergence_keeps_jp_translation_in_both_modes(
    db: Database, checked: bool
) -> None:
    dual_region(db)
    with db.transaction():
        db.update(
            "region_divergence",
            {"card_id": "card", "region": "en", "field_scope": "name"},
            {"resolved": False},
        )
    result = dual_project(db, jp_source(checked=checked))
    assert (
        object_value(
            array(one(result, "face_revision", "revision-en")["translations"])[0]
        )["basis"]
        == "jp_source"
    )


def test_completed_display_check_keeps_the_same_jp_source_basis(db: Database) -> None:
    dual_region(db)
    chosen = replace(jp_source(), display_checks=jp_source(checked=True).display_checks)
    for result in (dual_project(db, chosen), dual_project(db, jp_source(checked=True))):
        assert (
            object_value(
                array(one(result, "face_revision", "revision-en")["translations"])[0]
            )["basis"]
            == "jp_source"
        )


@pytest.mark.parametrize("side", ["source_unit_id", "counterpart_unit_id"])
def test_display_checks_are_bound_to_each_current_source(
    db: Database, side: str
) -> None:
    dual_region(db)
    original = jp_source(checked=True).display_checks[0]
    check = (
        replace(original, source_unit_id="t:stale")
        if side == "source_unit_id"
        else replace(original, counterpart_unit_id="t:stale")
    )
    with pytest.raises(
        ValueError, match=r"^Translation display check source mismatch$"
    ):
        dual_project(db, replace(jp_source(checked=True), display_checks=(check,)))


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
    binding = replace(jp_source().display_bindings[0], target_lang=lang)
    with pytest.raises(
        ValueError, match=r"^JP source translation region/language mismatch$"
    ):
        dual_project(db, replace(jp_source(), display_bindings=(binding,)))


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


def test_jp_translation_rejects_a_snapshot_missing_the_exact_donor_owner(
    db: Database,
) -> None:
    dual_region(db)
    with db.transaction():
        db.delete("ruling_evidence", {"id": "evidence"})
        db.delete("route_override", {"route_key": "TEST-001%E2%93%88a"})
    with pytest.raises(ValueError, match="public-annotation/owner"):
        project(
            db,
            regions=("en",),
            as_of="2026-10-01",
            settings=SETTINGS,
            decisions=jp_source(),
        )


@pytest.mark.parametrize("basis", ["shared_jp", "shared_jp_unchecked"])
def test_retired_jp_display_basis_is_rejected(db: Database, basis: str) -> None:
    dual_region(db)
    chosen = replace(
        decisions(),
        display_bindings=(
            DisplayBinding("use", ("face_revision", "revision-en"), "zh-Hant", basis),
        ),
    )
    with pytest.raises(ValueError, match="Unknown translation display basis"):
        dual_project(db, chosen)
