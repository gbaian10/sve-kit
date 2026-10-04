"""Each product identity constraint has an independently invalid input."""

import copy
import json
import re
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError
from ruamel.yaml.error import YAMLError

from sve_carddb.build_inputs import BuildContext
from sve_carddb.products.identities import _model, _shard, load_product_identities
from sve_carddb.products.identity_models import (
    IdentityIndex,
    IdentityShard,
    ProductLink,
)
from sve_carddb.products.official import parse_products as parse_verified_products
from sve_carddb.registry.storage import MAX_BYTES, read_yaml
from sve_carddb.snapshot.values import digest
from sve_carddb.source_archive import ArchiveError

from .product_fixtures import checksum, decision, first_record, items, obj, write_yaml
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
    from pathlib import Path

    from sve_carddb.build_inputs import Source
    from sve_carddb.products.official import ProductPage
    from sve_carddb.registry.records import Region


def parse_products(raw: bytes, source: Source, region: Region) -> ProductPage:
    return parse_verified_products(
        raw, source.model_copy(update={"sha256": digest(raw)}), region
    )


def wire_error(area: str, field: str) -> str:
    overrides = {
        ("record", "filing_key"): "Product identity filing/path/region mismatch",
        ("record", "record_key"): "Product identity record key mismatch",
        ("match", "product_url"): "records.0.data:value_error",
        ("match", "kind"): "records.0.data.match:union_tag_invalid",
        ("decision", "reviewed_by"): "decisions.0:value_error",
        ("decision", "reviewed_at"): "decisions.0:value_error",
        ("decision", "reviewed_precision"): "decisions.0:value_error",
        ("decision", "authored_by"): "decisions.0:value_error",
    }
    prefixes = {
        "index": "Invalid product identity fields: ",
        "shard": "Invalid product identity fields: ",
        "record": "records.0.",
        "data": "records.0.data.",
        "match": "records.0.data.match.product_link.",
        "evidence": "records.0.evidence.0.",
        "decision": "decisions.0.",
    }
    return overrides.get((area, field), prefixes[area] + field + ":")


@pytest.mark.parametrize(
    "constraint",
    [
        "missing_index",
        "missing_shard",
        "unindexed",
        "yml",
        "unsafe_path",
        "wrong_sequence",
        "hash",
        "symlink_shard",
        "symlink_directory",
        "oversized",
        "duplicate_key",
        "alias",
        "unknown_yaml_tag",
        "dirty_index",
        "dirty_shard",
    ],
)
def test_inventory_yaml_and_revision_constraints(  # ruff: ignore[complex-structure, too-many-branches] -- independent filesystem and YAML counterexamples
    identity_fixture: IdentityFixture, constraint: str
) -> None:
    fixture = identity_fixture
    index_path = fixture.root / "product-identities/index.yaml"
    shard_path = fixture.root / NAME
    index = obj(read_yaml(index_path))
    if constraint == "missing_index":
        index_path.unlink()
    elif constraint == "missing_shard":
        shard_path.unlink()
    elif constraint in {"unindexed", "yml"}:
        (
            shard_path.parent / ("002.yml" if constraint == "yml" else "002.yaml")
        ).write_bytes(shard_path.read_bytes())
    elif constraint == "unsafe_path":
        index["includes"] = {"../escape.yaml": "sha256:" + "0" * 64}
        write_yaml(index_path, index)
    elif constraint == "wrong_sequence":
        shard_path.rename(shard_path.parent / "01.yaml")
        index["includes"] = {
            "product-identities/jp/01.yaml": obj(index["includes"])[NAME]
        }
        write_yaml(index_path, index)
    elif constraint == "hash":
        obj(index["includes"])[NAME] = "sha256:" + "0" * 64
        write_yaml(index_path, index)
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
        index_path.write_bytes(
            index_path.read_bytes() + b"kind: product_identity_index\n"
        )
    elif constraint == "alias":
        index_path.write_text(
            "product_identity_format: &format 1\nkind: product_identity_index\nincludes: *format\n"
        )
    elif constraint == "unknown_yaml_tag":
        index_path.write_text(
            "product_identity_format: !!int 1\nkind: product_identity_index\nincludes: {}\n"
        )
    elif constraint == "dirty_index":
        index_path.write_bytes(index_path.read_bytes() + b"# Dirty physical bytes\n")
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
        record["record_key"] = (
            '["product_identity","jp",{"expansion_code":"Other","kind":"product_link","product_url":"https://shadowverse-evolve.com/products/synthetic/"}]'
        )
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
    install_identity(fixture.root, shard, resign=True)
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
        name="product-identities/jp/002.yaml",
    )
    fixture.revision = commit(fixture.root)
    with pytest.raises(ValueError, match="Duplicate product identity decision"):
        fixture.load()


@pytest.mark.parametrize(
    "area",
    [
        "index_hash",
        "revision",
        "path",
        "missing_config",
        "missing_dependency",
        "physical_hash",
    ],
)
def test_each_build_pin_is_required(
    identity_fixture: IdentityFixture, area: str
) -> None:
    identities = identity_fixture.load()
    dependencies = identities.dependencies()
    config = identities.configuration()
    if area == "index_hash":
        config["index_hash"] = "sha256:" + "0" * 64
    elif area == "revision":
        config["authored_revision"] = "0" * 40
    elif area == "path":
        config["index_path"] = "products/index.yaml"
    elif area == "missing_dependency":
        del dependencies["authored/" + NAME]
    elif area == "physical_hash":
        dependencies["authored/" + NAME] += b"Changed"
    context = BuildContext.from_inputs(
        identity_fixture.revision,
        dependencies,
        {} if area == "missing_config" else {"product_identity": config},
    )
    with pytest.raises(ValueError, match="pin mismatch"):
        identities.verify_context(context)


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
        fixture.root, identity_envelope([alias]), name="product-identities/jp/002.yaml"
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
        fixture.root, identity_envelope([alias]), name="product-identities/jp/002.yaml"
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
        name="product-identities/en/001.yaml",
    )
    with pytest.raises(ValueError, match="across regions"):
        fixture.load()


def test_manual_product_shares_global_region_namespace(
    identity_fixture: IdentityFixture,
) -> None:
    from sve_carddb.products import load_products  # ruff: ignore[import-outside-top-level] -- reload the complete augmented catalog

    from .product_fixtures import envelope, install, product  # ruff: ignore[import-outside-top-level] -- isolate the existing manual namespace contract

    fixture = identity_fixture
    manual = product(region="en")
    data = obj(manual["data"])
    data.update(id="permanent-example", family_id="TEST")
    manual["record_key"] = '["product","permanent-example"]'
    install(fixture.root, "products/product/unassigned/001.yaml", envelope([manual]))
    catalog = load_products(fixture.root, registry=fixture.preview.snapshot)
    with pytest.raises(ValueError, match="across regions"):
        load_product_identities(
            fixture.root,
            authored_revision=fixture.revision,
            catalog=catalog,
            stores={"test-store": fixture.store},
        )


def test_duplicate_match_same_id_with_distinct_decision_still_fails(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    row = identity_record(fixture.pages[0])
    refs = items(row["evidence"])
    refs.append(obj(refs[0]).copy() | {"role": "supplementary"})
    install_identity(
        fixture.root, identity_envelope([row]), name="product-identities/jp/002.yaml"
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
    with pytest.raises(ValueError, match="product_block_ordinal:"):
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


@pytest.mark.parametrize("target", ["records", "members"])
def test_exact_order_is_checked_independently(
    identity_fixture: IdentityFixture, target: str
) -> None:
    fixture = identity_fixture
    page = fixture.pages[0]
    shard = identity_envelope(
        [identity_record(page), identity_record(page, match_index=-1)]
    )
    review = decision(shard)
    if target == "records":
        items(shard["records"]).reverse()
        items(review["members"]).reverse()
        items(review["sample_ids"]).reverse()
    else:
        items(review["members"]).reverse()
    review["membership_hash"] = checksum(review["members"])
    review["id"] = "d:" + str(review["membership_hash"]).removeprefix("sha256:")
    shard["default_decision_id"] = review["id"]
    install_identity(fixture.root, shard)
    fixture.revision = commit(fixture.root)
    message = (
        "Product identity records must be sorted and unique"
        if target == "records"
        else "Product identity exact members mismatch"
    )
    with pytest.raises(ValueError, match=message):
        fixture.load()


def test_evidence_region_is_not_assumed_from_match_region(
    identity_fixture: IdentityFixture,
) -> None:
    import json  # ruff: ignore[import-outside-top-level] -- independent canonical key signer

    fixture = identity_fixture
    record = identity_record(fixture.pages[0], match_index=-1, product_id="english-id")
    record["filing_key"] = "en"
    data = obj(record["data"])
    data["region"] = "en"
    record["record_key"] = json.dumps(
        ["product_identity", "en", data["match"]], sort_keys=True, separators=(",", ":")
    )
    install_identity(
        fixture.root, identity_envelope([record]), name="product-identities/en/001.yaml"
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
        fixture.root, identity_envelope([record]), name="product-identities/jp/002.yaml"
    )
    fixture.revision = commit(fixture.root)
    assert fixture.load().match("jp", page.blocks[0].matches) == "permanent-example"


def test_proposed_identity_fails_its_state_constraint_with_valid_proposed_metadata(
    identity_fixture: IdentityFixture,
) -> None:
    from pydantic import ValidationError  # ruff: ignore[import-outside-top-level] -- direct wire boundary isolates confirmed-only from membership checks

    from sve_carddb.products.identity_models import IdentityDecision  # ruff: ignore[import-outside-top-level] -- independently test the closed decision state

    shard = obj(read_yaml(identity_fixture.root / NAME))
    review = decision(shard)
    review.update(
        state="proposed",
        sample_ids=[],
        reviewed_by=None,
        reviewed_at=None,
        reviewed_precision=None,
    )
    with pytest.raises(ValidationError, match="Input should be 'confirmed'"):
        IdentityDecision.model_validate(review)


def test_changed_shard_bytes_with_unchanged_pinned_content_hash_fail_only_hash(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    path = fixture.root / NAME
    original = path.read_bytes()
    changed = original.replace(b"Synthetic review note", b"Changed review note")
    assert changed != original
    path.write_bytes(changed)
    fixture.revision = commit(fixture.root)
    with pytest.raises(ValueError, match="Modified immutable product identity shard"):
        fixture.load()


def test_comment_only_bytes_keep_semantic_hash_but_fail_exact_revision(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    path = fixture.root / NAME
    before = read_yaml(path)
    path.write_bytes(path.read_bytes() + b"# Physical-only edit\n")
    assert checksum(read_yaml(path)) == checksum(before)
    with pytest.raises(ValueError, match="bytes differ from pinned authored revision"):
        fixture.load()


@pytest.mark.parametrize("field", ["source_version_id", "product_block_ordinal"])
def test_source_block_version_and_ordinal_match_the_evidence_reference(
    identity_fixture: IdentityFixture, field: str
) -> None:
    import json  # ruff: ignore[import-outside-top-level] -- independent canonical match key, not the production helper

    fixture = identity_fixture
    record = identity_record(fixture.pages[0], match_index=-1)
    data = obj(record["data"])
    match = obj(data["match"])
    match[field] = "src:v1:" + "1" * 64 if field == "source_version_id" else 1
    record["record_key"] = json.dumps(
        ["product_identity", "jp", match], sort_keys=True, separators=(",", ":")
    )
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
    index_path = fixture.root / "product-identities/index.yaml"
    index = obj(read_yaml(index_path))
    del obj(index["includes"])[NAME]
    write_yaml(index_path, index)
    install_identity(fixture.root, shard, name="product-identities/en/001.yaml")
    fixture.revision = commit(fixture.root)
    with pytest.raises(
        ValueError, match="Product identity filing/path/region mismatch"
    ):
        fixture.load()


class TestIdentityWireConstraints:
    @pytest.fixture(scope="class")
    def identity_wire(self) -> tuple[bytes, bytes]:
        match: dict[str, JsonValue] = {
            "kind": "product_link",
            "product_url": "https://shadowverse-evolve.com/products/synthetic/",
            "expansion_code": "Test-A",
        }
        record: dict[str, JsonValue] = {
            "record_key": json.dumps(
                ["product_identity", "jp", match], sort_keys=True, separators=(",", ":")
            ),
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
        shard = identity_envelope([record])
        index = {
            "product_identity_format": 1,
            "kind": "product_identity_index",
            "includes": {NAME: checksum(shard)},
        }
        return json.dumps(index).encode(), json.dumps(shard).encode()

    @pytest.mark.parametrize(
        ("area", "field", "value"),
        [
            ("index", "product_identity_format", True),
            ("index", "product_identity_format", 2),
            ("index", "kind", "product_index"),
            ("index", "extra", "unexpected"),
            ("shard", "product_identity_format", True),
            ("shard", "product_identity_format", 2),
            ("shard", "kind", "product_shard"),
            ("shard", "extra", "unexpected"),
            ("shard", "records", []),
            ("shard", "decisions", []),
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
            ("decision", "state", "proposed"),
            ("decision", "state", "sampled"),
            ("decision", "scope", "row"),
            ("decision", "category", "product_catalog"),
            ("decision", "category", "identity_registry"),
            ("decision", "policy_id", "product-authored-v1"),
            ("decision", "reviewed_by", None),
            ("decision", "reviewed_by", "  "),
            ("decision", "reviewed_at", None),
            ("decision", "reviewed_at", "2026-09-30T12:00:00Z"),
            ("decision", "reviewed_precision", None),
            ("decision", "reviewed_precision", "month"),
            ("decision", "authored_by", " "),
            ("decision", "authored_at", "not-a-date"),
            ("decision", "extra", "unexpected"),
        ],
    )
    def test_each_wire_constraint(
        self,
        identity_wire: tuple[bytes, bytes],
        tmp_path: Path,
        area: str,
        field: str,
        value: JsonValue,
    ) -> None:
        index = obj(json.loads(identity_wire[0]))
        shard = obj(json.loads(identity_wire[1]))
        if area == "index":
            index[field] = value
        else:
            record = first_record(shard)
            target = {
                "shard": shard,
                "record": record,
                "data": obj(record["data"]),
                "match": obj(obj(record["data"])["match"]),
                "evidence": obj(items(record["evidence"])[0]),
                "decision": decision(shard),
            }[area]
            target[field] = value
        expected = (
            "decisions.0.reviewed_precision:literal_error"
            if area == "decision" and field == "reviewed_precision" and value == "month"
            else wire_error(area, field)
        )
        with pytest.raises(ValueError, match=re.escape(expected)):
            check_wire(tmp_path, index, shard, area, field)

    @pytest.mark.parametrize(
        ("area", "field"),
        [
            ("data", "product_id"),
            ("data", "region"),
            ("data", "match"),
            ("match", "expansion_code"),
            ("record", "evidence"),
            ("decision", "reviewed_precision"),
        ],
    )
    def test_required_nullable_and_nonnullable_fields(
        self, identity_wire: tuple[bytes, bytes], area: str, field: str
    ) -> None:
        shard = obj(json.loads(identity_wire[1]))
        record = first_record(shard)
        target = {
            "record": record,
            "data": obj(record["data"]),
            "match": obj(obj(record["data"])["match"]),
            "decision": decision(shard),
        }[area]
        del target[field]
        prefix = {
            "record": "records.0.",
            "data": "records.0.data.",
            "match": "records.0.data.match.product_link.",
            "decision": "decisions.0.",
        }[area]
        with pytest.raises(ValueError, match=re.escape(prefix + field + ":missing")):
            _model(IdentityShard, shard)

    @pytest.mark.parametrize(
        "constraint",
        [
            "semantic_hash",
            "members_missing",
            "members_extra",
            "members_reordered",
            "membership_hash",
            "decision_id",
            "default_decision_id",
            "samples_missing",
            "samples_extra",
            "samples_duplicate",
            "duplicate_evidence",
            "missing_match_role",
        ],
    )
    def test_each_exact_membership_constraint(  # ruff: ignore[complex-structure] -- each independent mutation isolates a different envelope constraint
        self, identity_wire: tuple[bytes, bytes], tmp_path: Path, constraint: str
    ) -> None:
        shard = obj(json.loads(identity_wire[1]))
        review = decision(shard)
        if constraint == "semantic_hash":
            items(items(review["members"])[0])[1] = "sha256:" + "0" * 64
        elif constraint == "members_missing":
            review["members"] = []
        elif constraint == "members_extra":
            items(review["members"]).append(["extra", "sha256:" + "0" * 64])
        elif constraint == "members_reordered":
            items(review["members"]).insert(0, ["extra", "sha256:" + "0" * 64])
        elif constraint == "membership_hash":
            review["membership_hash"] = "sha256:" + "0" * 64
        elif constraint == "decision_id":
            review["id"] = "d:" + "0" * 64
            shard["default_decision_id"] = review["id"]
        elif constraint == "default_decision_id":
            shard["default_decision_id"] = "d:" + "0" * 64
        elif constraint == "samples_missing":
            review["sample_ids"] = []
        elif constraint == "samples_extra":
            items(review["sample_ids"]).append("extra")
        elif constraint == "samples_duplicate":
            items(review["sample_ids"]).append(items(review["sample_ids"])[0])
        elif constraint == "duplicate_evidence":
            refs = items(first_record(shard)["evidence"])
            refs.append(copy.deepcopy(refs[0]))
        else:
            obj(items(first_record(shard)["evidence"])[0])["role"] = "family"
        write_yaml(tmp_path / NAME, shard)
        message = {
            "semantic_hash": "Product identity exact members mismatch",
            "members_missing": "Product identity exact members mismatch",
            "members_extra": "Product identity exact members mismatch",
            "members_reordered": "Product identity exact members mismatch",
            "membership_hash": "Product identity membership hash mismatch",
            "decision_id": "Product identity decision ID mismatch",
            "default_decision_id": "Product identity default decision mismatch",
            "samples_missing": "Product identity requires every exact member checked",
            "samples_extra": "Product identity requires every exact member checked",
            "samples_duplicate": "Product identity requires every exact member checked",
            "duplicate_evidence": "Duplicate product identity evidence",
            "missing_match_role": "Product identity requires match evidence",
        }[constraint]
        with pytest.raises(ValueError, match=message):
            _shard(tmp_path, NAME, checksum(shard))

    def test_wire_base_is_valid_and_each_decode_is_private(
        self, identity_wire: tuple[bytes, bytes], tmp_path: Path
    ) -> None:
        index = obj(json.loads(identity_wire[0]))
        shard = obj(json.loads(identity_wire[1]))
        _model(IdentityIndex, index)
        write_yaml(tmp_path / NAME, shard)
        loaded = _shard(tmp_path, NAME, checksum(shard))
        first_record(shard)["kind"] = "corrupt"
        assert (
            first_record(obj(json.loads(identity_wire[1])))["kind"]
            == "product_identity"
        )
        with pytest.raises(ValidationError, match="frozen"):
            loaded.envelope.records[0].data.product_id = "changed"  # type: ignore[misc]  # exercise the runtime frozen boundary


@pytest.mark.parametrize(
    "area",
    ["index", "shard", "record", "data", "match", "evidence", "decision", "membership"],
)
def test_loader_runs_each_wire_and_membership_check(
    identity_fixture: IdentityFixture, area: str
) -> None:
    fixture = identity_fixture
    if area == "index":
        index = obj(read_yaml(fixture.root / "product-identities/index.yaml"))
        index["kind"] = "wrong"
        write_yaml(fixture.root / "product-identities/index.yaml", index)
        expected = wire_error("index", "kind")
    else:
        shard = obj(read_yaml(fixture.root / NAME))
        record = first_record(shard)
        target = {
            "shard": shard,
            "record": record,
            "data": obj(record["data"]),
            "match": obj(obj(record["data"])["match"]),
            "evidence": obj(items(record["evidence"])[0]),
            "decision": decision(shard),
            "membership": decision(shard),
        }[area]
        field, value = {
            "shard": ("kind", "wrong"),
            "record": ("filing_key", "en"),
            "data": ("product_id", "UPPER"),
            "match": ("kind", "wrong"),
            "evidence": ("role", ""),
            "decision": ("state", "proposed"),
            "membership": ("membership_hash", "sha256:" + "0" * 64),
        }[area]
        target[field] = value
        install_identity(
            fixture.root, shard, resign=area in {"record", "data", "match", "evidence"}
        )
        expected = (
            "Product identity membership hash mismatch"
            if area == "membership"
            else wire_error(area, field)
        )
    fixture.revision = commit(fixture.root)
    with pytest.raises(ValueError, match=re.escape(expected)):
        fixture.load()


def check_wire(
    root: Path, index: JsonValue, shard: JsonValue, area: str, field: str
) -> None:
    if area == "index":
        _model(IdentityIndex, index)
    elif area == "record" and field in {"filing_key", "record_key"}:
        write_yaml(root / NAME, shard)
        _shard(root, NAME, checksum(shard))
    else:
        _model(IdentityShard, shard)
