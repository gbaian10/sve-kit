"""The battle producer keeps native bytes and public source identities in one closure."""

from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb import cli
from sve_carddb.core.json import array, canonical, digest, object_value, parse, string
from sve_carddb.export.battle_input import build_battle_inputs
from sve_carddb.export.preview import Roots, write_preview
from sve_carddb.export.read_api import ExportError, Snapshot, load_battle_inputs
from tests.support.battle_projection import snapshot as compatible_snapshot

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

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
            "qa",
            "qa_version",
            "cr_version",
            "cr_clause",
            "ruling_revision",
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
    rules = object_value(result.references["rules"])
    assert rules["manifest_sha256"] == digest(canonical(snapshot.manifest))
    assert {
        "kind": "ruling",
        "id": "ruling",
        "revision": "ruling-rev",
        "strength": "inferred",
    } in array(rules["entries"])
    with pytest.raises(ExportError):
        load_battle_inputs(roots.preview, "en")

    def command(output: Path, rules: str, region: str = "jp") -> int:
        engine_rules = tmp_path / f"{output.name}.yaml"
        engine_rules.write_text(rules, encoding="utf-8")
        arguments = ["--output", str(output), "--engine-rules", str(engine_rules)]
        arguments += ["--export-dir", str(roots.preview), "--region", region]
        return (
            CliRunner()
            .invoke(cli.app, ["snapshot", "battle-inputs", *arguments])
            .exit_code
        )

    pinned = (
        "scope: {region: jp}\ninput: {kind: legacy-jp, snapshot_sha256: reviewed}\n"
    )
    assert command(tmp_path / "battle", pinned) == 0
    assert (tmp_path / "battle/snapshot.jsonl").read_bytes() == result.snapshot
    assert parse((tmp_path / "battle/references.json").read_bytes()) == (
        result.references
    )
    assert parse((tmp_path / "battle/rules.json").read_bytes()) == {
        "scope": {"region": "jp"},
        "input": {
            "kind": "legacy-jp",
            "snapshot_sha256": digest(result.snapshot).removeprefix("sha256:"),
        },
    }
    assert command(tmp_path / "battle", pinned) != 0, "never mixes with earlier output"
    for name, document, region in [
        ("en", pinned, "en"),
        ("other", pinned.replace("jp", "en"), "jp"),
        ("unusable", "scope: {}\n", "jp"),
    ]:
        assert command(tmp_path / name, document, region) == 3, name
        assert not (tmp_path / name).exists()
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


@pytest.mark.parametrize("scenario", ["empty_class", "noncontiguous_faces"])
def test_native_characteristic_and_face_coordinates_must_be_representable(
    scenario: str,
) -> None:
    snapshot = source()
    if scenario == "empty_class":
        next(
            row for row in snapshot.tables["text_unit"] if row["id"] == "label-neutral"
        )["text"] = ""
    else:
        snapshot.tables["face"][1]["ordinal"] = 2
    with pytest.raises(ValueError, match="Battle snapshot projection is unavailable"):
        build_battle_inputs(snapshot, "jp")


def test_native_absent_characteristics_use_the_existing_sentinel() -> None:
    snapshot = source()
    # EP/SEP faces have neither a class nor numeric characteristics.
    snapshot.tables["face_revision"][0].update(
        {"class_code": None, "cost": None, "attack": None, "defense": None}
    )
    result = build_battle_inputs(snapshot, "jp")
    first = object_value(
        array(object_value(parse(result.snapshot.strip()))["faces"])[0]
    )
    fields = ("card_class", "cost", "power", "hp")
    assert [first[field] for field in fields] == ["-", "-", "-", "-"]


def test_battle_rule_identity_closure_keeps_parent_ids_and_all_exact_versions() -> None:
    snapshot = source()
    snapshot.tables["cr_version"] = [
        {"id": "cr-old", "region": "jp", "version": "1.0"},
        {"id": "cr-new", "region": "jp", "version": "2.0"},
        {"id": "cr-en", "region": "en", "version": "2.0"},
    ]
    snapshot.tables["cr_clause"] = [
        {"id": "clause-1", "cr_version_id": "cr-old", "number": "1.2"},
        {"id": "clause-2", "cr_version_id": "cr-new", "number": "1.2"},
        {"id": "clause-3", "cr_version_id": "cr-new", "number": "3.1"},
        {"id": "clause-en", "cr_version_id": "cr-en", "number": "1.2"},
    ]
    snapshot.tables["qa"] = [
        {"id": "answer-parent", "region": "jp", "official_number": "00042"},
        {"id": "answer-en", "region": "en", "official_number": "00042"},
    ]
    snapshot.tables["qa_version"] = [
        {"id": "qa-old", "qa_id": "answer-parent", "revision": 0},
        {"id": "qa-new", "qa_id": "answer-parent", "revision": 1},
        {"id": "qa-en", "qa_id": "answer-en", "revision": 0},
    ]
    snapshot.tables["ruling_revision"] = [
        {
            "id": "ruling-old",
            "ruling_id": "ruling-parent",
            "revision": 0,
            "strength": "generalized",
        },
        {
            "id": "ruling-new",
            "ruling_id": "ruling-parent",
            "revision": 1,
            "strength": "inferred",
        },
    ]
    rules = object_value(build_battle_inputs(snapshot, "jp").references["rules"])
    assert rules["manifest_sha256"] == snapshot.manifest_sha256
    expected: list[JsonValue] = [
        {"kind": "cr", "id": "1.2", "revision": "cr-old"},
        {"kind": "cr", "id": "1.2", "revision": "cr-new"},
        {"kind": "cr", "id": "3.1", "revision": "cr-new"},
        {"kind": "qa", "id": "answer-parent", "revision": "qa-old"},
        {"kind": "qa", "id": "answer-parent", "revision": "qa-new"},
        {
            "kind": "ruling",
            "id": "ruling-parent",
            "revision": "ruling-old",
            "strength": "generalized",
        },
        {
            "kind": "ruling",
            "id": "ruling-parent",
            "revision": "ruling-new",
            "strength": "inferred",
        },
    ]
    assert sorted(array(rules["entries"]), key=canonical) == sorted(
        expected, key=canonical
    )


@pytest.mark.parametrize(
    ("table", "rows"),
    [
        ("cr_clause", [{"id": "clause", "cr_version_id": "missing", "number": "1.2"}]),
        ("qa_version", [{"id": "answer-version", "qa_id": "missing"}]),
        (
            "ruling_revision",
            [{"id": "revision", "ruling_id": "ruling", "strength": "assumed-official"}],
        ),
        (
            "ruling_revision",
            [
                {"id": "revision", "ruling_id": "ruling", "strength": "inferred"},
                {"id": "revision", "ruling_id": "ruling", "strength": "inferred"},
            ],
        ),
    ],
)
def test_battle_rule_identity_projection_rejects_incomplete_or_ambiguous_rows(
    table: str, rows: list[Row]
) -> None:
    snapshot = source()
    snapshot.tables["cr_version"] = [{"id": "version", "region": "jp"}]
    snapshot.tables["qa"] = [{"id": "answer", "region": "jp"}]
    snapshot.tables[table] = rows
    with pytest.raises(ValueError, match="Battle snapshot projection is unavailable"):
        build_battle_inputs(snapshot, "jp")
