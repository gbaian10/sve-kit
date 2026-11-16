"""The battle producer keeps native bytes and public source identities in one closure."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import array, canonical, digest, object_value, parse, string
from sve_carddb.export.battle_input import build_battle_inputs
from sve_carddb.export.preview import Roots, write_preview
from sve_carddb.export.read_api import ExportError, Snapshot, load_battle_inputs
from tests.support.battle_projection import snapshot as compatible_snapshot

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.export.reader import Row


def source() -> Snapshot:
    tables: dict[str, list[Row]] = {
        table: []
        for table in (
            "card",
            "face",
            "printing",
            "face_revision",
            "text_unit",
            "vocabulary",
            "qa_version",
            "cr_clause",
            "product_family",
            "product",
            "keyword",
        )
    }
    tables["card"] = [
        {
            "id": "card-a",
            "faces": ["face-front", "face-back"],
            "regions": [{"region": "jp", "mapping_state": "confirmed"}],
        }
    ]
    tables["face"] = [
        {"id": "face-front", "card_id": "card-a", "ordinal": 0},
        {"id": "face-back", "card_id": "card-a", "ordinal": 1},
    ]
    tables["printing"] = [
        {
            "id": "printing-a",
            "card_id": "card-a",
            "region": "jp",
            "card_no": "SYNⓈa",
            "card_no_state": "official",
            "faces": [
                {
                    "face_id": "face-back",
                    "observations": [{"state": "available", "revision_id": "rev-back"}],
                },
                {
                    "face_id": "face-front",
                    "observations": [
                        {"state": "available", "revision_id": "rev-front"}
                    ],
                },
            ],
        }
    ]
    tables["face_revision"] = [
        {
            "id": revision,
            "face_id": face,
            "region": "jp",
            "name_unit_id": name,
            "effect_unit_id": "text-effect",
            "class_code": "neutral",
            "type_code": "follower",
            "cost": 1,
            "attack": 2,
            "defense": 3,
            "traits": [],
            "titles": [],
            "special_kinds": [],
            "sections": [],
        }
        for revision, face, name in [
            ("rev-front", "face-front", "text-front"),
            ("rev-back", "face-back", "text-back"),
        ]
    ]
    tables["text_unit"] = [
        {"id": identifier, "lang": "ja", "text": text}
        for identifier, text in [
            ("text-front", "Synthetic front"),
            ("text-back", "Synthetic back"),
            ("text-effect", "Synthetic effect"),
            ("label-neutral", "ニュートラル"),
            ("label-follower", "フォロワー"),
        ]
    ]
    tables["vocabulary"] = [
        {"kind": "class", "code": "neutral", "label_unit_id": "label-neutral"},
        {"kind": "type", "code": "follower", "label_unit_id": "label-follower"},
    ]
    return Snapshot({"regions": ["jp"]}, "sha256:" + "a" * 64, tables)


def test_battle_input_preserves_all_faces_and_exact_native_bytes() -> None:
    result = build_battle_inputs(source(), "jp")
    rows = [object_value(parse(line)) for line in result.snapshot.splitlines()]
    assert len(rows) == 1
    assert rows[0]["number"] == "SYNⓈa"
    assert [object_value(face)["name"] for face in array(rows[0]["faces"])] == [
        "Synthetic front",
        "Synthetic back",
    ]
    assert object_value(array(rows[0]["faces"])[0]) == {
        "name": "Synthetic front",
        "card_class": "ニュートラル",
        "card_type": "フォロワー",
        "cost": "1",
        "power": "2",
        "hp": "3",
        "traits": [],
        "text": "Synthetic effect",
        "sections": [],
    }
    assert result.references["region"] == "jp"
    assert result.references["snapshot_sha256"] == digest(result.snapshot).removeprefix(
        "sha256:"
    )
    assert result.references["faces"] == [
        {
            "card_no": "SYNⓈa",
            "native_face": 0,
            "card": {"printing_id": "printing-a", "face": 0},
        },
        {
            "card_no": "SYNⓈa",
            "native_face": 1,
            "card": {"printing_id": "printing-a", "face": 1},
        },
    ]
    wording = object_value(result.references["wording"])
    assert wording["manifest_sha256"] == "sha256:" + "a" * 64
    assert wording["faces"] == [
        {
            "card_no": "SYNⓈa",
            "native_face": 0,
            "face_id": "face-front",
            "revisions": [{"revision_id": "rev-front", "fields": [{"kind": "effect"}]}],
        },
        {
            "card_no": "SYNⓈa",
            "native_face": 1,
            "face_id": "face-back",
            "revisions": [{"revision_id": "rev-back", "fields": [{"kind": "effect"}]}],
        },
    ]


def test_battle_input_keeps_observed_wording_and_all_historical_owners() -> None:
    snapshot = source()
    latest = snapshot.tables["face_revision"][0] | {
        "id": "rev-latest",
        "name_unit_id": "text-latest",
    }
    snapshot.tables["face_revision"].append(latest)
    snapshot.tables["text_unit"].append(
        {"id": "text-latest", "lang": "ja", "text": "Synthetic latest"}
    )
    snapshot.tables["face"][0]["current"] = [
        {"region": "jp", "revision_id": "rev-latest"}
    ]
    result = build_battle_inputs(snapshot, "jp")
    row = object_value(parse(result.snapshot.strip()))
    assert object_value(array(row["faces"])[0])["name"] == "Synthetic front"
    wordings = array(object_value(result.references["wording"])["faces"])
    revisions = array(object_value(wordings[0])["revisions"])
    assert {object_value(row)["revision_id"] for row in revisions} == {
        "rev-front",
        "rev-latest",
    }


def test_battle_input_reads_the_verified_public_manifest(tmp_path: Path) -> None:
    snapshot = compatible_snapshot()
    roots = Roots(tmp_path / "public", tmp_path / "private")
    write_preview(snapshot, roots, {})
    result = load_battle_inputs(roots.preview, "jp")
    rows = [object_value(parse(line)) for line in result.snapshot.splitlines()]
    assert len(rows) == 2
    row = rows[0]
    assert row["number"] == "TEST-001Ⓢa"
    amulet = object_value(array(rows[1]["faces"])[0])
    assert amulet["card_type"] == "アミュレット"
    assert (amulet["power"], amulet["hp"]) == ("-", "-")
    assert result.references["faces"] == [
        {
            "card_no": "TEST-001Ⓢa",
            "native_face": 0,
            "card": {"printing_id": "printing", "face": 0},
        },
        {
            "card_no": "TEST-002",
            "native_face": 0,
            "card": {"printing_id": "printing-amulet", "face": 0},
        },
    ]
    assert object_value(result.references["wording"])["manifest_sha256"] == digest(
        canonical(snapshot.manifest)
    )
    with pytest.raises(ExportError):
        load_battle_inputs(roots.preview, "en")
    blob = object_value(array(snapshot.manifest["files"])[0])
    (roots.preview / string(blob["path"])).write_bytes(b"{}")
    with pytest.raises(ExportError):
        load_battle_inputs(roots.preview, "jp")


def test_historical_section_ordinals_do_not_reindex_native_printed_sections() -> None:
    snapshot = source()
    snapshot.tables["face_revision"].append(
        snapshot.tables["face_revision"][0]
        | {
            "id": "rev-section",
            "sections": [{"ordinal": 2, "text_unit_id": "text-effect"}],
        }
    )
    result = build_battle_inputs(snapshot, "jp")
    first = object_value(
        array(object_value(parse(result.snapshot.strip()))["faces"])[0]
    )
    assert first["sections"] == []
    face = object_value(array(object_value(result.references["wording"])["faces"])[0])
    history = next(
        object_value(row)
        for row in array(face["revisions"])
        if object_value(row)["revision_id"] == "rev-section"
    )
    assert history["fields"] == [{"kind": "effect"}, {"kind": "section", "ordinal": 2}]


def test_battle_input_includes_unselected_printings_and_tokens() -> None:
    snapshot = source()
    snapshot.tables["card"].append(
        {
            "id": "card-token",
            "faces": ["face-token"],
            "regions": [{"region": "jp", "mapping_state": "confirmed"}],
        }
    )
    snapshot.tables["face"].append(
        {"id": "face-token", "card_id": "card-token", "ordinal": 0}
    )
    snapshot.tables["face_revision"].append(
        snapshot.tables["face_revision"][0]
        | {
            "id": "rev-token",
            "face_id": "face-token",
            "special_kinds": ["token"],
        }
    )
    snapshot.tables["printing"].append(
        {
            "id": "printing-token",
            "card_id": "card-token",
            "region": "jp",
            "card_no": "TOKENⓈa",
            "card_no_state": "official",
            "faces": [
                {
                    "face_id": "face-token",
                    "observations": [
                        {"state": "available", "revision_id": "rev-token"}
                    ],
                }
            ],
        }
    )
    snapshot.tables["text_unit"].append(
        {"id": "label-token", "lang": "ja", "text": "トークン"}
    )
    snapshot.tables["vocabulary"].append(
        {"kind": "special_kind", "code": "token", "label_unit_id": "label-token"}
    )
    result = build_battle_inputs(snapshot, "jp")
    rows = [object_value(parse(line)) for line in result.snapshot.splitlines()]
    assert [row["number"] for row in rows] == ["SYNⓈa", "TOKENⓈa"]
    assert (
        object_value(array(rows[1]["faces"])[0])["card_type"] == "フォロワー・トークン"
    )
    assert len(array(result.references["faces"])) == 3
    assert object_value(array(result.references["faces"])[2])["card"] == {
        "printing_id": "printing-token",
        "face": 0,
    }


def test_missing_effect_text_does_not_publish_an_exact_effect_field() -> None:
    snapshot = source()
    snapshot.tables["face_revision"][0]["effect_unit_id"] = None
    result = build_battle_inputs(snapshot, "jp")
    first = object_value(
        array(object_value(parse(result.snapshot.strip()))["faces"])[0]
    )
    assert first["text"] is None
    face = object_value(array(object_value(result.references["wording"])["faces"])[0])
    assert face["revisions"] == [{"revision_id": "rev-front", "fields": []}]


@pytest.mark.parametrize(
    "scenario",
    [
        "foreign_region",
        "unknown_revision",
        "ambiguous_revision",
        "partial_faces",
        "duplicate_number",
        "noncontiguous_native_sections",
        "missing_vocabulary",
        "invalid_manifest",
        "empty_catalog",
    ],
)
def test_battle_input_refuses_incomplete_or_ambiguous_catalogues(scenario: str) -> None:
    snapshot = source()
    printing = snapshot.tables["printing"][0]
    front = object_value(array(printing["faces"])[1])
    match scenario:
        case "foreign_region":
            snapshot.tables["face_revision"][0]["region"] = "en"
        case "unknown_revision":
            front["observations"] = [{"state": "available", "revision_id": "absent"}]
        case "ambiguous_revision":
            snapshot.tables["face_revision"].append(
                snapshot.tables["face_revision"][0] | {"id": "rev-other"}
            )
            array(front["observations"]).append(
                {"state": "available", "revision_id": "rev-other"}
            )
        case "partial_faces":
            array(printing["faces"]).pop()
        case "duplicate_number":
            snapshot.tables["printing"].append(printing | {"id": "another-printing"})
        case "noncontiguous_native_sections":
            snapshot.tables["face_revision"][0]["sections"] = [
                {"ordinal": 2, "text_unit_id": "text-effect"}
            ]
        case "missing_vocabulary":
            snapshot.tables["vocabulary"] = []
        case "invalid_manifest":
            snapshot = Snapshot(
                snapshot.manifest, "not-a-manifest-hash", snapshot.tables
            )
        case "empty_catalog":
            snapshot.tables["printing"] = []
        case _:
            raise AssertionError("Unknown synthetic mutation")
    with pytest.raises(ValueError, match="Battle snapshot projection is unavailable"):
        build_battle_inputs(snapshot, "jp")


def test_manual_evolution_resources_do_not_become_native_card_printings() -> None:
    snapshot = source()
    snapshot.tables["card"].append(
        {
            "id": "resource",
            "faces": ["resource-face"],
            "regions": [{"region": "jp", "mapping_state": "confirmed_none"}],
        }
    )
    snapshot.tables["face"].append(
        {"id": "resource-face", "card_id": "resource", "ordinal": 0}
    )
    snapshot.tables["printing"].append(
        {
            "id": "resource-print",
            "card_id": "resource",
            "region": "jp",
            "card_no": "",
            "card_no_state": "none",
            "manual_type_code": "ep",
            "faces": [{"face_id": "resource-face", "observations": []}],
        }
    )
    result = build_battle_inputs(snapshot, "jp")
    assert len(result.snapshot.splitlines()) == 1
    assert len(array(result.references["faces"])) == 2


def test_en_catalogue_preserves_its_own_region_without_normalizing_card_numbers() -> (
    None
):
    snapshot = source()
    snapshot.manifest["regions"] = ["en"]
    snapshot.tables["card"][0]["regions"] = [
        {"region": "en", "mapping_state": "confirmed"}
    ]
    for revision in snapshot.tables["face_revision"]:
        revision["region"] = "en"
    snapshot.tables["printing"][0]["region"] = "en"
    snapshot.tables["printing"][0]["card_no"] = "SYNⓈaEN"
    for text in snapshot.tables["text_unit"]:
        if text["id"] in {"text-front", "text-back", "text-effect"}:
            text["lang"] = "en"
    result = build_battle_inputs(snapshot, "en")
    assert object_value(parse(result.snapshot.strip()))["number"] == "SYNⓈaEN"
    assert result.references["region"] == "en"
    assert {
        object_value(row)["card_no"] for row in array(result.references["faces"])
    } == {"SYNⓈaEN"}


@pytest.mark.parametrize(
    "scenario", ["missing_class", "empty_class", "noncontiguous_faces"]
)
def test_native_characteristic_and_face_coordinates_must_be_representable(
    scenario: str,
) -> None:
    snapshot = source()
    if scenario == "missing_class":
        snapshot.tables["face_revision"][0]["class_code"] = None
    elif scenario == "empty_class":
        next(
            row for row in snapshot.tables["text_unit"] if row["id"] == "label-neutral"
        )["text"] = ""
    else:
        snapshot.tables["face"][1]["ordinal"] = 2
    with pytest.raises(ValueError, match="Battle snapshot projection is unavailable"):
        build_battle_inputs(snapshot, "jp")


def test_native_absent_numeric_characteristics_use_the_existing_sentinel() -> None:
    snapshot = source()
    snapshot.tables["face_revision"][0].update(
        {"cost": None, "attack": None, "defense": None}
    )
    result = build_battle_inputs(snapshot, "jp")
    first = object_value(
        array(object_value(parse(result.snapshot.strip()))["faces"])[0]
    )
    assert [first[field] for field in ("cost", "power", "hp")] == ["-", "-", "-"]
