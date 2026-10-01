"""Execute production T0 constraints against wholly synthetic connected rows."""

import sqlite3
from typing import TYPE_CHECKING

import pytest
from jsonschema import ValidationError
from pydantic import JsonValue

from sve_carddb.build_db import Database, Json, Value
from sve_carddb.build_db.database import install_functions, open_database
from sve_carddb.build_db.domains import DATE as DATE_PATTERN
from sve_carddb.build_db.domains import INSTANT as INSTANT_PATTERN
from sve_carddb.build_db.model import identifier
from sve_carddb.build_db.t0 import TABLES, compile_t0
from sve_carddb.build_db.t0_json import symbol_valid
from sve_carddb.build_db.validation import Rules
from sve_carddb.snapshot.contract import validate
from sve_carddb.snapshot.values import canonical

from .build_db_fixtures import DATE, HASH, INSTANT, seed

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build_db import CompiledSchema

    from .database_fixtures import DatabaseTemplate

# ruff: file-ignore[hardcoded-sql-expression] -- transactions and fixture-authored SQL test native constraint enforcement


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_t0()


@pytest.fixture
def db(t0_database_template: DatabaseTemplate) -> Iterator[Database]:
    with t0_database_template.copy() as database:
        yield database


def _update(db: Database, table: str, values: dict[str, Value]) -> None:
    declaration = next(t for t in TABLES if t.name == table)
    existing = db.rows(table)[0].values
    key = {name: existing[name] for name in declaration.primary_key}
    with db.transaction():
        db.update(table, key, values)


@pytest.mark.parametrize(
    "kind",
    ["booster", "promo", "deck", "collaboration", "special_pack", "special", "other"],
)
def test_product_family_accepts_supported_kinds(db: Database, kind: str) -> None:
    _update(db, "product_family", {"kind": kind})
    assert db.rows("product_family")[0].values["kind"] == kind


@pytest.mark.parametrize(
    ("table", "column"),
    [
        ("printing_face", "art_id"),
        ("rules_profile_revision", "cr_version_id"),
        ("card_engine_support", "dsl_id"),
        ("card_engine_support", "load_id"),
        ("text_symbol", "keyword_id"),
    ],
)
def test_disabled_nullable_foreign_keys_reject_values(
    db: Database, table: str, column: str
) -> None:
    assert db.rows(table)[0].values[column] is None
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        _update(db, table, {column: "unavailable"})
    assert db.rows(table)[0].values[column] is None


@pytest.mark.parametrize(
    ("column", "value"),
    [
        ("candidate_hash", HASH),
        ("dsl_version", "1.0"),
        ("engine_version", "1.0"),
        ("engine_build_hash", HASH),
        ("validation_policy_id", "policy"),
        ("status", "draft"),
        ("status", "reviewed"),
        ("status", "engine_passed"),
        ("status", "load_rejected"),
        ("automatic", True),
        ("validation_state", "fresh"),
        ("reason_codes", Json([])),
        ("reason_codes", Json(["z", "a"])),
    ],
)
def test_t0_without_engine_cannot_claim_support(
    db: Database, column: str, value: Value
) -> None:
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        _update(db, "card_engine_support", {column: value})


@pytest.mark.parametrize(
    ("table", "updates"),
    [
        ("source_record", {"kind": "official_page"}),
        ("decision", {"reviewed_by": None}),
        ("decision", {"reviewed_at": None}),
        ("decision", {"reviewed_by": " "}),
        ("decision", {"sample_ids": Json([])}),
        ("decision", {"scope": "batch"}),
        ("identity_change", {"new_card_id": "old_card"}),
        ("identity_change", {"kind": "reassign_printing"}),
        ("identity_change", {"printing_id": "printing"}),
        ("product", {"released_on": DATE}),
        ("product", {"date_precision": "day"}),
        ("product", {"date_precision": "month"}),
        ("product", {"date_precision": "year"}),
        ("printing", {"serial_total": 0}),
        ("printing", {"decklog_available": False}),
        ("printing", {"decklog_verification": "verified"}),
        ("printing", {"decklog_checked_on": DATE}),
        ("printing_product", {"first_available_on": DATE}),
        ("printing_product", {"first_available_precision": "day"}),
        ("printing_product", {"first_available_precision": "month"}),
        ("printing_product", {"first_available_raw": "2026"}),
        ("face_revision", {"cost": -1}),
        ("face_revision", {"attack": -1}),
        ("face_revision", {"defense": -1}),
        ("face_revision", {"effective_from": DATE, "effective_until": DATE}),
        ("face_current", {"basis": "reviewed_override"}),
        ("source_coverage", {"until_date": DATE}),
        ("source_coverage", {"as_of": "2026-09-28"}),
        ("rules_profile_revision", {"effective_until": DATE}),
        ("restriction", {"effective_until": DATE}),
        ("restriction", {"max_copies": None}),
        ("restriction", {"max_selected_groups": 1}),
        ("restriction", {"kind": "choice_group"}),
        ("restriction_coverage", {"until_date": DATE}),
        ("card_route", {"namespace": "provisional"}),
        ("card_route_alias", {"old_key": "０１"}),
        (
            "card_route_alias",
            {"namespace": "official", "old_key": "TEST-001%E2%93%88a"},
        ),
    ],
)
def test_row_checks_reject_invalid_combinations(
    db: Database, table: str, updates: dict[str, Value]
) -> None:
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        _update(db, table, updates)


@pytest.mark.parametrize(
    ("table", "updates"),
    [
        (
            "source_record",
            {
                "kind": "official_page",
                "url": "https://example.invalid/card",
                "fetched_at": INSTANT,
            },
        ),
        ("decision", {"state": "sampled"}),
        (
            "decision",
            {
                "scope": "batch",
                "membership_hash": HASH,
                "policy_id": "policy",
                "sample_ids": Json(["card"]),
            },
        ),
        ("identity_change", {"kind": "reassign_printing", "printing_id": "printing"}),
        ("product", {"date_precision": "day", "released_on": DATE}),
        ("product", {"date_precision": "month", "date_raw": "2026-09"}),
        ("printing", {"catalog_state": "unlisted", "decklog_available": False}),
        (
            "printing",
            {
                "decklog_verification": "verified",
                "decklog_source_id": "source",
                "decklog_checked_on": DATE,
                "decklog_available": False,
            },
        ),
        (
            "printing_product",
            {"first_available_precision": "day", "first_available_on": DATE},
        ),
        ("printing_product", {"first_available_precision": "unknown"}),
        (
            "printing_product",
            {"first_available_precision": "year", "first_available_raw": "2026"},
        ),
        ("face_revision", {"cost": 0, "attack": 0, "defense": 9007199254740991}),
        ("face_current", {"basis": "reviewed_override", "decision_id": "decision"}),
        (
            "restriction",
            {"kind": "choice_group", "max_copies": None, "max_selected_groups": 1},
        ),
    ],
)
def test_row_checks_accept_legal_alternatives(
    db: Database, table: str, updates: dict[str, Value]
) -> None:
    _update(db, table, updates)
    assert all(db.rows(table)[0].values[key] == value for key, value in updates.items())


@pytest.mark.parametrize(
    ("table", "column", "kind"),
    [
        ("printing", "rarity_code", "rarity"),
        ("printing_face", "frame_code", "frame"),
        ("face_revision", "class_code", "class"),
        ("face_revision", "type_code", "type"),
        ("face_trait", "trait_code", "trait"),
        ("face_title", "title_code", "title"),
        ("face_special_kind", "special_kind_code", "special_kind"),
    ],
)
def test_vocabulary_kind_is_fixed_even_for_nullable_codes(
    db: Database, table: str, column: str, kind: str
) -> None:
    with db.transaction():
        db.insert(
            "vocabulary",
            {
                "kind": "unrelated",
                "code": "wrong",
                "label_unit_id": "text",
                "active": True,
            },
        )
    with pytest.raises(sqlite3.IntegrityError, match=r"(?i)foreign key"):
        _update(db, table, {column: "wrong"})
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        with db.transaction():
            db._connection.execute(
                f"UPDATE {identifier(table)} SET {identifier(kind + '_kind')} = ?",
                ("unrelated",),
            )
    _update(db, table, {column: "follower" if kind == "type" else "synthetic"})


@pytest.mark.parametrize(
    ("table", "updates"),
    [
        ("printing_face", {"card_id": "old_card"}),
        ("face_current", {"region": "en"}),
        ("face_rules_name", {"region": "en"}),
        ("default_printing_override", {"region": "en"}),
        ("default_printing_override", {"card_id": "old_card"}),
    ],
)
def test_composite_foreign_keys_enforce_ownership(
    db: Database, table: str, updates: dict[str, Value]
) -> None:
    with pytest.raises(sqlite3.IntegrityError, match=r"(?i)foreign key"):
        _update(db, table, updates)


def test_face_observation_and_sections_cannot_use_another_face(db: Database) -> None:
    with db.transaction():
        db.insert(
            "face",
            {"id": "other_face", "card_id": "old_card", "ordinal": 0, "side": "front"},
        )
        revision = dict(db.rows("face_revision")[0].values)
        db.insert(
            "face_revision",
            revision | {"id": "other_revision", "face_id": "other_face"},
        )
    with pytest.raises(sqlite3.IntegrityError, match=r"(?i)foreign key"):
        _update(db, "printing_face_observation", {"revision_id": "other_revision"})
    with pytest.raises(sqlite3.IntegrityError, match=r"(?i)foreign key"):
        _update(db, "printing_text_section", {"face_id": "other_face"})
    with pytest.raises(sqlite3.IntegrityError, match=r"(?i)foreign key"):
        _update(db, "face_revision", {"supersedes_id": "revision"})


def test_partial_unique_allows_provisional_collisions_and_official_variants(
    db: Database,
) -> None:
    original = dict(db.rows("printing")[0].values)
    with db.transaction():
        db.insert(
            "printing",
            original | {"id": "provisional_a", "card_no_state": "provisional"},
        )
        db.insert(
            "printing",
            original | {"id": "provisional_b", "card_no_state": "provisional"},
        )
        db.insert(
            "printing", original | {"id": "alternate", "variant_key": "alternate"}
        )
        db.insert("printing", original | {"id": "en_printing", "region": "en"})
    with pytest.raises(sqlite3.IntegrityError, match="UNIQUE"):
        with db.transaction():
            db.insert("printing", original | {"id": "duplicate"})
    with pytest.raises(sqlite3.IntegrityError, match="UNIQUE"):
        with db.transaction():
            db.update(
                "printing", {"id": "provisional_a"}, {"card_no_state": "official"}
            )


def test_route_override_requires_official_namespace(db: Database) -> None:
    with db.transaction():
        db.insert(
            "card_route",
            {
                "namespace": "provisional",
                "route_key": "20002",
                "printing_id": "printing",
            },
        )
    with pytest.raises(sqlite3.IntegrityError, match=r"(?i)foreign key"):
        _update(db, "route_override", {"route_key": "20002"})
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        with db.transaction():
            db._connection.execute("UPDATE route_override SET namespace='provisional'")


@pytest.mark.parametrize("table", [table.name for table in TABLES])
def test_primary_keys_are_enforced(db: Database, table: str) -> None:
    with pytest.raises(sqlite3.IntegrityError, match="UNIQUE"):
        with db.transaction():
            db.insert(table, dict(db.rows(table)[0].values))


@pytest.mark.parametrize(
    ("table", "column"),
    [
        (table.name, column.name)
        for table in TABLES
        for column in table.columns
        if not column.nullable
    ],
)
def test_required_columns_reject_sql_null(
    db: Database, table: str, column: str
) -> None:
    with pytest.raises(sqlite3.IntegrityError):
        with db.transaction():
            db._connection.execute(
                f"UPDATE {identifier(table)} SET {identifier(column)} = NULL"
            )


@pytest.mark.parametrize(
    ("table", "column"),
    [
        (table.name, column.name)
        for table in TABLES
        for column in table.columns
        if column.choices
    ],
)
def test_enum_columns_reject_unknown_values(
    db: Database, table: str, column: str
) -> None:
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        with db.transaction():
            db._connection.execute(
                f"UPDATE {identifier(table)} SET {identifier(column)} = ?", ("invalid",)
            )


@pytest.mark.parametrize("value", [0, 4294967295])
def test_uint32_endpoints(db: Database, value: int) -> None:
    _update(db, "card_int_id", {"int_id": value})


@pytest.mark.parametrize("value", [-1, 4294967296])
def test_uint32_outside_domain_is_rejected_by_sql(db: Database, value: int) -> None:
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        with db.transaction():
            db._connection.execute("UPDATE card_int_id SET int_id=?", (value,))


@pytest.mark.parametrize(
    ("table", "field", "value"),
    [
        ("face", "ordinal", -1),
        ("face", "ordinal", 9007199254740992),
        ("face_revision", "cost", 9007199254740992),
        ("face_revision", "cost", -9007199254740992),
        ("card_int_id", "int_id", 4294967296),
    ],
)
def test_read_boundary_rejects_corrupt_integer_range(
    db: Database, table: str, field: str, value: int
) -> None:
    db._connection.execute("PRAGMA ignore_check_constraints=ON")
    db._connection.execute(
        f"UPDATE {identifier(table)} SET {identifier(field)}=?", (value,)
    )
    with pytest.raises(ValueError, match="Integer outside domain"):
        db.rows(table)


@pytest.mark.parametrize("value", [2, -1, "not_boolean"])
def test_bool_sql_constraint(db: Database, value: int | str) -> None:
    with pytest.raises(sqlite3.IntegrityError):
        with db.transaction():
            db._connection.execute("UPDATE vocabulary SET active=?", (value,))


def test_json_null_is_not_sql_null(db: Database) -> None:
    with pytest.raises(ValidationError):
        _update(db, "decision", {"sample_ids": Json(None)})
    assert db.rows("decision")[0].values["sample_ids"] is None


@pytest.mark.parametrize(
    ("table", "field", "data"),
    [
        ("decision", "sample_ids", '["x", "x"]'),
        ("decision", "sample_ids", '[""]'),
        ("decision", "sample_ids", "[null]"),
        ("language", "fallback_order", '["ja", "ja"]'),
        ("language", "fallback_order", '["en\\n"]'),
        ("language", "fallback_order", "{}"),
        ("card_engine_support", "reason_codes", '["missing_dsl", "missing_dsl"]'),
        ("card_engine_support", "reason_codes", '["UPPER"]'),
        ("card_engine_support", "reason_codes", '["code\\n"]'),
        ("text_symbol", "parameter_schema", "{}"),
        ("text_symbol", "parameter_schema", '{"parameters":null}'),
        ("text_symbol", "parameter_schema", '{"parameters":[],"extra":1}'),
        ("text_symbol", "parameter_schema", '{"parameters":[],"parameters":[]}'),
        ("text_symbol", "spellings", '[{"lang":"ja"}]'),
        ("text_symbol", "spellings", "[null]"),
        ("text_symbol", "localizations", '[{"lang":"ja","name":"x","tooltip":"x"}]'),
        (
            "text_symbol",
            "localizations",
            '[{"lang":"ja","name":"x","tooltip":"x","copy_pattern":"x","regex":".*"}]',
        ),
        (
            "text_symbol",
            "localizations",
            '[{"lang":"ja","name":"x","tooltip":null,"copy_pattern":"x"}]',
        ),
    ],
)
def test_six_json_shapes_reject_malformed_values_at_sql_boundary(
    db: Database, table: str, field: str, data: str
) -> None:
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        with db.transaction():
            db._connection.execute(
                f"UPDATE {identifier(table)} SET {identifier(field)}=?", (data,)
            )


@pytest.mark.parametrize("field", ["parameter_schema", "spellings", "localizations"])
@pytest.mark.parametrize(
    "data", ["{invalid}", "null", '"scalar"', "true", "1.1", '"\\ud800"']
)
def test_symbol_json_never_accepts_untyped_payload(
    db: Database, field: str, data: str
) -> None:
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        with db.transaction():
            db._connection.execute(
                f"UPDATE text_symbol SET {identifier(field)}=?", (data,)
            )


@pytest.mark.parametrize(
    "parameters",
    [
        [{"name": "value", "uint": None, "variables": []}],
        [{"name": "value", "uint": None, "variables": ["Y"]}],
        [{"name": "value", "uint": None, "variables": ["X", "X"]}],
        [{"name": "value", "uint": {"minimum": 2, "maximum": 1}, "variables": []}],
        [{"name": "value", "uint": {"minimum": -1, "maximum": 1}, "variables": []}],
        [
            {
                "name": "value",
                "uint": {"minimum": 0, "maximum": 9007199254740992},
                "variables": [],
            }
        ],
        [{"name": "value", "uint": {"minimum": False, "maximum": 1}, "variables": []}],
        [{"name": "value", "uint": {"minimum": 0.5, "maximum": 1}, "variables": []}],
        [{"name": "value", "uint": None}],
        [
            {"name": "a", "uint": None, "variables": ["X"]},
            {"name": "a", "uint": None, "variables": ["X"]},
        ],
        [
            {"name": "z", "uint": None, "variables": ["X"]},
            {"name": "a", "uint": None, "variables": ["X"]},
        ],
    ],
)
def test_invalid_parameter_declarations(
    db: Database, parameters: list[JsonValue]
) -> None:
    with pytest.raises((ValueError, ValidationError, sqlite3.IntegrityError)):
        _update(
            db, "text_symbol", {"parameter_schema": Json({"parameters": parameters})}
        )


@pytest.mark.parametrize(
    ("uint", "variables", "kind"),
    [
        (None, ["X"], "variable"),
        ({"minimum": 0, "maximum": 9007199254740991}, [], "uint"),
        ({"minimum": 0, "maximum": 9}, ["X"], "variable"),
    ],
)
def test_parameter_spelling_domain_and_public_shape(
    db: Database, uint: JsonValue, variables: list[JsonValue], kind: str
) -> None:
    declaration: JsonValue = {
        "parameters": [{"name": "value", "uint": uint, "variables": variables}]
    }
    spelling: dict[str, JsonValue] = {
        "lang": "ja",
        "literal_prefix": "[",
        "literal_suffix": "]",
        "parameter_name": "value",
        "parse_kind": kind,
    }
    _update(
        db,
        "text_symbol",
        {"parameter_schema": Json(declaration), "spellings": Json([spelling])},
    )
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        _update(
            db,
            "text_symbol",
            {"spellings": Json([spelling | {"parameter_name": "missing"}])},
        )
    disabled = "uint" if uint is None else "variable"
    if uint is None or not variables:
        with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
            _update(
                db,
                "text_symbol",
                {"spellings": Json([spelling | {"parse_kind": disabled}])},
            )


def test_batch_decision_samples_and_all_json_positive_shapes(db: Database) -> None:
    _update(
        db,
        "decision",
        {
            "state": "sampled",
            "scope": "batch",
            "membership_hash": HASH,
            "policy_id": "policy",
            "sample_ids": Json(["record:b", "record:a"]),
        },
    )
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        _update(db, "decision", {"sample_ids": Json([])})
    _update(db, "language", {"fallback_order": Json(["en", "zh-Hant"])})
    _update(
        db,
        "card_engine_support",
        {"reason_codes": Json(["engine_unassigned", "missing_dsl"])},
    )
    _update(
        db,
        "text_symbol",
        {
            "parameter_schema": Json({"parameters": []}),
            "spellings": Json([]),
            "localizations": Json([]),
        },
    )


@pytest.mark.parametrize(
    "table",
    [
        "source_record",
        "decision",
        "card_int_id",
        "face_revision",
        "rules_profile_revision",
    ],
)
def test_ascii_dates_and_instants_match_public_validation(
    db: Database, table: str
) -> None:
    declaration = next(t for t in TABLES if t.name == table)
    column = next(
        c for c in declaration.columns if c.pattern in {DATE_PATTERN, INSTANT_PATTERN}
    )
    public_type = "Date" if column.pattern == DATE_PATTERN else "Instant"
    good = DATE if public_type == "Date" else INSTANT
    _update(db, table, {column.name: good})
    validate(public_type, good)
    for value in (good + "\n", good.replace("2", "２"), good.replace("09-29", "02-30")):
        with pytest.raises(ValueError, match="pattern"):
            _update(db, table, {column.name: value})
        with pytest.raises(ValidationError):
            validate(public_type, value)


def test_scratch_database_executes_complete_ddl(schema: CompiledSchema) -> None:
    connection = sqlite3.connect(":memory:", isolation_level=None)
    try:
        install_functions(connection, schema)
        connection.execute("PRAGMA foreign_keys=ON")
        connection.executescript(schema.sql)
        database = Database(connection, schema, Rules(dict(schema.json_schemas)))
        seed(database)
        database.verify()
        assert len(database.rows("language")) == 1
    finally:
        connection.close()


@pytest.mark.parametrize(
    ("table", "column"),
    [
        (
            table.name,
            next(
                c
                for c in key.columns
                if table.column(c).fixed is None and not table.column(c).choices
            ),
        )
        for table in TABLES
        for key in table.foreign_keys
        if key.table in {t.name for t in TABLES}
    ],
)
def test_all_enabled_foreign_keys_reject_missing_targets(
    db: Database, table: str, column: str
) -> None:
    updates: dict[str, Value] = {column: "xx"}
    if (table, column) == ("identity_change", "printing_id"):
        updates["kind"] = "reassign_printing"
    with pytest.raises(sqlite3.IntegrityError, match=r"(?i)foreign key"):
        _update(db, table, updates)


@pytest.mark.parametrize(
    ("table", "pk"),
    [
        (table.name, next(pk for pk in table.primary_key if pk not in key.columns))
        for table in TABLES
        for key in table.unique
        if any(pk not in key.columns for pk in table.primary_key)
    ],
)
def test_nonredundant_unique_keys_reject_new_primary_keys(
    db: Database, table: str, pk: str
) -> None:
    row = dict(db.rows(table)[0].values)
    value = row[pk]
    row[pk] = value + 1 if isinstance(value, int) else str(value) + "_new"
    with pytest.raises(sqlite3.IntegrityError, match="UNIQUE"):
        with db.transaction():
            db.insert(table, row)


@pytest.mark.parametrize(
    "card_id", ["card", "old_card"], ids=["face_card_fk", "printing_card_fk"]
)
def test_printing_face_card_foreign_keys_independently(
    db: Database, card_id: str
) -> None:
    with db.transaction():
        db.insert(
            "face",
            {"id": "other_face", "card_id": "old_card", "ordinal": 0, "side": "front"},
        )
    row = dict(db.rows("printing_face")[0].values)
    # The printing belongs to card, the face to old_card; exactly one compound FK fails.
    with pytest.raises(sqlite3.IntegrityError, match=r"(?i)foreign key"):
        with db.transaction():
            db.insert(
                "printing_face", row | {"face_id": "other_face", "card_id": card_id}
            )


@pytest.mark.parametrize("table", ["search_alias", "text_unit"])
def test_language_foreign_keys_require_registration(db: Database, table: str) -> None:
    # fr is lexically valid, so only the missing registry row can reject it.
    with pytest.raises(sqlite3.IntegrityError, match=r"(?i)foreign key"):
        _update(db, table, {"lang": "fr"})
    with db.transaction():
        db.insert(
            "language",
            {
                "code": "fr",
                "fallback_order": Json([]),
                "display_name": "Synthetic French",
            },
        )
    _update(db, table, {"lang": "fr"})
    assert db.rows(table)[0].values["lang"] == "fr"


def test_literal_spelling_requires_null_parameter_at_both_boundaries(
    db: Database, schema: CompiledSchema
) -> None:
    parameters: JsonValue = {
        "parameters": [{"name": "value", "uint": None, "variables": ["X"]}]
    }
    spelling: dict[str, JsonValue] = {
        "lang": "ja",
        "literal_prefix": "{Q}",
        "literal_suffix": "",
        "parameter_name": None,
        "parse_kind": "literal",
    }
    _update(
        db,
        "text_symbol",
        {"parameter_schema": Json(parameters), "spellings": Json([spelling])},
    )
    invalid: JsonValue = [spelling | {"parameter_name": "value"}]
    rules = Rules(dict(schema.json_schemas))
    with pytest.raises(ValidationError):
        rules.validate("text_symbol_spellings", invalid)
    assert (
        symbol_valid(canonical(parameters).decode(), canonical(invalid).decode()) == 0
    )
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        with db.transaction():
            db._connection.execute(
                "UPDATE text_symbol SET spellings=?", (canonical(invalid).decode(),)
            )


@pytest.mark.parametrize(
    ("table", "column", "valid", "invalid"),
    [
        (
            "source_record",
            "sha256",
            "sha256:" + "abcdef0123456789" * 4,
            "sha256:" + "ABCDEF0123456789" * 4,
        ),
        ("vocabulary", "kind", "synthetic_kind", "Synthetic_kind"),
        ("vocabulary", "code", "synthetic_code", "Synthetic_code"),
        ("vocabulary", "code", "synthetic_code", "synthetic_Code"),
    ],
)
def test_hash_and_vocabulary_codes_require_lowercase(
    db: Database, table: str, column: str, valid: str, invalid: str
) -> None:
    row = dict(db.rows(table)[0].values)
    if table == "vocabulary":
        row |= {"kind": "synthetic_kind", "code": "synthetic_code"}
    else:
        row["id"] = "lowercase_hash"
    row[column] = valid
    with db.transaction():
        db.insert(table, row)
    declaration = next(t for t in TABLES if t.name == table)
    key = {field: row[field] for field in declaration.primary_key}
    with pytest.raises(ValueError, match="pattern"):
        with db.transaction():
            db.update(table, key, {column: invalid})
    where = " AND ".join(f"{identifier(field)}=?" for field in key)
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        with db.transaction():
            db._connection.execute(
                f"UPDATE {identifier(table)} SET {identifier(column)}=? WHERE {where}",
                (invalid, *key.values()),
            )


def test_missing_dsl_reason_does_not_have_to_repeat_status(db: Database) -> None:
    _update(db, "card_engine_support", {"reason_codes": Json(["engine_unassigned"])})
    assert db.rows("card_engine_support")[0].values["status"] == "missing_dsl"
    with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
        _update(db, "card_engine_support", {"reason_codes": Json([])})


class TestT0Inventory:
    @pytest.fixture(scope="class")
    def db(
        self,
        tmp_path_factory: pytest.TempPathFactory,
        t0_database_template: DatabaseTemplate,
    ) -> Iterator[Database]:
        path = tmp_path_factory.mktemp("readonly-t0") / "build.sqlite"
        path.write_bytes(t0_database_template.content)
        with open_database(t0_database_template.schema, path) as database:
            yield database

    def test_all_tables_have_real_rows_and_deferred_references(
        self, db: Database
    ) -> None:
        assert all(db.rows(table.name) for table in TABLES)
        assert db.rows("printing")[0].values["card_no"] == "TEST-001Ⓢa"
        assert db.rows("printing_face")[0].values["art_id"] is None
        db.verify()

    def test_shared_database_rejects_writes(self, db: Database) -> None:
        before = db.rows("language")
        with (
            pytest.raises(sqlite3.OperationalError, match="readonly"),
            db.transaction(),
        ):
            db.update("language", {"code": "ja"}, {"display_name": "changed"})
        assert db.rows("language") == before
        assert not db._connection.in_transaction
        db.verify()
