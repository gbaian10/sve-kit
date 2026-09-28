"""Compare code-authored declarations to the authoritative Markdown inventory."""

import re
from pathlib import Path

import pytest

from sve_carddb.build_db import ForeignKey, Kind, Table
from sve_carddb.build_db.domains import DATE, HASH, INSTANT, LANG
from sve_carddb.build_db.t0 import TABLES
from sve_carddb.build_db.t1 import TABLES as T1_TABLES

DOCS = Path(__file__).resolve().parents[2] / "docs" / "schema"


def _rows() -> dict[str, str]:
    return dict(
        re.findall(
            r"^\| `([a-z_]+)`\s*\| (.+)\|\s*$",
            (DOCS / "build-db.md").read_text(),
            re.MULTILINE,
        )
    )


def _fields(declaration: str) -> dict[str, tuple[str, bool]]:
    result: dict[str, tuple[str, bool]] = {}
    pending: list[str] = []
    for fragment in declaration.replace(r"\|", "|").split(","):
        field = fragment.strip().removesuffix(" UNIQUE").removesuffix(" PK")
        if "→" in field:
            name, target = field.split("→")
            result |= dict.fromkeys(pending, ("ID", False))
            pending = []
            result[name] = ("ID", target.endswith("?"))
        elif ":" in field:
            name, kind = field.split(":")
            result[name] = (kind.rstrip("?"), kind.endswith("?"))
        else:
            pending.append(field)
    assert not pending
    return result


def test_t0_table_set_exactly_matches_tiers() -> None:
    expected = re.findall(
        r"^\| `([a-z_]+)`\s*\| T0\s*\|",
        (DOCS / "implementation-tiers.md").read_text(),
        re.MULTILINE,
    )
    assert len(expected) == len(set(expected)) == 40
    assert {table.name for table in TABLES} == set(expected)


@pytest.mark.parametrize("table", [*TABLES, *T1_TABLES], ids=lambda table: table.name)
def test_logical_columns_domains_nullability_and_enums(table: object) -> None:
    assert isinstance(table, Table)
    declaration = re.findall(r"`([^`]+)`", _rows()[table.name])[0]
    documented = _fields(declaration)
    logical = {c.name: c for c in table.columns if c.fixed is None}
    assert logical.keys() == documented.keys()
    domains = {
        "ID": Kind.ID,
        "Code": Kind.ID,
        "Lang": Kind.ID,
        "Text": Kind.TEXT,
        "Date": Kind.TEXT,
        "Instant": Kind.TEXT,
        "Hash": Kind.TEXT,
        "Region": Kind.TEXT,
        "UInt": Kind.UINT,
        "Int": Kind.INT,
        "Bool": Kind.BOOL,
        "Json": Kind.JSON,
    }
    for name, (kind, nullable) in documented.items():
        column = logical[name]
        assert column.nullable == nullable, name
        expected = domains.get(kind, Kind.TEXT)
        if (table.name, name) == ("card_int_id", "int_id"):
            expected = Kind.UINT32
        assert column.kind == expected, name
        if "|" in kind:
            assert column.choices == tuple(kind.split("|")), name
        if kind == "Region":
            assert column.choices == ("jp", "en")


@pytest.mark.parametrize("table", [*TABLES, *T1_TABLES], ids=lambda table: table.name)
def test_documented_lexical_domains_have_patterns_on_every_column(table: Table) -> None:
    declaration = re.findall(r"`([^`]+)`", _rows()[table.name])[0]
    patterns = {"Date": DATE, "Instant": INSTANT, "Hash": HASH, "Lang": LANG}
    for name, (kind, _) in _fields(declaration).items():
        if kind in patterns:
            assert table.column(name).pattern == patterns[kind], (table.name, name)


@pytest.mark.parametrize("table", [*TABLES, *T1_TABLES], ids=lambda table: table.name)
def test_documented_primary_and_unique_keys(table: object) -> None:
    assert isinstance(table, Table)
    raw = _rows()[table.name]
    declaration = re.findall(r"`([^`]+)`", raw)[0]
    inline = re.findall(r"(?:^|,)\s*([a-z_]+)[^,]*? PK", declaration)
    expected_pk = (
        tuple(inline)
        if inline
        else tuple(re.findall(r"PK\(([^)]+)\)", raw)[0].split(","))
    )
    assert table.primary_key == expected_pk
    expected_uq = {
        tuple(fields.split(",")) for fields in re.findall(r"UQ\(([^)]+)\)", raw)
    }
    expected_uq |= {
        (name,) for name in re.findall(r"(?:^|,)\s*([a-z_]+)[^,]*? UNIQUE", declaration)
    }
    assert expected_uq <= {unique.columns for unique in table.unique}
    for key in table.foreign_keys:
        if key.table == "vocabulary":
            fixed = table.column(key.columns[0])
            assert fixed.fixed is not None
            assert not fixed.nullable
            assert key.target == ("kind", "code")


@pytest.mark.parametrize("table", [*TABLES, *T1_TABLES], ids=lambda table: table.name)
def test_every_documented_foreign_column_has_target(table: Table) -> None:
    documented = _rows()[table.name]
    declaration = re.findall(r"`([^`]+)`", documented)[0]
    pending: list[str] = []
    for raw in declaration.split(","):
        field = raw.strip().removesuffix(" PK").removesuffix(" UNIQUE")
        if "→" in field:
            name, target = field.split("→")
            group = (*pending, name)
            assert any(
                key.table == target.rstrip("?") and set(group) <= set(key.columns)
                for key in table.foreign_keys
            ), (table.name, group)
            pending = []
        elif ":" not in field:
            pending.append(field)
    assert not pending
    # Fixed-namespace prose is covered by the dedicated assertions below.
    for columns, target, target_columns in re.findall(
        r"FK\(([a-z_]+(?:,[a-z_]+)*)\)→([a-z_]+)\(([a-z_]+(?:,[a-z_]+)*)\)",
        documented,
    ):
        expected = ForeignKey(
            tuple(columns.split(",")), target, tuple(target_columns.split(","))
        )
        assert expected in table.foreign_keys, (table.name, expected)


def test_fixed_vocabulary_and_route_keys_are_not_omitted() -> None:
    tables = {table.name: table for table in TABLES}
    required = {
        "printing": ("rarity",),
        "printing_face": ("frame",),
        "face_revision": ("class", "type"),
        "face_trait": ("trait",),
        "face_title": ("title",),
        "face_special_kind": ("special_kind",),
    }
    for name, kinds in required.items():
        for kind in kinds:
            table = tables[name]
            assert table.column(kind + "_kind").fixed == kind
            assert any(
                key.columns == (kind + "_kind", kind + "_code")
                and key.table == "vocabulary"
                and key.target == ("kind", "code")
                for key in table.foreign_keys
            )
    route = tables["route_override"]
    assert route.column("namespace").fixed == "official"
    assert any(
        key.columns == ("namespace", "route_key") and key.table == "card_route"
        for key in route.foreign_keys
    )
