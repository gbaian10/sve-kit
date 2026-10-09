"""Shared command readers still reject changed bytes and mismatched input pins."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.products import FrozenProducts
from sve_carddb.domains.text_observations import FrozenTexts
from sve_carddb.ingest.archive.frozen_sources import FrozenBatches
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import ArchiveError, Scope, seal_batch
from sve_carddb.parse.pages import official_en, official_jp

from .test_effect_presence import page
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.ingest.archive.frozen_sources import FrozenSources
    from sve_carddb.ingest.archive.source_archive import ArchiveStore


@pytest.fixture
def command_sources(
    tmp_path: Path,
) -> tuple[ArchiveStore, FrozenBatches, FrozenSources]:
    store = _store(tmp_path / "sources")
    for region in (SourceRegion.EN, SourceRegion.JP):
        url = (official_en if region == SourceRegion.EN else official_jp).card_url(
            "SYN-01"
        )
        raw = page("en" if region == SourceRegion.EN else "jp")
        _put(
            store,
            replace(
                _resource(url, f"raw/{region.value}.html", raw, Kind.CARD),
                region=region,
            ),
            raw,
        )
    batch = seal_batch(store, scope=(Scope(provider="en", kind="card"),))
    readers = FrozenBatches()
    return store, readers, readers.batch(store.root, store.store_id, batch.batch_id)


@pytest.mark.parametrize("changed", ["descriptor", "receipt", "raw"])
@pytest.mark.parametrize("adapter", ["texts", "products"])
def test_shared_adapters_recheck_every_source_read(
    command_sources: tuple[ArchiveStore, FrozenBatches, FrozenSources],
    changed: str,
    adapter: str,
) -> None:
    store, readers, sources = command_sources
    texts = FrozenTexts(
        store.root,
        store.store_id,
        sources.batch_id,
        region="en",
        parser_version="official-en-exact-v1",
        sources=readers.configured({store.store_id: store.root}, sources.batch_id),
    )
    products = FrozenProducts(
        store.root, store.store_id, sources.batch_id, region="en", sources=sources
    )
    assert texts.card("en", "SYN-01") is not None
    assert len(products.pages()) == 1
    entry = sources.inventory.entries[0]
    if changed == "descriptor":
        target = store.root / "descriptors" / (entry.descriptor_sha256[7:] + ".json")
    elif changed == "receipt":
        descriptor = sources.descriptor(entry.source_version_id)
        target = store.root / "receipts" / (descriptor.first_receipt_id[7:] + ".json")
    else:
        target = store.root / entry.blob.path
    target.write_bytes(b"Synthetic changed source")
    read = (
        (lambda: texts.card("en", "SYN-01")) if adapter == "texts" else products.pages
    )
    with pytest.raises(ArchiveError, match="content hash mismatch"):
        read()


def test_shared_reader_rejects_version_from_another_batch(
    command_sources: tuple[ArchiveStore, FrozenBatches, FrozenSources],
) -> None:
    store, readers, en = command_sources
    jp_batch = seal_batch(store, scope=(Scope(provider="jp", kind="card"),))
    jp = readers.batch(store.root, store.store_id, jp_batch.batch_id)
    with pytest.raises(ArchiveError, match="absent from pinned batch"):
        en.read(jp.inventory.entries[0].source_version_id, parser_version="synthetic")


@pytest.mark.parametrize("mismatch", ["root", "store", "batch"])
def test_adapter_cannot_borrow_reader_from_another_input_pin(
    command_sources: tuple[ArchiveStore, FrozenBatches, FrozenSources], mismatch: str
) -> None:
    store, _, sources = command_sources
    with pytest.raises(ValueError, match="differs from configured input"):
        FrozenTexts(
            store.root / "other" if mismatch == "root" else store.root,
            "other-store" if mismatch == "store" else store.store_id,
            "sha256:" + "0" * 64 if mismatch == "batch" else sources.batch_id,
            region="en",
            parser_version="official-en-exact-v1",
            sources=sources,
        )


def test_opened_batch_does_not_authorize_a_different_configured_store(
    command_sources: tuple[ArchiveStore, FrozenBatches, FrozenSources],
) -> None:
    store, readers, sources = command_sources
    with pytest.raises(ArchiveError, match="invalid archive blob locator"):
        readers.configured({"other-store": store.root}, sources.batch_id)
