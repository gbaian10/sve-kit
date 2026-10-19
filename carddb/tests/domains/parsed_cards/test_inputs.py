"""Exercise portable inputs against the real archive and all physical projections."""

import json
from dataclasses import replace
from pathlib import Path

import pytest
from typer.testing import CliRunner

from sve_carddb.core.json import digest
from sve_carddb.core.regions import Region, SourceRegion
from sve_carddb.domains import physical_sources
from sve_carddb.domains.parsed_cards import batches
from sve_carddb.domains.parsed_cards.codec import decode, encode
from sve_carddb.domains.parsed_cards.commands import app
from sve_carddb.domains.parsed_cards.filing import band
from sve_carddb.domains.parsed_cards.generate import generate
from sve_carddb.domains.parsed_cards.models import Index, Shard, SplitFamily
from sve_carddb.domains.parsed_cards.projection import replay
from sve_carddb.domains.parsed_cards.storage import load, write
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import seal_batch
from sve_carddb.parse.pages import official_en, official_jp

from ...ingest.test_source_archive import _put, _resource, _store
from ...support.en_extract_fixtures import page

REVISION = "a" * 40


def _raw(region: Region) -> tuple[str, bytes]:
    if region == "en":
        return "SYN-199EN", page("SYN-199EN", double=True)
    return "SYN01-007", (
        Path(__file__).parents[2] / "fixtures/synthetic_jp/F07-card.html"
    ).read_bytes()


def _batch(
    tmp_path: Path, region: Region, *, changed: bool = False
) -> tuple[FrozenSources, str]:
    store = _store(tmp_path)
    number, raw = _raw(region)
    if changed:
        raw = raw.replace(b"Synthetic front", b"Revised synthetic front")
    adapter = official_jp if region == "jp" else official_en
    resource = replace(
        _resource(adapter.card_url(number), "raw/card.html", raw, Kind.CARD),
        region=SourceRegion(region),
    )
    _put(store, resource, raw)
    batch = seal_batch(store)
    return FrozenSources(store.root, store.store_id, batch.batch_id), number


@pytest.mark.parametrize("region", ["jp", "en"])
def test_roundtrip_all_projections_without_archive(
    tmp_path: Path, region: Region
) -> None:
    sources, number = _batch(tmp_path, region)
    filing = {(region, number): "SYN"}
    catalog = generate([(region, sources)], filing, producer_revision=REVISION)
    version = next(iter(catalog.observations()))
    source, _ = sources.metadata(version, parser_version="synthetic-consumer")
    expected = physical_sources.page(
        sources, version, region=region, parser_version="synthetic-consumer"
    )
    root = tmp_path / "inputs"
    assert write(root, catalog) == catalog
    assert (
        generate(
            [(region, sources)], filing, producer_revision="b" * 40, previous=catalog
        )
        == catalog
    )
    sources.root.rename(tmp_path / "unmounted")
    restored = load(root)
    assert (
        batches.inventory(restored, sources.batch_id, sources.store_id)
        == sources.inventory
    )
    assert (
        batches.source(
            restored.observations()[version],
            batch_id=sources.batch_id,
            store_id=sources.store_id,
            parser_version="synthetic-consumer",
        )
        == source
    )
    with pytest.raises(ValueError, match="original batch identity"):
        batches.inventory(restored, sources.batch_id, "incorrect-store")
    actual = replay(restored.observations()[version], source)
    assert actual.record.get() == expected.record.get()
    assert actual.observation.get() == expected.observation.get()
    assert actual.products.get() == expected.products.get()
    assert actual.extras.get() == expected.extras.get()
    assert actual.text.get().model_dump() == expected.text.get().model_dump()
    for index in range(len(actual.text.get().faces)):
        assert actual.text.get().projected(index) == expected.text.get().projected(
            index
        )
    assert encode(restored.index) == encode(catalog.index)


@pytest.mark.parametrize(
    ("region", "number", "expected"),
    [
        ("jp", "PR-000", "000-199"),
        ("jp", "PR-199", "000-199"),
        ("jp", "PR-200", "200-399"),
        ("en", "PR-399EN", "200-399"),
        ("en", "PR-400EN", "400-599"),
        ("jp", "CP04-SP01", "000-199"),
        ("en", "BP03-LDⓈ01EN", "000-199"),
        ("jp", "SYN-1234a", "1200-1399"),
        ("jp", "SYN-EP", "unnumbered"),
        ("jp", "SYN-SEP", "unnumbered"),
    ],
)
def test_fixed_ranges(region: Region, number: str, expected: str) -> None:
    assert band(number, region) == expected


@pytest.mark.parametrize("number", ["NO-HYPHEN", "SYN12", "SYN-12-34x56", "SYN-١٢"])
def test_unknown_number_requires_explicit_exception(number: str) -> None:
    with pytest.raises(ValueError, match="range"):
        band(number, "jp")


def test_split_and_history_cannot_be_reassigned(tmp_path: Path) -> None:
    sources, number = _batch(tmp_path, "en")
    catalog = generate(
        [("en", sources)],
        {("en", number): "SYN"},
        producer_revision=REVISION,
        split_families=(SplitFamily(region="en", family="SYN"),),
    )
    assert catalog.index.shards[0].path == "en/SYN/000-199.json"
    assert write(tmp_path / "split", catalog) == catalog
    changed = generate(
        [("en", sources)], {("en", number): "OTHER"}, producer_revision=REVISION
    )
    with pytest.raises(ValueError, match="existing filing"):
        changed.validate_update(catalog)
    new_sources, new_number = _batch(tmp_path / "different", "jp")
    replacement = generate(
        [("jp", new_sources)], {("jp", new_number): "SYN"}, producer_revision=REVISION
    )
    with pytest.raises(ValueError, match="historical observation"):
        replacement.validate_update(catalog)


@pytest.mark.parametrize("change", ["text", "region", "face", "recipe", "unknown"])
def test_tampering_rejected_even_with_updated_outer_hash(
    tmp_path: Path, change: str
) -> None:
    sources, number = _batch(tmp_path, "en")
    catalog = generate(
        [("en", sources)], {("en", number): "SYN"}, producer_revision=REVISION
    )
    root = tmp_path / "inputs"
    write(root, catalog)
    pin = catalog.index.shards[0]
    path = root / pin.path
    value = json.loads(path.read_bytes())
    observation = value["cards"][0]["observations"][0]
    if change == "text":
        observation["document"]["record"]["faces"][0]["name"] = "Changed"
    elif change == "region":
        observation["document"]["region"] = "jp"
    elif change == "face":
        observation["layout"]["presence"].reverse()
    elif change == "recipe":
        observation["recipe"] = "unknown-parser"
    else:
        observation["document"]["record"]["unrecognized"] = "unexpected"
    raw = (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode()
    path.write_bytes(raw)
    changed_pin = pin.model_copy(update={"sha256": digest(raw), "bytes": len(raw)})
    (root / "index.json").write_bytes(
        encode(catalog.index.model_copy(update={"shards": (changed_pin,)}))
    )
    with pytest.raises(ValueError, match=r"schema|Parsed"):
        load(root)


def test_missing_duplicate_and_unlisted_members(tmp_path: Path) -> None:
    sources, number = _batch(tmp_path, "en")
    catalog = generate(
        [("en", sources)], {("en", number): "SYN"}, producer_revision=REVISION
    )
    raw = encode(catalog.index)
    with pytest.raises(ValueError, match=r"Duplicate|duplicate"):
        decode(Index, raw.replace(b'"format": 1,', b'"format": 1, "format": 1,'))
    with pytest.raises(ValueError, match="canonical"):
        decode(Shard, encode(catalog.shards[0]) + b"\n")
    root = tmp_path / "inputs"
    write(root, catalog)
    extra = root / "unlisted.json"
    extra.write_text("{}")
    with pytest.raises(ValueError, match="unlisted"):
        load(root)
    extra.unlink()
    shard = root / catalog.index.shards[0].path
    shard.unlink()
    with pytest.raises(FileNotFoundError):
        load(root)
    shard.symlink_to(root / "index.json")
    with pytest.raises(ValueError, match="symlink"):
        load(root)


def test_append_history_preserves_old_bytes_and_rejects_deletion(
    tmp_path: Path,
) -> None:
    first, number = _batch(tmp_path / "first", "en")
    second, _ = _batch(tmp_path / "second", "en", changed=True)
    filing: dict[tuple[Region, str], str] = {("en", number): "SYN"}
    original = generate([("en", first)], filing, producer_revision=REVISION)
    updated = generate(
        [("en", second)], filing, producer_revision=REVISION, previous=original
    )
    assert len(updated.observations()) == 2
    old_version = next(iter(original.observations()))
    assert updated.observations()[old_version] == original.observations()[old_version]
    assert write(tmp_path / "updated", updated) == updated
    current_only = generate([("en", second)], filing, producer_revision=REVISION)
    with pytest.raises(ValueError, match="historical observation"):
        current_only.validate_update(updated)


def test_cli_generate_validate_schema_and_fail_closed(tmp_path: Path) -> None:
    sources, number = _batch(tmp_path / "source", "en")
    request = tmp_path / "request.json"
    request.write_text(
        json.dumps(
            {
                "sources": [
                    {
                        "region": "en",
                        "root": str(sources.root),
                        "store_id": sources.store_id,
                        "batch_id": sources.batch_id,
                    }
                ],
                "filing": [{"region": "en", "card_no": number, "family": "SYN"}],
                "producer_revision": REVISION,
            }
        )
    )
    root = tmp_path / "candidate"
    runner = CliRunner()
    result = runner.invoke(app, ["generate", str(request), str(root)])
    assert result.exit_code == 0, result.output
    result = runner.invoke(app, ["validate", str(root), "--previous", str(root)])
    assert result.exit_code == 0, result.output
    result = runner.invoke(app, ["generate", str(request), str(root)])
    assert result.exit_code == 1
    assert "Synthetic front" not in result.output
    assert str(sources.root) not in result.output
    schemas = tmp_path / "schemas"
    assert runner.invoke(app, ["schema", str(schemas)]).exit_code == 0
    assert (
        json.loads((schemas / "index.schema.json").read_bytes())["properties"][
            "range_width"
        ]["const"]
        == 200
    )
    (root / "index.json").write_bytes(b"{}")
    assert runner.invoke(app, ["validate", str(root)]).exit_code == 1
