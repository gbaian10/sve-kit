"""Source classification follows verified media, while identity requires a card HTML page."""

import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build.source_rows import source_values
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import ArchiveError, seal_batch
from sve_carddb.parse.pages.official_jp import card_url
from sve_carddb.registry.preview import FrozenJP

from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize(
    ("media", "kind"),
    [
        ("text/html; charset=utf-8", "official_page"),
        ("application/xhtml+xml", "official_page"),
        ("application/json", "official_api"),
        ("application/pdf", "official_pdf"),
        ("image/png", "image"),
    ],
)
def test_raw_kind_comes_from_verified_receipt_media(
    tmp_path: Path, media: str, kind: str
) -> None:
    store = _store(tmp_path / "frozen")
    raw = b"Synthetic content; interpretation is outside the metadata reader"
    resource = replace(
        _resource(card_url("TEST-001"), "raw/input", raw, Kind.CARD), content_type=media
    )
    _put(store, resource, raw)
    sealed = seal_batch(store)
    version = sealed.inventory.current[0].source_version_id
    source, content, descriptor = FrozenSources(
        store.root, store.store_id, sealed.batch_id
    ).read(version, parser_version="metadata-test-v1")
    assert content == raw
    assert source.id == descriptor.id == version
    assert source.kind == kind
    assert source_values(source)["parser_version"] is None


def test_unrecognized_media_is_rejected_without_inferred_kind(tmp_path: Path) -> None:
    store = _store(tmp_path / "frozen")
    raw = b"Synthetic content"
    resource = replace(
        _resource(card_url("TEST-001"), "raw/input", raw, Kind.CARD),
        content_type="text/plain",
    )
    _put(store, resource, raw)
    sealed = seal_batch(store)
    sources = FrozenSources(store.root, store.store_id, sealed.batch_id)
    with pytest.raises(ValueError, match="Unsupported product evidence media type"):
        sources.read(
            sealed.inventory.current[0].source_version_id,
            parser_version="metadata-test-v1",
        )


def test_missing_version_cannot_fall_back_to_a_current_source(tmp_path: Path) -> None:
    store = _store(tmp_path / "frozen")
    raw = b"Synthetic content"
    _put(store, _resource(card_url("TEST-001"), "raw/input", raw, Kind.CARD), raw)
    sealed = seal_batch(store)
    sources = FrozenSources(store.root, store.store_id, sealed.batch_id)
    with pytest.raises(ArchiveError, match="absent from pinned batch"):
        sources.read("src:v1:" + "0" * 64, parser_version="metadata-test-v1")


def test_identity_adapter_rejects_non_html_card_source(tmp_path: Path) -> None:
    store = _store(tmp_path / "frozen")
    raw = b"Synthetic JSON"
    resource = replace(
        _resource(card_url("TEST-001"), "raw/input", raw, Kind.CARD),
        content_type="application/json",
    )
    _put(store, resource, raw)
    sealed = seal_batch(store)
    provider = FrozenJP(
        store.root, store.store_id, sealed.batch_id, parser_version="test-v1"
    )
    with pytest.raises(ArchiveError, match="source identity mismatch"):
        provider.card("jp", "TEST-001")


def test_configured_batch_preserves_sealed_identity_and_rejects_wrong_store(
    tmp_path: Path,
) -> None:
    store = _store(tmp_path / "frozen")
    raw = b"Synthetic content"
    _put(store, _resource(card_url("TEST-001"), "raw/input", raw, Kind.CARD), raw)
    sealed = seal_batch(store)
    version = sealed.inventory.current[0].source_version_id
    batch_root = store.root / "batches" / sealed.batch_id[7:]
    before = {p.name: p.read_bytes() for p in batch_root.iterdir() if p.is_file()}
    copied = tmp_path / "relocated"
    shutil.copytree(store.root, copied)
    resolved = FrozenSources.configured({store.store_id: copied}, sealed.batch_id)
    source, content, _ = resolved.read(version, parser_version="metadata-test-v1")
    assert content == raw
    assert source.id == version
    assert source.archive.batch_id == sealed.batch_id
    assert source.archive.store_id == store.store_id
    assert before == {
        p.name: p.read_bytes() for p in batch_root.iterdir() if p.is_file()
    }
    with pytest.raises(ArchiveError, match="invalid archive blob locator"):
        FrozenSources.configured({"wrong-store": copied}, sealed.batch_id)


@pytest.mark.parametrize("mode", ["missing", "unknown", "ambiguous", "unsafe"])
def test_configured_batch_never_guesses_a_store_or_another_batch(
    tmp_path: Path, mode: str
) -> None:
    store = _store(tmp_path / "frozen")
    raw = b"Synthetic content"
    _put(store, _resource(card_url("TEST-001"), "raw/input", raw, Kind.CARD), raw)
    sealed = seal_batch(store)
    stores = {store.store_id: store.root}
    batch = sealed.batch_id
    if mode == "missing":
        stores = {}
    elif mode == "unknown":
        batch = "sha256:" + "0" * 64
    elif mode == "ambiguous":
        copied = tmp_path / "second"
        shutil.copytree(store.root, copied)
        stores["second-store"] = copied
    else:
        batch = "sha256:../../outside"
    with pytest.raises(ValueError, match="configured"):
        FrozenSources.configured(stores, batch)
