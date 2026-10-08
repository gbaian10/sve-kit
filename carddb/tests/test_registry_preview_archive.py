"""Frozen JP adapter tests use synthetic HTML and temporary sealed archives only."""

import hashlib
from dataclasses import replace
from datetime import timedelta
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build.source_rows import source_values
from sve_carddb.domains.registry.inputs import canonical
from sve_carddb.domains.registry.parser_adapters.official_jp import legacy_projection
from sve_carddb.domains.registry.preview import FrozenJP
from sve_carddb.domains.registry.records import Observation
from sve_carddb.domains.registry.review import observation
from sve_carddb.ingest.archive.manifest import Kind, Manifest
from sve_carddb.ingest.archive.source_archive import ArchiveError, seal_batch
from sve_carddb.parse.pages.extract_jp import extract_card
from sve_carddb.parse.pages.official_jp import card_url

from .test_source_archive import NOW, _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path


RAW = (
    """<!DOCTYPE html><html><body>
<div class="cardlist-Detail"><div class="cardlist-Detail_Box_Inner">
<div class="img"><img src="/synthetic/exact-image.png"></div><h1 class="ttl">Synthetic card</h1>
<div class="info"><dl><dt>クラス</dt><dd>エルフ</dd></dl>
<dl><dt>カード種類</dt><dd>フォロワー</dd></dl><dl><dt>タイプ</dt><dd>-</dd></dl>
<dl><dt>レアリティ</dt><dd>LG</dd></dl></div><div class="status">
<span class="status-Item status-Item-Cost"><span class="heading">コスト</span>1</span>
<span class="status-Item status-Item-Power"><span class="heading">攻撃力</span>1</span>
<span class="status-Item status-Item-Hp"><span class="heading">体力</span>1</span></div>
<div class="detail">Synthetic rule.</div><div class="speech"></div></div></div>
<!-- """
    + "x" * 1000
    + " --></body></html>"
).encode()


def test_pins_version_url_hash_first_receipt_and_parser(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    store = _store(tmp_path)
    resource = replace(
        _resource(card_url("TEST-001"), "raw/card.html", RAW, Kind.CARD),
        etag="first-etag",
        first_fetched_at=NOW - timedelta(days=3),
        last_changed_at=NOW - timedelta(days=1),
    )
    _put(store, resource, RAW)
    first = seal_batch(store)
    _put(
        store,
        replace(resource, etag="later-etag", last_checked_at=NOW + timedelta(days=1)),
        RAW,
    )
    latest = seal_batch(store)
    assert latest.batch_id != first.batch_id

    def forbidden(*_args: object, **_kwargs: object) -> None:
        raise AssertionError("Live manifest must never be opened")

    monkeypatch.setattr(Manifest, "open", forbidden)
    provider = FrozenJP(
        store.root, store.store_id, latest.batch_id, parser_version="test-parser"
    )
    found = provider.card("jp", "TEST-001")
    assert found is not None
    assert found.source.id == first.inventory.current[0].source_version_id
    assert found.source.url == card_url("TEST-001")
    assert found.source.sha256 == "sha256:" + hashlib.sha256(RAW).hexdigest()
    assert (
        found.source.raw_locator == "test-store:" + first.inventory.entries[0].blob.path
    )
    assert found.source.etag == "first-etag"
    assert found.source.fetched_at == "2026-09-28T00:00:00Z"
    assert found.source.parser_version == "test-parser"
    assert found.source.archive.batch_id == latest.batch_id
    assert (
        found.source.archive.first_receipt_id != latest.inventory.entries[0].receipt_id
    )
    assert source_values(found.source)["parser_version"] is None
    expected = observation(
        legacy_projection(extract_card(RAW, number="TEST-001")), "jp"
    )
    assert found.observation == Observation.model_validate_json(canonical(expected))
    assert found.faces[0].rarity_raw == "LG"
    assert provider.card("jp", "MISSING") is None
    assert provider.card("en", "TEST-001") is None
    assert not provider.coverage(latest.batch_id)


@pytest.mark.parametrize("kind", ["descriptors", "receipts", "raw"])
def test_rechecks_pinned_bytes_after_provider_creation(
    tmp_path: Path, kind: str
) -> None:
    store = _store(tmp_path)
    _put(store, _resource(card_url("TEST-001"), "raw/card.html", RAW, Kind.CARD), RAW)
    sealed = seal_batch(store)
    provider = FrozenJP(
        store.root, store.store_id, sealed.batch_id, parser_version="test-parser"
    )
    path = next((store.root / kind).rglob("*.raw" if kind == "raw" else "*.json"))
    path.write_bytes(path.read_bytes() + b" ")
    with pytest.raises(ArchiveError, match="content hash"):
        provider.card("jp", "TEST-001")


def test_batch_seal_is_verified_before_evidence_access(tmp_path: Path) -> None:
    store = _store(tmp_path)
    _put(store, _resource(card_url("TEST-001"), "raw/card.html", RAW, Kind.CARD), RAW)
    sealed = seal_batch(store)
    seal = next((store.root / "batches").rglob("seal.json"))
    seal.write_text("{}")
    with pytest.raises(ArchiveError):
        FrozenJP(
            store.root, store.store_id, sealed.batch_id, parser_version="test-parser"
        )
