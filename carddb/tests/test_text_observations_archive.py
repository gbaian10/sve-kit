"""Production adapters must transcribe sealed bytes independently in both regions."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build.source_rows import source_values
from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.text_observations import FrozenTexts, RegionalTexts
from sve_carddb.ingest.archive.manifest import Kind, Manifest
from sve_carddb.ingest.archive.source_archive import ArchiveError, seal_batch
from sve_carddb.parse.pages import official_en as en
from sve_carddb.parse.pages import official_jp as jp

from .en_extract_fixtures import page
from .test_registry_preview_archive import RAW
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path


def test_two_regions_keep_face_sections_source_pins_and_exact_numbers(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    store = _store(tmp_path)
    english = page(double=True)
    _put(store, _resource(jp.card_url("SYN-01a"), "raw/jp.html", RAW, Kind.CARD), RAW)
    _put(
        store,
        replace(
            _resource(en.card_url("SYNⓈ-01aEN"), "raw/en.html", english, Kind.CARD),
            region=SourceRegion.EN,
        ),
        english,
    )
    sealed = seal_batch(store)

    def forbidden(*_args: object, **_kwargs: object) -> None:
        raise AssertionError("No live access")

    monkeypatch.setattr(Manifest, "open", forbidden)
    monkeypatch.setattr(Manifest, "open_live", forbidden)
    provider = RegionalTexts(
        {
            "jp": FrozenTexts(
                store.root,
                store.store_id,
                sealed.batch_id,
                region="jp",
                parser_version="jp-text-pin",
            ),
            "en": FrozenTexts(
                store.root,
                store.store_id,
                sealed.batch_id,
                region="en",
                parser_version="en-text-pin",
            ),
        }
    )
    japanese = provider.card("jp", "SYN-01a")
    assert japanese is not None
    assert japanese.faces[0].effect == "Synthetic rule."
    assert japanese.faces[0].flavor is not None
    assert not japanese.faces[0].flavor
    assert japanese.source.parser_version == "jp-text-pin"
    english_card = provider.card("en", "SYNⓈ-01aEN")
    assert english_card is not None
    assert len(english_card.faces) == 2
    assert english_card.faces[0].effect == "First {synthetic.badge|[badge]}\nNext"
    assert english_card.faces[0].sections == ("Auxiliary", "Last")
    assert english_card.faces[0].stats == ("-", "02", "X")
    assert english_card.faces[0].title == "Synthetic universe"
    assert english_card.has_errata_link
    assert english_card.source.archive.batch_id == sealed.batch_id
    assert source_values(english_card.source)["parser_version"] is None
    assert provider.card("jp", "SYNⓈ-01aEN") is None
    assert provider.card("en", "SYN-01a") is None


@pytest.mark.parametrize("empty", [False, True])
def test_missing_node_and_present_empty_node_are_distinct(
    tmp_path: Path, empty: bool
) -> None:
    raw = RAW.replace(
        b'<div class="detail">Synthetic rule.</div>',
        b'<div class="detail"></div>' if empty else b"",
    )
    store = _store(tmp_path)
    _put(store, _resource(jp.card_url("SYN-01"), "raw/jp.html", raw, Kind.CARD), raw)
    sealed = seal_batch(store)
    found = FrozenTexts(
        store.root, store.store_id, sealed.batch_id, region="jp", parser_version="pin"
    ).card("jp", "SYN-01")
    assert found is not None
    assert found.faces[0].effect == ("" if empty else None)


def test_later_raw_tampering_is_not_silently_used(tmp_path: Path) -> None:
    store = _store(tmp_path)
    _put(store, _resource(jp.card_url("SYN-01"), "raw/jp.html", RAW, Kind.CARD), RAW)
    sealed = seal_batch(store)
    provider = FrozenTexts(
        store.root, store.store_id, sealed.batch_id, region="jp", parser_version="pin"
    )
    (store.root / sealed.inventory.entries[0].blob.path).write_bytes(b"Changed")
    with pytest.raises(ArchiveError, match="hash"):
        provider.card("jp", "SYN-01")
