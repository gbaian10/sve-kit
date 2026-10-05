"""Flavor translations are a hash-keyed table: no templates, original text as fallback."""

from typing import TYPE_CHECKING, Concatenate

import pytest
from pydantic import JsonValue

from sve_carddb.build_db import Json, create_database
from sve_carddb.build_db.current import compile_current_build
from sve_carddb.snapshot import offline
from sve_carddb.snapshot.offline import build
from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.text_observations import populate_text_preview
from sve_carddb.translations.flavor import Entry, apply, load

from .build_db_fixtures import HASH, seed
from .database_fixtures import DatabaseTemplate
from .test_snapshot_offline import prepared as _prepared

prepared = _prepared

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import InputRecord
    from sve_carddb.card_extras import CardPage
    from sve_carddb.snapshot.offline import Inputs

    from .text_observation_fixtures import Case

H1 = "sha256:" + "1" * 64
H2 = "sha256:" + "2" * 64
FLAVOR_UNIT = "t:ja:" + digest("合成風味".encode())[7:23]


def entry(
    source_hash: str = H1, text: str = "風味", **extra: JsonValue
) -> dict[str, JsonValue]:
    base: dict[str, JsonValue] = {
        "source_hash": source_hash,
        "lang": "zh-Hant",
        "text": text,
        "origin": "machine",
        "low_confidence": False,
    }
    return base | extra


def shard(root: Path, name: str, *entries: dict[str, JsonValue]) -> None:
    directory = root / "flavor-translations"
    directory.mkdir(parents=True, exist_ok=True)
    (directory / name).write_bytes(
        canonical({"kind": "flavor_translation_shard", "entries": list(entries)})
    )


def test_missing_directory_means_no_translations(tmp_path: Path) -> None:
    assert load(tmp_path) == {}


def test_load_reads_every_shard_keyed_by_source_hash(tmp_path: Path) -> None:
    shard(tmp_path, "1.yaml", entry(H1, "甲\n乙"))
    shard(tmp_path, "2.yaml", entry(H2, "丙", low_confidence=True))
    loaded = load(tmp_path)
    assert set(loaded) == {(H1, "zh-Hant"), (H2, "zh-Hant")}
    assert loaded[H1, "zh-Hant"].text == "甲\n乙"
    assert loaded[H2, "zh-Hant"].low_confidence is True


@pytest.mark.parametrize(
    "bad",
    [
        entry(text=" 前"),
        entry(text="後 "),
        entry(text="a \nb"),
        entry(text="a\r\nb"),
        entry(text=""),
        entry(origin="official"),
        entry(source_hash="sha256:short"),
        entry(lang="en"),
        entry() | {"jp_text": "原文"},
    ],
)
def test_invalid_entries_are_rejected(
    tmp_path: Path, bad: dict[str, JsonValue]
) -> None:
    shard(tmp_path, "1.yaml", bad)
    with pytest.raises(ValueError, match="Invalid flavor translation shard"):
        load(tmp_path)


def test_entries_must_be_sorted_and_unique_within_a_shard(tmp_path: Path) -> None:
    shard(tmp_path, "1.yaml", entry(H2), entry(H1))
    with pytest.raises(ValueError, match="Invalid flavor translation shard"):
        load(tmp_path)


def test_one_source_text_cannot_appear_in_two_shards(tmp_path: Path) -> None:
    shard(tmp_path, "1.yaml", entry())
    shard(tmp_path, "2.yaml", entry())
    with pytest.raises(ValueError, match="two shards"):
        load(tmp_path)


def test_directory_holds_only_yaml_files(tmp_path: Path) -> None:
    shard(tmp_path, "1.yaml", entry())
    (tmp_path / "flavor-translations" / "notes.txt").write_text("x")
    with pytest.raises(ValueError, match="only YAML"):
        load(tmp_path)


@pytest.fixture(scope="module")
def template() -> DatabaseTemplate:
    schema = compile_current_build(("t0", "translation_evidence", "translation_names"))
    with create_database(schema) as db:
        seed(db)
        return DatabaseTemplate(schema, db._connection.serialize())


# The seeded text unit carries HASH as its content hash.
TEXT = Entry(
    source_hash=HASH,
    lang="zh-Hant",
    text="風味譯文",
    origin="machine",
    low_confidence=True,
)
ENTRIES = {(HASH, "zh-Hant"): TEXT}


def flavored(db: Database, state: str = "verified", unit: str | None = "text") -> None:
    with db.transaction():
        db.update(
            "printing_face",
            {"printing_id": "printing", "face_id": "face"},
            {"flavor_unit_id": unit, "printed_text_state": state},
        )


def test_apply_writes_a_flavor_use_with_machine_quality(
    template: DatabaseTemplate,
) -> None:
    with template.copy() as db:
        flavored(db)
        with db.transaction():
            report = apply(db, ENTRIES)
        assert report.payload() == {"entries": 1, "applied": 1, "unused": 0}
        use = db.rows("translation_use")[0].values
        assert (use["printing_id"], use["face_id"], use["field"]) == (
            "printing",
            "face",
            "flavor",
        )
        row = db.rows("translation")[0].values
        assert row["text"] == "風味譯文"
        assert (row["origin"], row["authority"]) == ("machine", "unofficial")
        assert row["low_confidence"] is True
        assert row["target_lang"] == "zh-Hant"


@pytest.mark.parametrize(
    ("state", "unit"),
    [("unknown", "text"), ("omitted", "text"), ("verified", None)],
)
def test_face_without_known_flavor_keeps_the_original(
    template: DatabaseTemplate, state: str, unit: str | None
) -> None:
    with template.copy() as db:
        flavored(db, state, unit)
        with db.transaction():
            report = apply(db, ENTRIES)
        assert report.applied == 0
        assert db.rows("translation") == ()


def test_changed_source_text_keeps_the_original(template: DatabaseTemplate) -> None:
    with template.copy() as db:
        flavored(db)
        old = TEXT.model_copy(update={"source_hash": H1})
        with db.transaction():
            report = apply(db, {(H1, "zh-Hant"): old})
        assert report.payload() == {"entries": 1, "applied": 0, "unused": 1}
        assert db.rows("translation") == ()


def test_unconfirmed_card_keeps_the_original(template: DatabaseTemplate) -> None:
    with template.copy() as db:
        flavored(db)
        with db.transaction():
            db.update("card", {"id": "card"}, {"identity_state": "provisional"})
            assert apply(db, ENTRIES).applied == 0


def test_translating_non_japanese_text_is_rejected(template: DatabaseTemplate) -> None:
    with template.copy() as db:
        with db.transaction():
            db.insert(
                "language",
                {"code": "en", "fallback_order": Json([]), "display_name": "English"},
            )
            db.insert(
                "text_unit",
                {"id": "english", "lang": "en", "text": "Flavor", "content_hash": H2},
            )
        flavored(db, unit="english")
        with db.transaction(), pytest.raises(ValueError, match="Japanese text"):
            apply(db, ENTRIES)


def flavored_text[**P](
    populate: Callable[Concatenate[Database, P], InputRecord],
) -> Callable[Concatenate[Database, P], InputRecord]:
    """The synthetic pages carry no flavor, so give one JP face a Japanese text unit."""

    def wrapped(db: Database, /, *args: P.args, **kwargs: P.kwargs) -> InputRecord:
        result = populate(db, *args, **kwargs)
        row = next(
            r.values
            for r in db.rows("printing_face")
            if db.select(
                "printing", ("id", "region"), where={"id": r.values["printing_id"]}
            )[0].values["region"]
            == "jp"
        )
        db.insert(
            "text_unit",
            {
                "id": FLAVOR_UNIT,
                "lang": "ja",
                "text": "合成風味",
                "content_hash": digest("合成風味".encode()),
            },
        )
        db.update(
            "printing_face",
            {"printing_id": row["printing_id"], "face_id": row["face_id"]},
            {"flavor_unit_id": FLAVOR_UNIT, "printed_text_state": "verified"},
        )
        return result

    return wrapped


def test_offline_build_projects_flavor_translation_and_reports_counts(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _, recipe, _ = prepared
    monkeypatch.setattr(
        offline, "populate_text_preview", flavored_text(populate_text_preview)
    )
    plain = build(recipe)
    assert plain.report["flavor_translations"] == {
        "entries": 0,
        "applied": 0,
        "unused": 0,
    }
    faces = [
        (printing, face)
        for printing in plain.projection.tables["printing"]
        if printing["region"] == "jp"
        for raw in array(printing["faces"])
        if (face := object_value(raw))["flavor_unit_id"] is not None
    ]
    assert faces
    printing, face = faces[0]
    shard(
        recipe.repo / "authored",
        "1.yaml",
        entry(digest("合成風味".encode()), "合成風味譯文"),
    )
    built = build(recipe, bundle_dir=tmp_path / "bundle")
    assert built.report["flavor_translations"] == {
        "entries": 1,
        "applied": 1,
        "unused": 0,
    }
    projected = next(
        object_value(raw)
        for row in built.projection.tables["printing"]
        if row["id"] == printing["id"]
        for raw in array(row["faces"])
        if object_value(raw)["face_id"] == face["face_id"]
    )
    bindings = [
        object_value(raw)
        for raw in array(projected["translations"])
        if object_value(raw)["field"] == "flavor"
    ]
    assert len(bindings) == 1
    assert (bindings[0]["target_lang"], bindings[0]["basis"]) == (
        "zh-Hant",
        "own_source",
    )
    translation = next(
        row
        for row in built.projection.tables["translation"]
        if row["id"] == bindings[0]["translation_id"]
    )
    assert (translation["origin"], translation["low_confidence"]) == ("machine", False)
    unit = next(
        row
        for row in built.projection.tables["text_unit"]
        if row["id"] == translation["text_unit_id"]
    )
    assert unit["text"] == "合成風味譯文"
