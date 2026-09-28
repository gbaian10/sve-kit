"""Exercise compiled DDL and typed boundaries with small synthetic declarations."""

import sqlite3

# ruff: file-ignore[pytest-raises-with-multiple-statements] -- expected failures encompass transaction writes and commit
from dataclasses import replace
from typing import TYPE_CHECKING, cast

import pytest
from jsonschema import SchemaError, ValidationError
from pydantic import JsonValue

from sve_carddb.build_db import (
    Capability,
    Check,
    Column,
    ForeignKey,
    Json,
    Kind,
    Registry,
    Table,
    Unique,
    compile_schema,
    create_database,
)
from sve_carddb.build_db.database import _raw_rows, install_functions
from sve_carddb.build_db.validation import BOUNDS, Rules, decode

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import CompiledSchema, Value

SAFE = 9007199254740991
JSON_SCHEMAS: dict[str, JsonValue] = {
    "document": {
        "type": "object",
        "properties": {"n": {"type": "integer", "minimum": 0}},
        "required": ["n"],
        "additionalProperties": False,
    },
    "anything": True,
}


def registry(*, art_implemented: bool = False) -> Registry:
    nodes = Table(
        "node",
        (
            Column("id", Kind.ID),
            Column("next_id", Kind.ID, nullable=True),
        ),
        ("id",),
        foreign_keys=(ForeignKey(("next_id",), "node", ("id",)),),
    )
    vocabulary = Table(
        "vocabulary",
        (
            Column("kind", Kind.ID),
            Column("code", Kind.ID, pattern="[a-z][a-z0-9_]*"),
        ),
        ("kind", "code"),
    )
    item = Table(
        "item",
        (
            Column("id", Kind.ID),
            Column("node_id", Kind.ID),
            Column("art_id", Kind.ID, nullable=True),
            Column("label_kind", Kind.TEXT, fixed="label"),
            Column("label_code", Kind.ID, nullable=True),
            Column("state", Kind.TEXT, choices=("official", "provisional")),
            Column("external_key", Kind.TEXT),
            Column("mark", Kind.TEXT, fixed="creator's"),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("node_id",), "node", ("id",)),
            ForeignKey(("label_kind", "label_code"), "vocabulary", ("kind", "code")),
            ForeignKey(("art_id", "node_id"), "art", ("id", "node_id")),
        ),
        unique=(Unique(("external_key",), where="state = 'official'"),),
        checks=(Check("length(external_key) > 0"),),
    )
    numbers = Table(
        "numbers",
        (
            Column("id", Kind.ID),
            Column("unsigned", Kind.UINT),
            Column("signed", Kind.INT),
            Column("serial", Kind.UINT32),
            Column("flag", Kind.BOOL),
            Column("document", Kind.JSON, json_schema="document"),
            Column("payload", Kind.JSON, nullable=True, json_schema="anything"),
        ),
        ("id",),
        checks=(Check('"signed" <= "unsigned"'),),
    )
    tables = [nodes, vocabulary, item, numbers]
    if art_implemented:
        tables.append(
            Table(
                "art",
                (Column("id", Kind.ID), Column("node_id", Kind.ID)),
                ("id",),
                unique=(Unique(("id", "node_id")),),
                foreign_keys=(ForeignKey(("node_id",), "node", ("id",)),),
            )
        )
    return Registry(
        tuple(tables),
        (
            Capability("base", ("node", "vocabulary")),
            Capability("catalog", ("item",)),
            Capability("numeric", ("numbers",)),
            Capability("images", ("art",), implemented=art_implemented),
        ),
    )


def compiled(*, images: bool = False) -> CompiledSchema:
    requested = ("catalog", "numeric", "images") if images else ("catalog", "numeric")
    return compile_schema(registry(art_implemented=images), requested, JSON_SCHEMAS)


def number_row() -> dict[str, Value]:
    return {
        "id": "n",
        "unsigned": SAFE,
        "signed": 0,
        "serial": 1,
        "flag": True,
        "document": Json({"n": 1}),
        "payload": None,
    }


def item_row(identifier: str = "p") -> dict[str, Value]:
    return {
        "id": identifier,
        "node_id": "node:a",
        "art_id": None,
        "label_code": None,
        "state": "official",
        "external_key": "SYN-001",
    }


def test_ddl_is_deterministic_and_executable_on_a_scratch_database() -> None:
    original = registry()
    shuffled = Registry(
        tuple(reversed(original.tables)), tuple(reversed(original.capabilities))
    )
    schema = compiled()
    assert (
        schema.sql
        == compile_schema(shuffled, ("numeric", "catalog", "catalog"), JSON_SCHEMAS).sql
    )
    assert {table.name for table in schema.tables} == {
        "node",
        "vocabulary",
        "item",
        "numbers",
    }
    assert 'CREATE TABLE "art"' not in schema.sql
    connection = sqlite3.connect(":memory:")
    try:
        install_functions(connection, schema)
        connection.execute("PRAGMA foreign_keys=ON")
        connection.executescript(schema.sql)
        connection.execute("INSERT INTO node VALUES ('a', NULL)")
        connection.commit()
    finally:
        connection.close()


def test_fixed_kind_defaults_and_nullable_compound_fk() -> None:
    with create_database(compiled()) as db:
        with db.transaction():
            db.insert("node", {"id": "node:a"})
            db.insert("vocabulary", {"kind": "label", "code": "sample"})
            db.insert("item", item_row() | {"label_code": "sample"})
        row = db.rows("item")[0]
        assert row.values["label_kind"] == "label"
        assert row.values["mark"] == "creator's"
        assert row.values["art_id"] is None
        with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
            with db.transaction():
                db._connection.execute("UPDATE item SET label_kind='other'")
        with pytest.raises(sqlite3.IntegrityError, match="NOT NULL"):
            with db.transaction():
                db._connection.execute("UPDATE item SET label_kind=NULL")
        with pytest.raises(sqlite3.IntegrityError, match="Foreign key"):
            with db.transaction():
                db.update("item", {"id": "p"}, {"label_code": "unknown"})
        assert db.rows("item")[0].values["label_code"] == "sample"


def test_disabled_optional_target_rejects_non_null_and_can_be_enabled() -> None:
    with create_database(compiled()) as db:
        with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
            with db.transaction():
                db.insert("node", {"id": "node:a"})
                db.insert("item", item_row() | {"art_id": "art:a"})
        assert db.rows("node") == ()
    with create_database(compiled(images=True)) as db:
        with db.transaction():
            db.insert("item", item_row() | {"art_id": "art:a"})
            db.insert("art", {"id": "art:a", "node_id": "node:a"})
            db.insert("node", {"id": "node:a"})
        with pytest.raises(sqlite3.IntegrityError, match="Foreign key"):
            with db.transaction():
                db.insert("node", {"id": "node:b"})
                db.update("item", {"id": "p"}, {"node_id": "node:b"})


def test_deferred_cycle_and_failed_commit_roll_back_the_entire_transaction() -> None:
    with create_database(compiled()) as db:
        with db.transaction():
            db.insert("node", {"id": "a", "next_id": "b"})
            db.insert("node", {"id": "b", "next_id": "a"})
        assert len(db.rows("node")) == 2
        with pytest.raises(sqlite3.IntegrityError, match="Foreign key"):
            with db.transaction():
                db.insert("node", {"id": "c", "next_id": "absent"})
                db.insert("node", {"id": "d"})
        assert [row.values["id"] for row in db.rows("node")] == ["a", "b"]
        with pytest.raises(RuntimeError, match="cancel"):
            with db.transaction():
                db.insert("node", {"id": "e"})
                raise RuntimeError("cancel")
        assert len(db.rows("node")) == 2
        with pytest.raises(sqlite3.IntegrityError, match="FOREIGN KEY"):
            with db.transaction():
                db.delete("node", {"id": "a"})
        with pytest.raises(sqlite3.IntegrityError, match="FOREIGN KEY"):
            with db.transaction():
                db.update("node", {"id": "a"}, {"id": "changed"})
        db.verify()


def test_partial_unique_only_constrains_official_keys() -> None:
    with create_database(compiled()) as db:
        with db.transaction():
            db.insert("node", {"id": "node:a"})
            db.insert("item", item_row())
            for name in ("candidate_a", "candidate_b"):
                db.insert("item", item_row(name) | {"state": "provisional"})
        with pytest.raises(sqlite3.IntegrityError, match="UNIQUE"):
            with db.transaction():
                db.update("item", {"id": "candidate_a"}, {"state": "official"})
        with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
            with db.transaction():
                db.update("item", {"id": "p"}, {"external_key": ""})
        assert len(db.rows("item")) == 3


@pytest.mark.parametrize(
    ("column", "value"),
    [
        ("unsigned", 0),
        ("unsigned", SAFE),
        ("signed", -SAFE),
        ("signed", SAFE),
        ("serial", 0),
        ("serial", 4294967295),
        ("flag", False),
    ],
)
def test_scalar_limits_round_trip(column: str, value: Value) -> None:
    with create_database(compiled()) as db:
        with db.transaction():
            db.insert("numbers", number_row() | {column: value})
        assert db.rows("numbers")[0].values[column] == value
        assert type(db.rows("numbers")[0].values["flag"]) is bool


@pytest.mark.parametrize(
    ("column", "value"),
    [
        ("unsigned", -1),
        ("unsigned", SAFE + 1),
        ("signed", -SAFE - 1),
        ("signed", SAFE + 1),
        ("serial", -1),
        ("serial", 4294967296),
        ("unsigned", True),
        ("flag", 1),
        ("unsigned", "12"),
        ("unsigned", 1.5),
        ("id", 123),
        ("id", ""),
        ("flag", None),
        ("document", Json({"n": -1})),
        ("document", Json({"n": True})),
        ("document", Json({"extra": 1})),
        ("document", Json(None)),
        ("document", '{"n":1}'),
        ("payload", Json(1.5)),
    ],
)
def test_python_boundary_rejects_coercion_and_bad_json(
    column: str, value: object
) -> None:
    with create_database(compiled()) as db:
        with pytest.raises((ValueError, TypeError, ValidationError)):
            with db.transaction():
                db.insert("numbers", number_row() | {column: cast("Value", value)})
        assert db.rows("numbers") == ()


@pytest.mark.parametrize(
    "assignment",
    [
        "unsigned=-1",
        f"unsigned={SAFE + 1}",
        f"signed={-SAFE - 1}",
        "serial=4294967296",
        "flag=2",
        "document='not json'",
        "document='[]'",
        "document='{\"n\":-1}'",
        'document=\'{"n":1,"n":2}\'',
        "document='{\"n\":1.0}'",
        "unsigned=1.5",
        "flag=NULL",
        "signed=1, unsigned=0",
    ],
)
def test_sql_checks_cannot_be_bypassed_by_skipping_typed_writes(
    assignment: str,
) -> None:
    with create_database(compiled()) as db:
        with db.transaction():
            db.insert("numbers", number_row())
        with pytest.raises(sqlite3.IntegrityError):
            with db.transaction():
                # Only fixture-authored SQL can reach this private test connection.
                db._connection.execute("UPDATE numbers SET " + assignment)  # ruff: ignore[hardcoded-sql-expression] -- assignment is a fixture-authored SQL fragment


def test_json_null_is_distinct_from_sql_null_and_data_is_bound() -> None:
    with create_database(compiled()) as db:
        with db.transaction():
            db.insert("numbers", number_row() | {"payload": Json(None)})
            db.insert("node", {"id": "'); DROP TABLE node; --"})
        assert db.rows("numbers")[0].values["payload"] == Json(None)
        assert db.rows("node")[0].values["id"] == "'); DROP TABLE node; --"
        with db.transaction():
            db.update("numbers", {"id": "n"}, {"payload": None})
        assert db.rows("numbers")[0].values["payload"] is None


def test_mutation_guards_and_fresh_file_policy(tmp_path: Path) -> None:
    path = tmp_path / "build.sqlite"
    with create_database(compiled(), path) as db:
        with pytest.raises(RuntimeError, match="transaction"):
            db.insert("node", {"id": "a"})
        with db.transaction():
            with pytest.raises(RuntimeError, match="Nested"):
                with db.transaction():
                    pass
            with pytest.raises(ValueError, match="Unknown column"):
                db.insert("node", {"unknown": "a"})
            with pytest.raises(ValueError, match="primary key"):
                db.delete("node", {})
            with pytest.raises(ValueError, match="Empty update"):
                db.update("node", {"id": "a"}, {})
            db.insert("node", {"id": "kept"})
    before = path.read_bytes()
    with pytest.raises(FileExistsError):
        with create_database(compiled(), path):
            pass
    assert path.read_bytes() == before


def test_read_boundary_rejects_corrupt_storage_and_disabled_enforcement() -> None:
    with create_database(compiled()) as db:
        with db.transaction():
            db.insert("numbers", number_row())
        db._connection.execute("PRAGMA ignore_check_constraints=ON")
        with pytest.raises(
            sqlite3.IntegrityError, match="CHECK constraint enforcement"
        ):
            db.verify()
        db._connection.execute("UPDATE numbers SET flag=2")
        with pytest.raises(ValueError, match="boolean"):
            db.rows("numbers")
        db._connection.execute("PRAGMA ignore_check_constraints=OFF")
        with pytest.raises(sqlite3.IntegrityError, match="Integrity"):
            db.verify()
        db._connection.execute("PRAGMA foreign_keys=OFF")
        with pytest.raises(sqlite3.IntegrityError, match="disabled"):
            db.verify()


@pytest.mark.parametrize(
    "raw", [None, [(1.5,)], [(b"bytes",)], [["list row"]], [(True,)]]
)
def test_raw_sqlite_results_do_not_escape_unchecked(raw: object) -> None:
    with pytest.raises(TypeError):
        _raw_rows(raw)


@pytest.mark.parametrize("kind", list(BOUNDS))
def test_read_boundary_does_not_coerce_string_integers(kind: Kind) -> None:
    with pytest.raises(TypeError):
        decode(Column("n", kind), "1", Rules({}))


def test_registry_rejects_unknown_unimplemented_and_required_future_targets() -> None:
    for requested in (("unknown",), ("images",)):
        with pytest.raises(ValueError, match="unimplemented"):
            compile_schema(registry(), requested, JSON_SCHEMAS)
    source = registry()
    item = next(table for table in source.tables if table.name == "item")
    required = replace(
        item,
        columns=tuple(
            replace(col, nullable=False) if col.name == "art_id" else col
            for col in item.columns
        ),
    )
    source = replace(
        source,
        tables=tuple(
            required if table.name == "item" else table for table in source.tables
        ),
    )
    with pytest.raises(ValueError, match="unimplemented"):
        compile_schema(source, ("catalog",), JSON_SCHEMAS)


def test_explicit_capability_dependencies_close_transitively() -> None:
    source = registry()
    source = replace(
        source,
        capabilities=tuple(
            replace(cap, requires=("numeric",)) if cap.name == "catalog" else cap
            for cap in source.capabilities
        ),
    )
    assert {t.name for t in source.resolve(("catalog",))} == {
        "item",
        "node",
        "vocabulary",
        "numbers",
    }


@pytest.mark.parametrize(
    "column",
    [
        Column("bad;name", Kind.ID),
        Column("id", Kind.JSON),
        Column("id", Kind.ID, nullable=True),
        Column("id", Kind.INT, fixed="x"),
        Column("id", Kind.TEXT, fixed="x", choices=("y",)),
    ],
)
def test_invalid_column_declarations_are_rejected(column: Column) -> None:
    source = Registry(
        (Table("sample", (column,), (column.name,)),),
        (Capability("core", ("sample",)),),
    )
    with pytest.raises(ValueError, match=r"Invalid|JSON|Primary|String|Fixed"):
        compile_schema(source, ("core",))


def test_fk_targets_require_real_unique_keys_and_matching_arity() -> None:
    parent = Table(
        "parent",
        (Column("id", Kind.ID), Column("code", Kind.ID)),
        ("id",),
        unique=(Unique(("code",), where="code != ''"),),
    )
    for target in (("code",), ("id", "code"), ("missing",)):
        child = Table(
            "child",
            (Column("id", Kind.ID),),
            ("id",),
            foreign_keys=(ForeignKey(("id",), "parent", target),),
        )
        source = Registry((parent, child), (Capability("core", ("parent", "child")),))
        with pytest.raises(ValueError, match=r"unique key|arity"):
            compile_schema(source, ("core",))


@pytest.mark.parametrize(
    "schema",
    [
        {"$ref": "https://example.invalid/schema"},
        {"type": "string", "format": "date"},
        {"$id": "https://example.invalid/root", "type": "object"},
        {"type": "not_a_type"},
    ],
)
def test_json_schema_contract_rejects_external_or_implicit_validation(
    schema: JsonValue,
) -> None:
    with pytest.raises((ValueError, SchemaError)):
        compile_schema(registry(), ("numeric",), JSON_SCHEMAS | {"document": schema})


def test_missing_json_schema_fails_before_connecting() -> None:
    with pytest.raises(ValueError, match="Undefined JSON schema"):
        compile_schema(registry(), ("numeric",))


def test_required_capability_cycle_is_closed_and_imported_atomically() -> None:
    alpha = Table(
        "alpha",
        (Column("id", Kind.ID), Column("beta_id", Kind.ID)),
        ("id",),
        foreign_keys=(ForeignKey(("beta_id",), "beta", ("id",)),),
    )
    beta = Table(
        "beta",
        (Column("id", Kind.ID), Column("alpha_id", Kind.ID)),
        ("id",),
        foreign_keys=(ForeignKey(("alpha_id",), "alpha", ("id",)),),
    )
    source = Registry(
        (alpha, beta),
        (Capability("first", ("alpha",)), Capability("second", ("beta",))),
    )
    schema = compile_schema(source, ("first",))
    assert {table.name for table in schema.tables} == {"alpha", "beta"}
    with create_database(schema) as db:
        with db.transaction():
            db.insert("alpha", {"id": "a", "beta_id": "b"})
            db.insert("beta", {"id": "b", "alpha_id": "a"})
        db.verify()


def test_unconditional_unique_keys_are_valid_fk_targets() -> None:
    parent = Table(
        "parent",
        (Column("id", Kind.ID), Column("region", Kind.ID), Column("name", Kind.ID)),
        ("id",),
        unique=(Unique(("region", "name")),),
    )
    child = Table(
        "child",
        (Column("id", Kind.ID), Column("region", Kind.ID), Column("name", Kind.ID)),
        ("id",),
        foreign_keys=(ForeignKey(("region", "name"), "parent", ("region", "name")),),
    )
    source = Registry((parent, child), (Capability("core", ("parent", "child")),))
    with create_database(compile_schema(source, ("core",))) as db:
        with db.transaction():
            db.insert("parent", {"id": "a", "region": "jp", "name": "example"})
            db.insert("child", {"id": "b", "region": "jp", "name": "example"})
        with pytest.raises(sqlite3.IntegrityError, match="UNIQUE"):
            with db.transaction():
                db.insert(
                    "parent", {"id": "duplicate", "region": "jp", "name": "example"}
                )
        with pytest.raises(sqlite3.IntegrityError, match="Foreign key"):
            with db.transaction():
                db.update("child", {"id": "b"}, {"region": "en"})


def test_local_json_schema_references_are_supported() -> None:
    schema: JsonValue = {
        "$defs": {"count": {"type": "integer", "minimum": 0}},
        "type": "object",
        "properties": {"n": {"$ref": "#/$defs/count"}},
        "required": ["n"],
        "additionalProperties": False,
    }
    compiled_schema = compile_schema(
        registry(), ("numeric",), JSON_SCHEMAS | {"document": schema}
    )
    with create_database(compiled_schema) as db:
        with db.transaction():
            db.insert("numbers", number_row())
        with pytest.raises(ValidationError):
            with db.transaction():
                db.update("numbers", {"id": "n"}, {"document": Json({"n": "1"})})


@pytest.mark.parametrize(
    "sql",
    [
        "INSERT INTO node (next_id) VALUES (NULL)",
        "INSERT INTO vocabulary VALUES ('label', 'Uppercase')",
        "INSERT INTO vocabulary VALUES ('label', 'valid')",
        "UPDATE item SET state='unknown'",
        "UPDATE item SET id=NULL",
    ],
)
def test_sql_primary_keys_patterns_and_enums(sql: str) -> None:
    with create_database(compiled()) as db:
        with db.transaction():
            db.insert("node", {"id": "node:a"})
            db.insert("vocabulary", {"kind": "label", "code": "valid"})
            db.insert("item", item_row())
        with pytest.raises(sqlite3.IntegrityError):
            with db.transaction():
                db._connection.execute(sql)


def test_registry_rejects_ambiguous_or_incomplete_ownership() -> None:
    table = Table("sample", (Column("id", Kind.ID),), ("id",))
    for source in (
        Registry((table, table), (Capability("core", ("sample",)),)),
        Registry(
            (table,),
            (Capability("first", ("sample",)), Capability("second", ("sample",))),
        ),
        Registry((), (Capability("core", ("sample",)),)),
        Registry((table,), (Capability("core", ("sample",), requires=("unknown",)),)),
    ):
        with pytest.raises(ValueError, match=r"Duplicate|multiple|lacks|Unknown"):
            compile_schema(source, ("core",))


def test_schema_keywords_inside_literal_data_are_not_schema_references() -> None:
    literal_data: JsonValue = {
        "$ref": "https://example.invalid/data",
        "format": "label",
    }
    schema = compile_schema(
        registry(), ("numeric",), JSON_SCHEMAS | {"anything": {"const": literal_data}}
    )
    with create_database(schema) as db:
        with db.transaction():
            db.insert("numbers", number_row() | {"payload": Json(literal_data)})
        assert db.rows("numbers")[0].values["payload"] == Json(literal_data)


@pytest.mark.parametrize(
    "schema",
    [
        {"properties": {"n": {"format": "date"}}},
        {"allOf": [{"$ref": "https://example.invalid/schema"}]},
        {"items": {"$id": "https://example.invalid/schema"}},
    ],
)
def test_nested_schemas_cannot_enable_external_or_format_validation(
    schema: JsonValue,
) -> None:
    with pytest.raises(ValueError, match=r"local JSON|External JSON"):
        compile_schema(registry(), ("numeric",), JSON_SCHEMAS | {"anything": schema})
