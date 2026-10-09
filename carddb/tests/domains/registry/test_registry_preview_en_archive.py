"""Synthetic sealed EN sources exercise metadata pins, region gates and raw changes."""

import hashlib
import json
from dataclasses import replace
from datetime import timedelta
from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb import cli
from sve_carddb.build.source_rows import source_values
from sve_carddb.core.json import canonical
from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.registry.preview import FrozenEN, FrozenJP, FrozenRegions
from sve_carddb.domains.registry.records import Observation
from sve_carddb.domains.registry.review import observation
from sve_carddb.ingest.archive.manifest import Kind, Link, Manifest
from sve_carddb.ingest.archive.source_archive import ArchiveError, seal_batch
from sve_carddb.ingest.http.validate import ValidationError
from sve_carddb.ingest.queries import list_root, sets_root
from sve_carddb.parse.pages import official_en as en
from sve_carddb.parse.pages import official_jp as jp
from sve_carddb.parse.pages.extract_en import extract_card

from ...ingest.test_source_archive import NOW, _put, _resource, _store
from ...support.en_extract_fixtures import page
from ...support.registry_observation_fixtures import parsed_card
from .test_registry_preview_archive import RAW as JP_RAW

if TYPE_CHECKING:
    from pathlib import Path


NUMBER = "SYNⓈ-01aEN"


def test_double_face_rarity_is_read_from_each_face(tmp_path: Path) -> None:
    raw = b"<dd>GR</dd>".join(page(double=True).rsplit(b"<dd>LG</dd>", 1))
    store = _store(tmp_path)
    _put(
        store,
        replace(
            _resource(en.card_url(NUMBER), "raw/card.html", raw, Kind.CARD),
            region=SourceRegion.EN,
        ),
        raw,
    )
    sealed = seal_batch(store)
    provider = FrozenEN(
        store.root, store.store_id, sealed.batch_id, parser_version="en-test"
    )
    found = provider.card("en", NUMBER)
    assert found is not None
    assert tuple(face.rarity_raw for face in found.faces) == ("LG", "GR")


def test_exact_en_source_pins_first_receipt_and_recomputes_both_hashes(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw = page(double=True)
    store = _store(tmp_path)
    resource = replace(
        _resource(en.card_url(NUMBER), "raw/card.html", raw, Kind.CARD),
        region=SourceRegion.EN,
        etag="original-etag",
        first_fetched_at=NOW - timedelta(days=4),
        last_changed_at=NOW - timedelta(days=2),
    )
    _put(store, resource, raw)
    first = seal_batch(store)
    _put(
        store,
        replace(resource, etag="new-etag", last_checked_at=NOW + timedelta(days=1)),
        raw,
    )
    latest = seal_batch(store)

    def forbidden(*_args: object, **_kwargs: object) -> None:
        raise AssertionError("Live manifest must never be opened")

    monkeypatch.setattr(Manifest, "open", forbidden)
    provider = FrozenEN(
        store.root, store.store_id, latest.batch_id, parser_version="en-test-pin"
    )
    found = provider.card("en", NUMBER)
    assert found is not None
    assert found.source.id == first.inventory.current[0].source_version_id
    assert found.source.sha256 == "sha256:" + hashlib.sha256(raw).hexdigest()
    assert found.source.url == en.card_url(NUMBER)
    assert (
        found.source.raw_locator == "test-store:" + first.inventory.entries[0].blob.path
    )
    assert found.source.kind == "official_page"
    assert found.source.etag == "original-etag"
    assert found.source.fetched_at == "2026-09-27T00:00:00Z"
    assert found.source.parser_version == "en-test-pin"
    assert source_values(found.source)["parser_version"] is None
    assert found.source.archive.batch_id == latest.batch_id
    assert (
        found.source.archive.first_receipt_id != latest.inventory.entries[0].receipt_id
    )
    expected = observation(parsed_card(extract_card(raw, number=NUMBER)), "en")
    assert found.observation == Observation.model_validate_json(canonical(expected))
    assert len(found.faces) == 2
    assert all(
        face.rarity_raw == "LG" and face.credit_raw == "Synthetic artist"
        for face in found.faces
    )
    assert provider.card("jp", NUMBER) is None
    assert provider.card("en", "MISSING") is None
    assert provider.card("en", NUMBER.removesuffix("EN")) is None
    assert not provider.coverage(latest.batch_id)
    assert not provider.coverage("sha256:" + "0" * 64)


@pytest.mark.parametrize("kind", ["descriptors", "receipts", "raw"])
def test_en_rechecks_each_pinned_file_after_construction(
    tmp_path: Path, kind: str
) -> None:
    raw = page()
    store = _store(tmp_path)
    resource = replace(
        _resource(en.card_url(NUMBER), "raw/card.html", raw, Kind.CARD),
        region=SourceRegion.EN,
    )
    _put(store, resource, raw)
    sealed = seal_batch(store)
    provider = FrozenEN(
        store.root, store.store_id, sealed.batch_id, parser_version="en-test"
    )
    path = next((store.root / kind).rglob("*.raw" if kind == "raw" else "*.json"))
    path.write_bytes(path.read_bytes() + b" ")
    with pytest.raises(ArchiveError, match="content hash"):
        provider.card("en", NUMBER)


@pytest.mark.parametrize("mismatch", ["provider", "kind", "media", "number"])
def test_en_refuses_wrong_source_identity_and_page_number(
    tmp_path: Path, mismatch: str
) -> None:
    raw = page("WRONG" if mismatch == "number" else NUMBER)
    store = _store(tmp_path)
    resource = replace(
        _resource(en.card_url(NUMBER), "raw/card.html", raw, Kind.CARD),
        region=SourceRegion.EN,
    )
    if mismatch == "provider":
        resource = replace(resource, region=SourceRegion.JP)
    elif mismatch == "kind":
        resource = replace(resource, kind=Kind.IMAGE)
    elif mismatch == "media":
        resource = replace(resource, content_type="image/png")
    _put(store, resource, raw)
    sealed = seal_batch(store)
    provider = FrozenEN(
        store.root, store.store_id, sealed.batch_id, parser_version="en-test"
    )
    with pytest.raises((ArchiveError, ValidationError)):
        provider.card("en", NUMBER)


def test_en_rejects_unsealed_batch(tmp_path: Path) -> None:
    raw = page()
    store = _store(tmp_path)
    _put(
        store,
        replace(
            _resource(en.card_url(NUMBER), "raw/card.html", raw, Kind.CARD),
            region=SourceRegion.EN,
        ),
        raw,
    )
    sealed = seal_batch(store)
    seal = next((store.root / "batches").rglob("seal.json"))
    seal.write_text("{}")
    with pytest.raises(ArchiveError):
        FrozenEN(store.root, store.store_id, sealed.batch_id, parser_version="en-test")


def test_back_face_rule_change_is_a_new_mismatching_observation(tmp_path: Path) -> None:
    raw = page(double=True)
    store = _store(tmp_path)
    resource = replace(
        _resource(en.card_url(NUMBER), "raw/card.html", raw, Kind.CARD),
        region=SourceRegion.EN,
    )
    _put(store, resource, raw)
    first = seal_batch(store)
    old = FrozenEN(
        store.root, store.store_id, first.batch_id, parser_version="en-test"
    ).card("en", NUMBER)
    assert old is not None
    changed = raw.rsplit(b"Auxiliary", 1)
    raw_new = b"Changed auxiliary".join(changed)
    _put(
        store,
        replace(
            resource,
            sha256=hashlib.sha256(raw_new).hexdigest(),
            raw_bytes=len(raw_new),
            stored_bytes=len(raw_new),
            last_changed_at=NOW + timedelta(days=1),
        ),
        raw_new,
    )
    second = seal_batch(store)
    new = FrozenEN(
        store.root, store.store_id, second.batch_id, parser_version="en-test"
    ).card("en", NUMBER)
    assert new is not None
    assert old.observation.observation_hash != new.observation.observation_hash
    assert old.observation.rules_hash != new.observation.rules_hash
    assert old.source.id != new.source.id
    pinned = FrozenEN(
        store.root, store.store_id, first.batch_id, parser_version="en-test"
    ).card("en", NUMBER)
    assert pinned == old


def test_composition_dispatches_explicit_regions_without_cross_region_fallback(
    tmp_path: Path,
) -> None:
    store = _store(tmp_path)
    raw = page()
    _put(
        store,
        replace(
            _resource(en.card_url(NUMBER), "raw/en.html", raw, Kind.CARD),
            region=SourceRegion.EN,
        ),
        raw,
    )
    _put(
        store,
        _resource(jp.card_url("TEST-001"), "raw/jp.html", JP_RAW, Kind.CARD),
        JP_RAW,
    )
    sealed = seal_batch(store)
    jp_provider = FrozenJP(
        store.root, store.store_id, sealed.batch_id, parser_version="jp-test"
    )
    en_provider = FrozenEN(
        store.root, store.store_id, sealed.batch_id, parser_version="en-test"
    )
    provider = FrozenRegions(jp=jp_provider, en=en_provider)
    assert provider.card("jp", "TEST-001") == jp_provider.card("jp", "TEST-001")
    assert provider.card("en", NUMBER) == en_provider.card("en", NUMBER)
    assert provider.card("jp", NUMBER) is None
    assert provider.card("en", "TEST-001") is None
    assert not provider.coverage(sealed.batch_id)


def test_archive_cli_extracts_en_twice_without_mutating_the_sealed_input(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    store = _store(tmp_path)
    raw = page(double=True)
    resource = replace(
        _resource(en.card_url(NUMBER), "raw/en.html", raw, Kind.CARD),
        region=SourceRegion.EN,
    )
    _put(store, resource, raw)
    with Manifest.open(store.manifest_path) as manifest:
        sets = manifest.generations.start(sets_root(SourceRegion.EN))
        with manifest.transaction():
            manifest.generations.add_page(
                sets.id,
                en.sets_url(),
                "sets-hash",
                [Link(en.list_url("SYN", 1), Kind.LIST, 0, "SYN")],
            )
        manifest.generations.validate(sets.id, declared_total=1)
        cards = manifest.generations.start(list_root("SYN", SourceRegion.EN))
        with manifest.transaction():
            manifest.generations.add_page(
                cards.id,
                en.list_url("SYN", 1),
                "list-hash",
                [Link(resource.url, Kind.CARD, 0, NUMBER)],
            )
        manifest.generations.validate(cards.id, declared_total=1)
    sealed = seal_batch(store)
    frozen = sealed.path / "manifest.sqlite"
    before = hashlib.sha256(frozen.read_bytes()).hexdigest()

    def forbidden(*_args: object, **_kwargs: object) -> None:
        raise AssertionError("Live manifest must never be opened")

    monkeypatch.setattr(Manifest, "open", forbidden)
    runner = CliRunner()
    outputs = [tmp_path / "one.jsonl", tmp_path / "two.jsonl"]
    for output in outputs:
        result = runner.invoke(
            cli.app,
            [
                "archive",
                "extract-cards",
                str(store.root),
                "--store-id",
                store.store_id,
                sealed.batch_id,
                str(output),
                "--region",
                "en",
            ],
        )
        assert result.exit_code == 0, result.stdout
        record = json.loads(output.read_text())
        assert record["number"] == NUMBER
        assert len(record["faces"]) == 2
        assert record["faces"][1]["sections"] == ["Auxiliary", "Last"]
    assert outputs[0].read_bytes() == outputs[1].read_bytes()
    assert hashlib.sha256(frozen.read_bytes()).hexdigest() == before
