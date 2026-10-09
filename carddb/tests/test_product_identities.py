"""Each product identity constraint has an independently invalid input."""

import copy
import json
import re
import shutil
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError
from ruamel.yaml.error import YAMLError

from sve_carddb.core.json import digest
from sve_carddb.core.yaml import MAX_BYTES
from sve_carddb.domains.products.identities import (
    _model,
    _shard,
    load_product_identities,
)
from sve_carddb.domains.products.identity_models import IdentityShard, ProductLink
from sve_carddb.domains.products.official import (
    parse_products as parse_verified_products,
)
from sve_carddb.domains.registry.storage import read_yaml
from sve_carddb.ingest.archive.source_archive import ArchiveError

from .product_fixtures import first_record, items, obj, write_yaml
from .product_identity_fixtures import (
    NAME,
    IdentityFixture,
    commit,
    html,
    identity_envelope,
    identity_record,
    install_identity,
)
from .product_identity_fixtures import identity_fixture as identity_fixture  # ruff: ignore[useless-import-alias] -- shared fixture

if TYPE_CHECKING:
    from sve_carddb.core.provenance import Source
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.products.official import ProductPage


def parse_products(raw: bytes, source: Source, region: Region) -> ProductPage:
    return parse_verified_products(
        raw, source.model_copy(update={"sha256": digest(raw)}), region
    )


def wire_error(area: str, field: str) -> str:
    overrides = {
        ("record", "filing_key"): "Product identity filing/path/region mismatch",
        ("record", "record_key"): "Invalid product identity fields",
        ("match", "product_url"): "records.0.data:value_error",
        ("match", "kind"): "records.0.data.match:union_tag_invalid",
    }
    prefixes = {
        "shard": "Invalid product identity fields: ",
        "record": "records.0.",
        "data": "records.0.data.",
        "match": "records.0.data.match.product_link.",
        "evidence": "records.0.evidence.0.",
    }
    return overrides.get((area, field), prefixes[area] + field + ":")


@pytest.mark.parametrize(
    "constraint",
    [
        "missing_directory",
        "extra_shard",
        "yml",
        "unexpected_path",
        "wrong_sequence",
        "symlink_shard",
        "symlink_directory",
        "oversized",
        "duplicate_key",
        "alias",
        "unknown_yaml_tag",
    ],
)
def test_inventory_yaml_and_revision_constraints(  # ruff: ignore[complex-structure] -- independent filesystem and YAML counterexamples
    identity_fixture: IdentityFixture, constraint: str
) -> None:
    fixture = identity_fixture
    shard_path = fixture.root / NAME
    if constraint == "missing_directory":
        shutil.rmtree(fixture.root / "products/identities")
    elif constraint in {"extra_shard", "yml"}:
        (
            shard_path.parent / ("002.yml" if constraint == "yml" else "002.yaml")
        ).write_bytes(shard_path.read_bytes())
    elif constraint == "unexpected_path":
        other = fixture.root / "products/identities/cn/001.yaml"
        other.parent.mkdir()
        other.write_bytes(shard_path.read_bytes())
    elif constraint == "wrong_sequence":
        shard_path.rename(shard_path.parent / "01.yaml")
    elif constraint == "symlink_shard":
        target = shard_path.with_suffix(".bak")
        shard_path.rename(target)
        shard_path.symlink_to(target)
    elif constraint == "symlink_directory":
        target = shard_path.parent.with_name("elsewhere")
        shard_path.parent.rename(target)
        shard_path.parent.symlink_to(target, target_is_directory=True)
    elif constraint == "oversized":
        shard_path.write_bytes(b"x" * MAX_BYTES)
    elif constraint == "duplicate_key":
        shard_path.write_bytes(
            shard_path.read_bytes() + b"kind: product_identity_shard\n"
        )
    elif constraint == "alias":
        shard_path.write_text(
            "format: &format 1\nkind: product_identity_shard\nrecords: *format\n"
        )
    elif constraint == "unknown_yaml_tag":
        shard_path.write_text(
            shard_path.read_text().replace("format: 1", "format: !custom 1")
        )
    else:
        shard_path.write_bytes(shard_path.read_bytes() + b"# Dirty physical bytes\n")
    with pytest.raises((ValueError, TypeError, FileNotFoundError, YAMLError)):
        fixture.load()


@pytest.mark.parametrize(
    "constraint",
    [
        "locator_noncanonical",
        "locator_unknown",
        "locator_bool",
        "locator_negative",
        "locator_missing_block",
        "wrong_match",
        "absent_version",
        "unconfigured",
        "corrupt_raw",
        "corrupt_descriptor",
        "corrupt_receipt",
        "unsealed",
    ],
)
def test_independent_evidence_constraints(
    identity_fixture: IdentityFixture, constraint: str
) -> None:
    fixture = identity_fixture
    shard = obj(read_yaml(fixture.root / NAME))
    record = first_record(shard)
    ref = obj(items(record["evidence"])[0])
    if constraint.startswith("locator"):
        ref["locator"] = {
            "locator_noncanonical": '{ "product_block_ordinal":0}',
            "locator_unknown": '{"extra":0,"product_block_ordinal":0}',
            "locator_bool": '{"product_block_ordinal":true}',
            "locator_negative": '{"product_block_ordinal":-1}',
            "locator_missing_block": '{"product_block_ordinal":9}',
        }[constraint]
    elif constraint == "wrong_match":
        obj(obj(record["data"])["match"])["expansion_code"] = "Other"
    elif constraint == "absent_version":
        ref["source_version_id"] = "src:v1:" + "0" * 64
    elif constraint.startswith("corrupt"):
        path = next(
            (
                fixture.store
                / {
                    "corrupt_raw": "raw",
                    "corrupt_descriptor": "descriptors",
                    "corrupt_receipt": "receipts",
                }[constraint]
            ).rglob("*.raw" if constraint == "corrupt_raw" else "*.json")
        )
        path.write_bytes(path.read_bytes() + b"Corrupt bytes")
    elif constraint == "unsealed":
        next((fixture.store / "batches").rglob("seal.json")).unlink()
    install_identity(fixture.root, shard)
    fixture.revision = commit(fixture.root)
    stores = {} if constraint == "unconfigured" else {"test-store": fixture.store}
    with pytest.raises((ValueError, ArchiveError)):
        load_product_identities(
            fixture.root,
            authored_revision=fixture.revision,
            catalog=fixture.catalog,
            stores=stores,
        )


def test_aliases_share_id_but_different_targets_conflict(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    page = fixture.pages[0]
    second = identity_record(page, match_index=-1)
    install_identity(fixture.root, identity_envelope([identity_record(page), second]))
    fixture.revision = commit(fixture.root)
    identities = fixture.load()
    assert identities.match("jp", page.blocks[0].matches) == "permanent-example"
    assert not identities.warnings
    obj(second["data"])["product_id"] = "different-id"
    install_identity(fixture.root, identity_envelope([identity_record(page), second]))
    fixture.revision = commit(fixture.root)
    with pytest.raises(ValueError, match="Conflicting permanent"):
        fixture.load().match("jp", page.blocks[0].matches)


def test_same_match_cannot_be_appended_even_to_same_id(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    install_identity(
        fixture.root,
        obj(read_yaml(fixture.root / NAME)),
        name="products/identities/jp/002.yaml",
    )
    fixture.revision = commit(fixture.root)
    with pytest.raises(ValueError, match="Duplicate global product identity match"):
        fixture.load()


def test_name_and_date_changes_keep_id_new_url_needs_alias(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    identities = fixture.load()
    page = fixture.pages[0]
    for body in (html(name="Changed name"), html(date="2025")):
        changed = parse_products(body, page.source, "jp")
        assert identities.match("jp", changed.blocks[0].matches) == "permanent-example"
    changed = parse_products(
        html(links=("/products/new/", "/cardlist/cardsearch?expansion=Test-A")),
        page.source,
        "jp",
    )
    assert identities.match("jp", changed.blocks[0].matches) is None
    assert identities.match("en", page.blocks[0].matches) is None


def test_read_only_snapshot_and_complete_authored_pins(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    before = {path: path.read_bytes() for path in fixture.root.rglob("*.yaml")}
    identities = fixture.load()
    identities.verify_context(fixture.context(identities))
    assert len(identities.records) == 1
    assert {use.usage for use in identities.source_uses()} == {
        "product_identity_evidence_closure",
        "official_product_identity",
    }
    assert (
        next(iter(identities.records.values())).data.product_id == "permanent-example"
    )
    assert next(iter(identities.records.values())).data.match == ProductLink(
        kind="product_link",
        product_url="https://shadowverse-evolve.com/products/synthetic/",
        expansion_code="Test-A",
    )
    assert before == {path: path.read_bytes() for path in before}


def test_same_expansion_different_ids_warns_but_alias_same_id_does_not(
    identity_fixture: IdentityFixture,
) -> None:
    from .product_identity_fixtures import add_page  # ruff: ignore[import-outside-top-level] -- keep the independently sealed warning case explicit

    fixture = identity_fixture
    page = add_page(
        fixture,
        html(
            number="TEST-002",
            links=("/products/second/", "/cardlist/cardsearch?expansion=Test-A"),
        ),
    )
    alias = identity_record(page, product_id="second-id")
    install_identity(
        fixture.root, identity_envelope([alias]), name="products/identities/jp/002.yaml"
    )
    fixture.revision = commit(fixture.root)
    identities = fixture.load()
    assert len(identities.warnings) == 1
    warning = obj(identities.warnings[0])
    assert warning["region"] == "jp"
    assert warning["expansion_code"] == "Test-A"
    assert {
        obj(obj(row)["data"])["product_id"] for row in items(warning["records"])
    } == {"permanent-example", "second-id"}
    obj(alias["data"])["product_id"] = "permanent-example"
    install_identity(
        fixture.root, identity_envelope([alias]), name="products/identities/jp/002.yaml"
    )
    fixture.revision = commit(fixture.root)
    assert not fixture.load().warnings
    assert fixture.load().match("jp", page.blocks[0].matches) == "permanent-example"


def test_same_id_cannot_cross_regions(identity_fixture: IdentityFixture) -> None:
    from .product_identity_fixtures import add_page  # ruff: ignore[import-outside-top-level] -- independently sealed EN region counterexample

    fixture = identity_fixture
    page = add_page(
        fixture,
        html(
            number="TEST-001EN",
            links=("/products/en/", "/cards/searchresults?expansion=EN"),
        ),
        region="en",
        number="TEST-001EN",
    )
    install_identity(
        fixture.root,
        identity_envelope([identity_record(page)]),
        name="products/identities/en/001.yaml",
    )
    with pytest.raises(ValueError, match="across regions"):
        fixture.load()


def test_manual_product_shares_global_region_namespace(
    identity_fixture: IdentityFixture,
) -> None:
    from sve_carddb.domains.products import load_products  # ruff: ignore[import-outside-top-level] -- reload the complete augmented catalog

    from .product_fixtures import envelope, install, product  # ruff: ignore[import-outside-top-level] -- isolate the existing manual namespace contract

    fixture = identity_fixture
    manual = product(region="en")
    data = obj(manual["data"])
    data.update(id="permanent-example", family_id="TEST")
    install(fixture.root, "products/product/unassigned/001.yaml", envelope([manual]))
    catalog = load_products(fixture.root, registry=fixture.preview.snapshot)
    with pytest.raises(ValueError, match="across regions"):
        load_product_identities(
            fixture.root,
            authored_revision=fixture.revision,
            catalog=catalog,
            stores={"test-store": fixture.store},
        )


def test_duplicate_match_same_id_with_distinct_evidence_still_fails(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    row = identity_record(fixture.pages[0])
    refs = items(row["evidence"])
    refs.append(obj(refs[0]).copy() | {"role": "supplementary"})
    install_identity(
        fixture.root, identity_envelope([row]), name="products/identities/jp/002.yaml"
    )
    with pytest.raises(ValueError, match="Duplicate global product identity match"):
        fixture.load()


def test_other_evidence_roles_have_closure_without_claiming_parser_use(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    row = identity_record(fixture.pages[0])
    refs = items(row["evidence"])
    refs.append(
        obj(refs[0]).copy() | {"role": "supplementary", "locator": "Human note"}
    )
    install_identity(fixture.root, identity_envelope([row]))
    fixture.revision = commit(fixture.root)
    uses = fixture.load().source_uses()
    assert sum(use.usage == "product_identity_evidence_closure" for use in uses) == 2
    assert sum(use.usage == "official_product_identity" for use in uses) == 1


@pytest.mark.parametrize("ordinal", [True, -1, 9_007_199_254_740_992, "0"])
def test_source_block_ordinal_is_strict_uint(
    identity_fixture: IdentityFixture, ordinal: JsonValue
) -> None:
    fixture = identity_fixture
    row = identity_record(fixture.pages[0], match_index=-1)
    obj(obj(row["data"])["match"])["product_block_ordinal"] = ordinal
    install_identity(fixture.root, identity_envelope([row]))
    error = (
        "Expected safe integer"
        if ordinal == 9_007_199_254_740_992
        else "product_block_ordinal:"
    )
    with pytest.raises(ValueError, match=error):
        fixture.load()


def test_source_block_alias_does_not_generalize_to_new_version(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    install_identity(
        fixture.root,
        identity_envelope([identity_record(fixture.pages[0], match_index=-1)]),
    )
    fixture.revision = commit(fixture.root)
    source = fixture.pages[0].source.model_copy(update={"id": "src:v1:" + "0" * 64})
    changed = parse_products(html(), source, "jp")
    assert fixture.load().match("jp", changed.blocks[0].matches) is None


@pytest.mark.parametrize(
    "query",
    [
        "Expansion=Test-A",
        "expansion=",
        "expansion=Test-A&expansion=Test-A",
        "expansion=Other",
    ],
)
def test_expansion_match_requires_exact_verified_query(
    identity_fixture: IdentityFixture, query: str
) -> None:
    fixture = identity_fixture
    row = identity_record(fixture.pages[0])
    obj(row["data"])["match"] = {
        "kind": "expansion_link",
        "search_url": "https://shadowverse-evolve.com/cardlist/cardsearch?" + query,
        "expansion_code": "Test-A",
    }
    install_identity(fixture.root, identity_envelope([row]))
    with pytest.raises(ValueError, match=re.escape("records.0.data:value_error")):
        fixture.load()


def test_unsorted_records_are_accepted(identity_fixture: IdentityFixture) -> None:
    fixture = identity_fixture
    page = fixture.pages[0]
    shard = identity_envelope(
        [identity_record(page), identity_record(page, match_index=-1)]
    )
    items(shard["records"]).reverse()
    install_identity(fixture.root, shard)
    fixture.revision = commit(fixture.root)
    assert len(fixture.load().records) == 2


def test_evidence_region_is_not_assumed_from_match_region(
    identity_fixture: IdentityFixture,
) -> None:

    fixture = identity_fixture
    record = identity_record(fixture.pages[0], match_index=-1, product_id="english-id")
    record["filing_key"] = "en"
    data = obj(record["data"])
    data["region"] = "en"
    install_identity(
        fixture.root,
        identity_envelope([record]),
        name="products/identities/en/001.yaml",
    )
    fixture.revision = commit(fixture.root)
    with pytest.raises(ValueError, match="source region/kind mismatch"):
        fixture.load()


def test_ambiguous_block_can_only_use_confirmed_exact_source_block(
    identity_fixture: IdentityFixture,
) -> None:
    from .product_identity_fixtures import add_page  # ruff: ignore[import-outside-top-level] -- independently sealed ambiguous block

    fixture = identity_fixture
    page = add_page(
        fixture, html(number="TEST-002", links=("/products/a/", "/products/b/"))
    )
    record = identity_record(page)
    assert obj(obj(record["data"])["match"])["kind"] == "source_block"
    install_identity(
        fixture.root,
        identity_envelope([record]),
        name="products/identities/jp/002.yaml",
    )
    fixture.revision = commit(fixture.root)
    assert fixture.load().match("jp", page.blocks[0].matches) == "permanent-example"


def test_state_field_is_not_part_of_the_identity_format(
    identity_fixture: IdentityFixture,
) -> None:
    shard = obj(read_yaml(identity_fixture.root / NAME))
    first_record(shard)["state"] = "proposed"
    with pytest.raises(ValueError, match=re.escape("records.0.state:extra_forbidden")):
        _model(IdentityShard, shard)


def test_comment_only_working_tree_edit_keeps_semantic_hash(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    path = fixture.root / NAME
    before = read_yaml(path)
    path.write_bytes(path.read_bytes() + b"# Physical-only edit\n")
    assert read_yaml(path) == before
    assert fixture.load().records


@pytest.mark.parametrize("field", ["source_version_id", "product_block_ordinal"])
def test_source_block_version_and_ordinal_match_the_evidence_reference(
    identity_fixture: IdentityFixture, field: str
) -> None:

    fixture = identity_fixture
    record = identity_record(fixture.pages[0], match_index=-1)
    data = obj(record["data"])
    match = obj(data["match"])
    match[field] = "src:v1:" + "1" * 64 if field == "source_version_id" else 1
    install_identity(fixture.root, identity_envelope([record]))
    fixture.revision = commit(fixture.root)
    with pytest.raises(
        ValueError, match="Product identity source block evidence mismatch"
    ):
        fixture.load()


def test_filing_path_is_checked_independently_of_data_region(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    shard = obj(read_yaml(fixture.root / NAME))
    (fixture.root / NAME).unlink()
    install_identity(fixture.root, shard, name="products/identities/en/001.yaml")
    fixture.revision = commit(fixture.root)
    with pytest.raises(
        ValueError, match="Product identity filing/path/region mismatch"
    ):
        fixture.load()


class TestIdentityWireConstraints:
    @pytest.fixture(scope="class")
    def identity_wire(self) -> bytes:
        match: dict[str, JsonValue] = {
            "kind": "product_link",
            "product_url": "https://shadowverse-evolve.com/products/synthetic/",
            "expansion_code": "Test-A",
        }
        record: dict[str, JsonValue] = {
            "kind": "product_identity",
            "filing_key": "jp",
            "data": {"product_id": "permanent-example", "region": "jp", "match": match},
            "evidence": [
                {
                    "batch_id": "sha256:" + "1" * 64,
                    "source_version_id": "src:v1:" + "2" * 64,
                    "locator": '{"product_block_ordinal":0}',
                    "role": "product_identity_match",
                }
            ],
        }
        return json.dumps(identity_envelope([record])).encode()

    @pytest.mark.parametrize(
        ("area", "field", "value"),
        [
            ("shard", "format", True),
            ("shard", "format", 2),
            ("shard", "kind", "product_shard"),
            ("shard", "extra", "unexpected"),
            ("shard", "records", []),
            ("shard", "decisions", []),
            ("shard", "default_decision_id", "d:" + "0" * 64),
            ("record", "kind", "product"),
            ("record", "filing_key", "en"),
            ("record", "record_key", '["product_identity","jp",{}]'),
            ("record", "extra", "unexpected"),
            ("record", "evidence", []),
            ("data", "product_id", "UPPER"),
            ("data", "product_id", " leading"),
            ("data", "product_id", "trailing\n"),
            ("data", "product_id", "a/../b"),
            ("data", "product_id", "商品"),
            ("data", "product_id", 3),
            ("data", "region", "cn"),
            ("data", "family_id", "TEST"),
            ("match", "kind", "fuzzy"),
            (
                "match",
                "product_url",
                "http://shadowverse-evolve.com/products/synthetic/",
            ),
            (
                "match",
                "product_url",
                "https://en.shadowverse-evolve.com/products/synthetic/",
            ),
            (
                "match",
                "product_url",
                "https://dev.shadowverse-evolve.com/products/synthetic/",
            ),
            ("match", "product_url", "https://shadowverse-evolve.com/cardlist/"),
            ("match", "extra", "unexpected"),
            ("match", "expansion_code", ""),
            ("evidence", "store_id", "../unsafe"),
            ("evidence", "source_version_id", "sha256:" + "1" * 64),
            ("evidence", "locator", ""),
            ("evidence", "role", ""),
            ("evidence", "extra", "unexpected"),
            ("record", "state", "confirmed"),
            ("record", "note", "Synthetic review note"),
        ],
    )
    def test_each_wire_constraint(
        self,
        identity_wire: bytes,
        tmp_path: Path,
        area: str,
        field: str,
        value: JsonValue,
    ) -> None:
        shard = obj(json.loads(identity_wire))
        record = first_record(shard)
        target = {
            "shard": shard,
            "record": record,
            "data": obj(record["data"]),
            "match": obj(obj(record["data"])["match"]),
            "evidence": obj(items(record["evidence"])[0]),
        }[area]
        target[field] = value
        expected = wire_error(area, field)
        with pytest.raises(ValueError, match=re.escape(expected)):
            check_wire(tmp_path, shard, area, field)

    @pytest.mark.parametrize(
        ("area", "field"),
        [
            ("data", "product_id"),
            ("data", "region"),
            ("data", "match"),
            ("match", "expansion_code"),
            ("record", "evidence"),
        ],
    )
    def test_required_nullable_and_nonnullable_fields(
        self, identity_wire: bytes, area: str, field: str
    ) -> None:
        shard = obj(json.loads(identity_wire))
        record = first_record(shard)
        target = {
            "record": record,
            "data": obj(record["data"]),
            "match": obj(obj(record["data"])["match"]),
        }[area]
        del target[field]
        prefix = {
            "record": "records.0.",
            "data": "records.0.data.",
            "match": "records.0.data.match.product_link.",
        }[area]
        with pytest.raises(ValueError, match=re.escape(prefix + field + ":missing")):
            _model(IdentityShard, shard)

    @pytest.mark.parametrize("constraint", ["duplicate_evidence", "missing_match_role"])
    def test_each_evidence_constraint(
        self, identity_wire: bytes, tmp_path: Path, constraint: str
    ) -> None:
        shard = obj(json.loads(identity_wire))
        refs = items(first_record(shard)["evidence"])
        if constraint == "duplicate_evidence":
            refs.append(copy.deepcopy(refs[0]))
        else:
            obj(refs[0])["role"] = "family"
        write_yaml(tmp_path / NAME, shard)
        message = {
            "duplicate_evidence": "Duplicate product identity evidence",
            "missing_match_role": "Product identity requires match evidence",
        }[constraint]
        with pytest.raises(ValueError, match=message):
            _shard(tmp_path, NAME)

    def test_wire_base_is_valid_and_each_decode_is_private(
        self, identity_wire: bytes, tmp_path: Path
    ) -> None:
        shard = obj(json.loads(identity_wire))
        write_yaml(tmp_path / NAME, shard)
        loaded = _shard(tmp_path, NAME)
        first_record(shard)["kind"] = "corrupt"
        assert (
            first_record(obj(json.loads(identity_wire)))["kind"] == "product_identity"
        )
        with pytest.raises(ValidationError, match="frozen"):
            loaded.envelope.records[0].data.product_id = "changed"  # type: ignore[misc]  # exercise the runtime frozen boundary


@pytest.mark.parametrize("area", ["shard", "record", "data", "match", "evidence"])
def test_loader_runs_each_wire_check(
    identity_fixture: IdentityFixture, area: str
) -> None:
    fixture = identity_fixture
    shard = obj(read_yaml(fixture.root / NAME))
    record = first_record(shard)
    target = {
        "shard": shard,
        "record": record,
        "data": obj(record["data"]),
        "match": obj(obj(record["data"])["match"]),
        "evidence": obj(items(record["evidence"])[0]),
    }[area]
    field, value = {
        "shard": ("kind", "wrong"),
        "record": ("filing_key", "en"),
        "data": ("product_id", "UPPER"),
        "match": ("kind", "wrong"),
        "evidence": ("role", ""),
    }[area]
    target[field] = value
    install_identity(fixture.root, shard)
    fixture.revision = commit(fixture.root)
    with pytest.raises(ValueError, match=re.escape(wire_error(area, field))):
        fixture.load()


def check_wire(root: Path, shard: JsonValue, area: str, field: str) -> None:
    if area == "record" and field in {"filing_key", "record_key"}:
        write_yaml(root / NAME, shard)
        _shard(root, NAME)
    else:
        _model(IdentityShard, shard)


def test_working_tree_shard_is_read_once(
    identity_fixture: IdentityFixture, monkeypatch: pytest.MonkeyPatch
) -> None:
    read = Path.read_bytes
    seen: list[Path] = []

    def watched(path: Path) -> bytes:
        seen.append(path)
        return read(path)

    monkeypatch.setattr(Path, "read_bytes", watched)
    identity_fixture.load()
    assert seen.count(identity_fixture.root / NAME) == 1
