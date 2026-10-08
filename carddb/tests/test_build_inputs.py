"""Independent raw metadata, per-use provenance and complete closure counterexamples."""

import json
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import (
    FilePin,
    InputRecord,
    SourceUse,
    input_record,
    insert_raw_sources,
)

from .registry_preview_fixtures import BUILD, observed
from .test_registry import card

if TYPE_CHECKING:
    from sve_carddb.build_inputs import Source


@pytest.fixture
def source() -> Source:
    return observed(card("TEST-001", "Synthetic card"), "jp").source


@pytest.mark.parametrize("reverse", [False, True])
def test_many_parsers_and_uses_share_one_raw_row(source: Source, reverse: bool) -> None:
    product = source.model_copy(update={"parser_version": "product-parser-v2"})
    uses = (
        SourceUse(source=source, usage="registry_observation", locator="jp:TEST-001"),
        SourceUse(
            source=product, usage="product_observation", locator="product block 0"
        ),
        SourceUse(
            source=product,
            usage="printing_product_observation",
            locator="product block 1",
        ),
    )
    with create_database(compile_build()) as db, db.transaction():
        sequence = reversed(uses) if reverse else iter(uses)
        for use in sequence:
            insert_raw_sources(db, (use.source,))
        record = input_record(BUILD, (*uses, uses[0]))
        [row] = db.rows("source_record")
        assert row.values == source.values()
        assert row.values["parser_version"] is None
        assert len(record.uses) == 3
        assert {use.source.parser_version for use in record.uses} == {
            "synthetic-v1",
            "product-parser-v2",
        }
        assert InputRecord.model_validate_json(record.content()) == record
        assert not db.rows("decision")
        assert "Rule." not in record.content().decode()


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("url", "https://example.invalid/other"),
        ("sha256", "sha256:" + "0" * 64),
        ("kind", "official_api"),
        ("raw_locator", "synthetic:raw/other"),
        ("fetched_at", "2026-09-30T00:00:00Z"),
        ("etag", "changed"),
        ("last_modified", "changed"),
    ],
)
@pytest.mark.parametrize("existing", [False, True])
def test_each_shared_metadata_field_conflicts_independently(
    source: Source, field: str, value: str, existing: bool
) -> None:
    other = source.model_copy(update={field: value})
    with create_database(compile_build()) as db:
        if existing:
            with db.transaction():
                insert_raw_sources(db, (source,))
        with (
            pytest.raises(ValueError, match="Conflicting raw source metadata"),
            db.transaction(),
        ):
            insert_raw_sources(db, (other,) if existing else (source, other))
        assert len(db.rows("source_record")) == int(existing)
        if existing:
            assert db.rows("source_record")[0].values == source.values()


@pytest.mark.parametrize(
    "case",
    [
        "missing_archive",
        "missing_parser",
        "missing_first_receipt",
        "duplicate_uses",
        "unsorted_uses",
        "unexpected_field",
    ],
)
def test_input_record_requires_all_provenance_fields(source: Source, case: str) -> None:
    use = SourceUse(source=source, usage="identity", locator="card")
    data = json.loads(input_record(BUILD, (use,)).content())
    if case == "missing_archive":
        del data["uses"][0]["source"]["archive"]
    elif case == "missing_parser":
        del data["uses"][0]["source"]["parser_version"]
    elif case == "missing_first_receipt":
        del data["uses"][0]["source"]["archive"]["first_receipt_id"]
    elif case == "duplicate_uses":
        data["uses"] *= 2
    elif case == "unsorted_uses":
        data["uses"].append(
            use.model_copy(update={"locator": "aaa"}).model_dump(mode="json")
        )
    else:
        data["unexpected"] = "rejected"
    with pytest.raises(ValidationError):
        InputRecord.model_validate_json(json.dumps(data))


@pytest.mark.parametrize(
    "name",
    [
        "/private/dependencies.lock",
        "../dependencies.lock",
        "dir/../dependencies.lock",
        "dir//dependencies.lock",
        "dir/dependencies.lock/",
    ],
)
def test_dependency_names_cannot_use_machine_paths_or_noncanonical_aliases(
    name: str,
) -> None:
    with pytest.raises(ValueError, match="canonical relative path"):
        FilePin(name=name, sha256="sha256:" + "a" * 64)
