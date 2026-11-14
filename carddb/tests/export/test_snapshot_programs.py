"""Admitted programs round-trip with exact card references and closed DSL admission."""

from copy import deepcopy
from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
import referencing
from jsonschema import Draft202012Validator, ValidationError
from pydantic import JsonValue
from referencing import Resource

from sve_carddb.build import Json, create_database
from sve_carddb.build.compiler import compile_schema
from sve_carddb.build.dsl import TABLES, schemas
from sve_carddb.build.model import Capability
from sve_carddb.build.registry import Registry
from sve_carddb.contracts.generate_schema import generate
from sve_carddb.contracts.snapshot import schema as contract_schema
from sve_carddb.contracts.snapshot import validate
from sve_carddb.core.json import array, canonical, digest, object_value, parse, string
from sve_carddb.export.media_config import offline_configuration
from sve_carddb.export.preview import Roots, write_preview
from sve_carddb.export.read_api import load_snapshot
from sve_carddb.export.reader import read_snapshot
from sve_carddb.export.transport import Ownership, export_snapshot

from ..support.snapshot_contract_fixtures import _reseal
from ..support.snapshot_project_fixtures import populate, schema
from .test_snapshot_export import BATCH
from .test_snapshot_project import projected

if TYPE_CHECKING:
    from referencing.jsonschema import Schema

    from sve_carddb.export.project import Projection
    from sve_carddb.export.transport import Snapshot


@pytest.fixture(scope="module")
def admitted() -> tuple[Projection, Ownership]:
    base = schema()
    tables = (*base.tables, *TABLES)
    merged = compile_schema(
        Registry(
            tables,
            (
                Capability("all", tuple(table.name for table in tables)),
                Capability("reserved", ("dsl_load",), implemented=False),
            ),
        ),
        ("all",),
        {name: parse(value.encode()) for name, value in base.json_schemas} | schemas(),
    )
    with create_database(merged) as db:
        with db.transaction():
            populate(db)
            program: dict[str, JsonValue] = {
                "card_id": "card",
                "scope": "shared",
                "abilities": [],
            }
            db.insert(
                "dsl_document",
                {
                    "id": "program",
                    "card_id": "card",
                    "scope": "shared",
                    "dsl_version": "1.0",
                    "status": "draft",
                    "ast": Json(program),
                    "program_hash": digest(canonical(program)),
                },
            )
            db.update(
                "card_engine_support",
                {"card_id": "card", "region": "jp"},
                {
                    "status": "draft",
                    "dsl_id": "program",
                    "candidate_hash": digest(canonical(program)),
                    "dsl_version": "1.0",
                    "validation_state": "fresh",
                    "reason_codes": Json(["engine_unassigned"]),
                },
            )
        result = projected(db)
        result = replace(result, config=offline_configuration(result.config))
        return result, Ownership.from_database(db, result)


@pytest.fixture(scope="module")
def packed(admitted: tuple[Projection, Ownership]) -> Snapshot:
    return export_snapshot(*admitted, BATCH)


def test_programs_require_the_new_minor_profile_and_preserve_manual_support(
    packed: Snapshot, admitted: tuple[Projection, Ownership]
) -> None:
    assert packed.manifest["format_version"] == "4.1.0"
    assert packed.manifest["min_reader_version"] == "4.1.0"
    assert "dsl-programs-v1" in array(packed.manifest["required_capabilities"])
    packed.verify(admitted[0])
    payload = object_value(parse(packed.payloads["programs"].raw))
    assert payload["entries"] == [
        {
            "id": "program",
            "dsl_version": "1.0",
            "ast": {"card_id": "card", "scope": "shared", "abilities": []},
        }
    ]
    rows = admitted[0].tables["card_engine_support"]
    support = next(row for row in rows if row["card_id"] == "card")["shared"]
    assert object_value(support)["status"] == "draft"
    assert object_value(support)["reasons"] == ["engine_unassigned"]
    missing = next(row for row in rows if row["card_id"] == "old_card")["shared"]
    assert object_value(missing)["status"] == "missing_dsl"
    with pytest.raises(ValueError, match=r"4\.1\.0"):
        export_snapshot(*admitted, BATCH, format_version="4.0.0")


@pytest.mark.parametrize(
    "entries",
    [
        [{"id": "program", "dsl_version": "9.0", "ast": {}}],
        [
            {
                "id": "program",
                "dsl_version": "1.0",
                "ast": {"card_id": "card", "scope": "shared", "meta": {}},
            }
        ],
        [
            {
                "id": "program",
                "dsl_version": "1.0",
                "ast": {
                    "card_id": "card",
                    "scope": "shared",
                    "abilities": [{"use": "unexpanded"}],
                },
            }
        ],
        [
            {
                "id": "program",
                "dsl_version": "1.0",
                "ast": {
                    "card_id": "card",
                    "scope": "shared",
                    "abilities": [
                        {
                            "line": 1,
                            "ports": {"imports": {}, "exports": {}},
                            "spell": {"do": [{"unknown": {}}]},
                        }
                    ],
                },
            }
        ],
    ],
)
def test_program_schema_rejects_unknown_versions_audit_fields_and_macros(
    entries: JsonValue,
) -> None:
    with pytest.raises((ValidationError, TypeError, ValueError)):
        validate("Programs", {"format_version": "4.1.0", "entries": entries}, "4.1.0")


def test_old_format_still_rejects_every_nonempty_program_package() -> None:
    with pytest.raises(ValidationError):
        validate(
            "Programs",
            {
                "format_version": "4.0.0",
                "entries": [
                    {
                        "id": "program",
                        "dsl_version": "1.0",
                        "ast": {"card_id": "card", "scope": "shared"},
                    }
                ],
            },
            "4.0.0",
        )


@pytest.mark.parametrize(
    "change",
    ["missing", "duplicate", "wrong_card", "wrong_face", "unreferenced", "wrong_ref"],
)
def test_reader_rejects_inexact_program_closure(packed: Snapshot, change: str) -> None:
    manifest = deepcopy(packed.manifest)
    payloads = {key: parse(blob.raw) for key, blob in packed.payloads.items()}
    programs = object_value(payloads["programs"])
    entries = programs["entries"]
    assert isinstance(entries, list)
    if change == "missing":
        entries.clear()
    elif change == "duplicate":
        entries.extend(deepcopy(entries))
    elif change == "wrong_card":
        object_value(object_value(entries[0])["ast"])["card_id"] = "old_card"
    elif change == "wrong_face":
        object_value(object_value(entries[0])["ast"])["faces"] = {
            "not-this-card": {"abilities": []}
        }
    elif change == "unreferenced":
        entries.append(object_value(entries[0]) | {"id": "unreferenced"})
    else:
        object_value(entries[0])["id"] = "other_program"
    sealed = _reseal(manifest, payloads)
    with pytest.raises((ValueError, ValidationError)):
        read_snapshot(manifest, sealed)


def test_required_capability_cannot_be_omitted(packed: Snapshot) -> None:
    manifest = deepcopy(packed.manifest)
    capabilities = manifest["required_capabilities"]
    assert isinstance(capabilities, list)
    capabilities.remove("dsl-programs-v1")
    with pytest.raises((ValueError, ValidationError)):
        read_snapshot(
            manifest, {key: blob.raw for key, blob in packed.payloads.items()}
        )


def test_program_profile_schema_is_reproducible() -> None:
    assert parse(generate("4.1.0")) == contract_schema("4.1.0")


def test_preview_reader_loads_program_profiles(
    packed: Snapshot, admitted: tuple[Projection, Ownership], tmp_path: Path
) -> None:
    roots = Roots(tmp_path / "public", tmp_path / "private")
    write_preview(packed, roots, {})
    loaded = load_snapshot(roots.preview)
    assert loaded.manifest["format_version"] == "4.1.0"
    assert loaded.tables == admitted[0].tables


def test_snapshot_verify_compares_programs_as_well_as_tables(
    packed: Snapshot, admitted: tuple[Projection, Ownership]
) -> None:
    with pytest.raises(ValueError, match="public programs"):
        packed.verify(replace(admitted[0], programs=()))


def test_expanded_program_rejects_typed_parameter_references() -> None:
    value: JsonValue = {
        "card_id": "card",
        "scope": "shared",
        "abilities": [
            {
                "line": 1,
                "ports": {"imports": {}, "exports": {}},
                "keywords": ["synthetic-keyword"],
            }
        ],
    }
    root = Path(__file__).resolve().parents[3] / "dsl"
    nodes = object_value(parse((root / "author-nodes.schema.json").read_bytes()))
    public = object_value(parse((root / "program.schema.json").read_bytes()))
    registry: referencing.Registry[Schema] = referencing.Registry().with_resource(
        string(nodes["$id"]), Resource.from_contents(nodes)
    )
    authority = Draft202012Validator(public, registry=registry)
    authority.validate(value)
    validate("DSLProgram", value, "4.1.0")
    object_value(array(object_value(value)["abilities"])[0])["keywords"] = [
        {"param": "keyword"}
    ]
    with pytest.raises(ValidationError):
        authority.validate(value)
    with pytest.raises(ValidationError):
        validate("DSLProgram", value, "4.1.0")
