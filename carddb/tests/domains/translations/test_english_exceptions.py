"""Real sealed English pages authorize display uses, never Japanese rule offsets."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json, create_database
from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.build.t1 import compile_build
from sve_carddb.contracts.four_layer import (
    FaceRevisionOwner,
    OwnerField,
    PrintingFaceOwner,
)
from sve_carddb.core.json import canonical
from sve_carddb.core.provenance import BuildContext
from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.translations.english_exceptions import apply
from sve_carddb.domains.translations.four_layer_authored import from_files
from sve_carddb.domains.translations.four_layer_sources import descriptor
from sve_carddb.domains.translations.four_layer_storage import read_annotation
from sve_carddb.domains.translations.sources import Sources
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import seal_batch
from sve_carddb.parse.pages.official_en import card_url

from ...ingest.test_source_archive import _put, _resource, _store
from ...support.build_db_fixtures import rows
from ...workflows.test_snapshot_offline_english import records
from ..text_observations.test_effect_presence import page

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.build import Database
    from sve_carddb.contracts.source_binding import SourceDescriptor
    from sve_carddb.domains.translations.four_layer_authored import Inputs


@pytest.fixture
def english(  # ruff: ignore[too-many-locals] -- seal two independent real EN pages and join their DB owners in one fixture
    tmp_path: Path,
) -> Iterator[tuple[Database, Sources, tuple[SourceDescriptor, ...]]]:
    store = _store(tmp_path / "archive")
    for number, text in (
        ("SYN-01", "Invented effect."),
        ("SYN-02", "Other invented effect."),
    ):
        raw = page("en", f'<div class="detail">{text}</div>').replace(
            b"SYN-01", number.encode()
        )
        resource = replace(
            _resource(card_url(number), f"raw/{number}.html", raw, Kind.CARD),
            region=SourceRegion.EN,
        )
        _put(store, resource, raw)
    sealed = seal_batch(store)
    frozen = FrozenSources(store.root, store.store_id, sealed.batch_id)
    sources = Sources(
        {store.store_id: store.root}, tmp_path, BuildContext.from_inputs("a" * 40, {})
    )
    schema = compile_build(
        (
            "t0",
            "en",
            "translation_evidence",
            "translation_names",
            "translation_templates",
        )
    )
    values = rows()
    with create_database(schema) as db:
        with db.transaction():
            db.insert("source_record", values["source_record"])
            for lang in ("en", "zh-Hant"):
                db.insert(
                    "language",
                    {"code": lang, "display_name": lang, "fallback_order": Json([])},
                )
            texts = TextInterner(db)
            label = texts.intern(LocalizedText(lang="zh-Hant", text="合成類型"))
            db.insert(
                "product_family", values["product_family"] | {"name_unit_id": label}
            )
            db.insert("vocabulary", values["vocabulary"] | {"label_unit_id": label})
            db.insert("card", values["card"])
            db.insert("face", values["face"])
            db.insert(
                "region_mapping_review",
                {
                    "card_id": "card",
                    "target_region": "jp",
                    "state": "confirmed_none",
                    "as_of": "2026-10-01",
                    "coverage_scope": "synthetic complete input",
                    "source_id": "source",
                },
            )
            descriptors = []
            for index, current in enumerate(frozen.inventory.current):
                source, _, _ = frozen.read(
                    current.source_version_id, parser_version="translation-en-v1"
                )
                insert_raw_sources(db, (source,))
                _, document, _ = sources.projection(
                    sealed.batch_id, source.id, "translation-en-v1"
                )
                assert isinstance(document, dict)
                faces = document["faces"]
                assert isinstance(faces, list)
                assert isinstance(faces[0], dict)
                raw_text = faces[0]["text"]
                assert isinstance(raw_text, str)
                unit = texts.intern(LocalizedText(lang="en", text=raw_text))
                number = "SYN-01" if index == 0 else "SYN-02"
                printing = f"printing:{index}"
                db.insert(
                    "printing",
                    values["printing"]
                    | {
                        "id": printing,
                        "region": "en",
                        "card_no": number,
                        "source_id": source.id,
                    },
                )
                db.insert(
                    "printing_face",
                    values["printing_face"]
                    | {
                        "printing_id": printing,
                        "printed_effect_unit_id": unit,
                        "source_id": source.id,
                    },
                )
                descriptors.append(
                    descriptor(
                        db,
                        sources,
                        OwnerField(
                            owner=PrintingFaceOwner(
                                kind="printing_face",
                                printing_id=printing,
                                face_id="face",
                            ),
                            field="effect",
                            ordinal=None,
                        ),
                        sealed.batch_id,
                        lang="en",
                    )
                )
        yield db, sources, tuple(descriptors)


def inputs(values: list[JsonValue]) -> Inputs:
    raw = canonical({"format": 3, "kind": "translation_shard", "records": values})
    return from_files((("translations/overrides/english/000.yaml", raw, raw),))


@pytest.mark.parametrize(
    "mode",
    [
        "translated",
        "missing_label",
        "low_label",
        "bad_vocabulary",
        "pending",
        "latest_pending",
        "has_jp",
        "wrong_owner",
    ],
)
def test_required_references_quality_ranges_and_identity_gates(  # ruff: ignore[too-many-statements] -- each public selection gate has an independent fixture mutation
    english: tuple[Database, Sources, tuple[SourceDescriptor, ...]],
    mode: str,
) -> None:
    db, sources, descriptors = english
    source = descriptors[0]
    values = records(source, "card", "face")
    target = values[0]
    assert isinstance(target, dict)
    assert isinstance(target["data"], dict)
    target["origin"] = "project"
    target["low_confidence"] = False
    target["data"]["references"] = {
        "kind": {"kind": "vocabulary", "key": ["type", "follower"]}
    }
    target["data"]["nodes"] = [
        {"kind": "Literal", "text": "😀é"},
        {"kind": "LeafRef", "slot": "kind"},
        {"kind": "Literal", "text": "／"},
        {"kind": "LeafRef", "slot": "kind"},
    ]
    with db.transaction():
        if mode == "missing_label":
            db.insert(
                "language",
                {"code": "ja", "display_name": "ja", "fallback_order": Json([])},
            )
            label = TextInterner(db).intern(
                LocalizedText(lang="ja", text="Synthetic label")
            )
            db.update(
                "vocabulary",
                {"kind": "type", "code": "follower"},
                {"label_unit_id": label},
            )
        elif mode == "low_label":
            db.update(
                "vocabulary",
                {"kind": "type", "code": "follower"},
                {"origin": "machine", "low_confidence": True},
            )
        elif mode == "bad_vocabulary":
            db.update(
                "vocabulary", {"kind": "type", "code": "follower"}, {"active": False}
            )
        elif mode == "pending":
            db.update(
                "region_mapping_review",
                {"card_id": "card", "target_region": "jp", "as_of": "2026-10-01"},
                {"state": "pending"},
            )
        elif mode == "latest_pending":
            db.insert(
                "region_mapping_review",
                {
                    "card_id": "card",
                    "target_region": "jp",
                    "as_of": "2026-10-02",
                    "state": "pending",
                    "coverage_scope": "new synthetic input",
                    "source_id": "source",
                },
            )
        elif mode == "has_jp":
            db.insert("printing", rows()["printing"] | {"id": "printing:jp"})
        elif mode == "wrong_owner":
            assert isinstance(values[1], dict)
            assert isinstance(values[1]["data"], dict)
            data = values[1]["data"]["source"]
            assert isinstance(data, dict)
            data["owner"] = {
                "kind": "printing_face",
                "printing_id": "printing:absent",
                "face_id": "face",
            }
    authored = inputs(values)
    if mode == "bad_vocabulary":
        with (
            db.transaction(),
            pytest.raises(ValueError, match="unavailable vocabulary"),
        ):
            apply(
                db,
                authored,
                sources,
                {record.record_key: "source" for record in authored.records},
            )
        return
    with db.transaction():
        report = apply(
            db,
            authored,
            sources,
            {record.record_key: "source" for record in authored.records},
        )
    assert report is not None
    translated = mode in {"translated", "low_label"}
    assert report["translated"] == int(translated)
    if not translated:
        assert db.rows("translation_selection") == ()
        assert db.rows("translation_use_annotation") == ()
        return
    translation = db.rows("translation")[0].values
    assert translation["text"] == "😀é合成類型／合成類型"
    assert translation["low_confidence"] is (mode == "low_label")
    assert translation["origin"] == ("machine" if mode == "low_label" else "project")
    annotation = read_annotation(
        db, str(db.rows("translation_annotation")[0].values["annotation_set_id"])
    )
    assert tuple(
        (a.ranges[0].start, a.ranges[0].end) for a in annotation.occurrences
    ) == ((3, 7), (8, 12))
    assert all(a.bold is True for a in annotation.occurrences)
    with pytest.raises(ValueError, match="text identity mismatch"):
        annotation.verify(annotation.text_unit_id, "en", "Invented effect.")
    assert db.rows("text_template_binding") == ()
    assert db.rows("translation_binding") == ()
    assert db.rows("translation_use_annotation") == ()


def test_each_printing_uses_its_own_complete_frozen_field(
    english: tuple[Database, Sources, tuple[SourceDescriptor, ...]],
) -> None:
    db, sources, descriptors = english
    first = records(descriptors[0], "card", "face")
    second = records(descriptors[1], "card", "face")
    assert descriptors[0].source_hash != descriptors[1].source_hash
    assert isinstance(second[0], dict)
    assert isinstance(second[0]["data"], dict)
    assert isinstance(second[1], dict)
    assert isinstance(second[1]["data"], dict)
    second[0]["data"]["id"] = second[1]["data"]["target_id"] = "synthetic.second"
    second[0]["data"]["nodes"] = [{"kind": "Literal", "text": "其他合成規則。"}]
    authored = inputs(first + second)
    with db.transaction():
        report = apply(
            db,
            authored,
            sources,
            {record.record_key: "source" for record in authored.records},
        )
    assert report is not None
    assert report["translated"] == 2
    assert {row.values["text"] for row in db.rows("translation")} == {
        "合成規則😀。",
        "其他合成規則。",
    }
    foreign = first[1]
    assert isinstance(foreign, dict)
    assert isinstance(foreign["data"], dict)
    foreign["data"]["source"] = descriptors[1].model_dump(mode="json")
    with pytest.raises(ValueError, match="same exact source"):
        inputs(first)


@pytest.mark.parametrize(
    "change",
    [
        "missing_target",
        "unknown_slot",
        "unknown_node",
        "official_origin",
        "duplicate_use",
    ],
)
def test_invalid_authored_exceptions_do_not_enter_the_build(
    english: tuple[Database, Sources, tuple[SourceDescriptor, ...]],
    change: str,
) -> None:
    values = records(english[2][0], "card", "face")
    target, use = values
    assert isinstance(target, dict)
    assert isinstance(target["data"], dict)
    assert isinstance(use, dict)
    assert isinstance(use["data"], dict)
    if change == "missing_target":
        use["data"]["target_id"] = "absent"
    elif change == "unknown_slot":
        target["data"]["nodes"] = [{"kind": "LeafRef", "slot": "absent"}]
    elif change == "unknown_node":
        target["data"]["nodes"] = [{"kind": "Form", "form_id": "absent", "args": {}}]
    elif change == "official_origin":
        target["origin"] = "official"
    else:
        values.append(use)
    expected = (
        "shared target"
        if change == "missing_target"
        else "Duplicate"
        if change == "duplicate_use"
        else "Invalid four-layer authored shard"
    )
    with pytest.raises(ValueError, match=expected):
        inputs(values)


@pytest.mark.parametrize("current", [True, False])
def test_identical_source_and_semantics_share_one_target_across_exact_owners(
    english: tuple[Database, Sources, tuple[SourceDescriptor, ...]],
    current: bool,
) -> None:
    db, sources, descriptors = english
    printed = descriptors[0]
    revision = printed.model_copy(
        update={
            "owner": FaceRevisionOwner(kind="face_revision", revision_id="revision:en")
        }
    )
    with db.transaction():
        db.insert(
            "face_revision",
            rows()["face_revision"]
            | {
                "id": "revision:en",
                "region": "en",
                "name_unit_id": printed.source_unit_id,
                "effect_unit_id": printed.source_unit_id,
                "source_id": printed.source_ref.source_version_id,
            },
        )
        if current:
            db.insert(
                "face_current",
                rows()["face_current"] | {"region": "en", "revision_id": "revision:en"},
            )
    first = records(printed, "card", "face")
    authored = inputs([*first, records(revision, "card", "face")[1]])
    with db.transaction():
        report = apply(
            db,
            authored,
            sources,
            {record.record_key: "source" for record in authored.records},
        )
    assert report is not None
    assert report["translated"] == 1 + int(current)
    assert len(db.rows("translation_use")) == 1 + int(current)
    assert {row.values["text"] for row in db.rows("translation")} == {"合成規則😀。"}
    if not current:
        assert report["fallback_reasons"] == {"english_source_not_current": 1}
