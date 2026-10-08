"""Regional supplemental parsing uses sealed input; fixtures contain invented text."""

import shutil
from dataclasses import replace
from typing import TYPE_CHECKING, cast

import httpx
import pytest

from sve_carddb.card_extras import FrozenCardExtras, parse_card_page
from sve_carddb.card_extras.archive import EN_PARSER, PARSER, card_number
from sve_carddb.core.json import digest
from sve_carddb.ingest.archive.manifest import Kind, Manifest
from sve_carddb.ingest.archive.manifest import Region as ManifestRegion
from sve_carddb.ingest.archive.source_archive import ArchiveError, seal_batch
from sve_carddb.parse.html import MissingElementError
from sve_carddb.parse.pages import official_en, official_jp

from .card_extras_fixtures import source
from .en_extract_fixtures import page as en_page
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.card_extras import CardPage
    from sve_carddb.core.provenance import Source
    from sve_carddb.ingest.archive.source_archive import ArchiveStore
    from sve_carddb.registry.records import Region


RAW = (
    b"""<!doctype html><html><body>
<div class="cardlist-Detail"><div class="ttl">Synthetic card name</div>
<div class="illustrator"><span class="name">TEST-001&#x24C8;a</span></div>
<div class="img"><img src="https://example.invalid/synthetic.png"></div>
<div class="illustrator"><a href="/errata/synthetic/">Synthetic notice</a></div></div>
<div class="cardlist-Under"><div class="cardlist-Detail_QA">
<div class="qa-List_Item"><div class="qa-List_Ttl">Q900000 (2026/10/1)</div>
<div class="qa-List_Txt-Q"><span class="Garamond">Q</span>Synthetic question?<br>Next <img alt="synthetic.icon"></div>
<div class="qa-List_Txt-A"><span class="Garamond">A</span>Synthetic answer.</div></div>
<div class="qa-List_Item" id="synthetic-anchor"><div class="qa-List_Ttl">2026/10/1</div>
<div class="qa-List_Txt-Q">Unnumbered synthetic?</div><div class="qa-List_Txt-A">Unnumbered answer.</div></div>
</div></div><div class="cardlist-Detail_Relation"><a href="?cardno=TEST-002">Synthetic target</a></div>
</body></html>"""
    + b" " * 3000
)

EN_RAW = (
    en_page("TEST-001Ⓢa")
    .replace(
        b'<div class="cardlist-Detail_QA">', b'</div><div class="cardlist-Detail_QA">'
    )
    .replace(
        b'</div></div></div><div class="cardlist-Detail_Relation">',
        b'</div></div><div class="cardlist-Detail_Relation">',
    )
    .replace(b"Synthetic QA title", b"Q900001 (Oct. 10, 2026)")
)


def regional_page(region: Region) -> tuple[bytes, Source]:
    raw = RAW if region == "jp" else EN_RAW
    pin = source(region=region, raw=raw).model_copy(
        update={"parser_version": PARSER if region == "jp" else EN_PARSER}
    )
    return raw, pin


@pytest.mark.parametrize("region", ["jp", "en"])
def test_regional_qa_container(region: Region) -> None:
    raw, pin = regional_page(region)
    parsed = parse_card_page(raw, pin, region=region)
    assert parsed.region == region
    assert parsed.card_no == "TEST-001Ⓢa"
    assert len(parsed.qa) == (2 if region == "jp" else 1)
    assert parsed.qa[0].official_number == ("Q900000" if region == "jp" else "Q900001")
    assert parsed.qa[0].published_on == (
        "2026-10-01" if region == "jp" else "2026-10-10"
    )
    assert parsed.qa[0].question == (
        "Synthetic question?\nNext {synthetic.icon}" if region == "jp" else "Question"
    )
    assert parsed.qa[0].answer == (
        "Synthetic answer." if region == "jp" else "Answer\nMore"
    )


@pytest.mark.parametrize(
    ("raw_date", "expected"),
    [
        ("Jan. 12, 2026", "2026-01-12"),
        ("Feb. 12, 2026", "2026-02-12"),
        ("Mar. 12, 2026", "2026-03-12"),
        ("Apr. 12, 2026", "2026-04-12"),
        ("May. 12, 2026", "2026-05-12"),
        ("Jun. 12, 2026", "2026-06-12"),
        ("Jul. 12, 2026", "2026-07-12"),
        ("Aug. 12, 2026", "2026-08-12"),
        ("Sep. 12, 2026", "2026-09-12"),
        ("Oct. 12, 2026", "2026-10-12"),
        ("Nov. 12, 2026", "2026-11-12"),
        ("Dec. 12, 2026", "2026-12-12"),
        ("Oct. 1, 2026", "2026-10-01"),
        ("May 12, 2026", "2026-05-12"),
        ("Feb. 29, 2024", "2024-02-29"),
    ],
)
def test_en_qa_observed_date_shapes(raw_date: str, expected: str) -> None:
    raw, pin = regional_page("en")
    raw = raw.replace(b"Oct. 10, 2026", raw_date.encode())
    pin = pin.model_copy(update={"sha256": digest(raw)})
    entry = parse_card_page(raw, pin, region="en").qa[0]
    assert entry.published_on == expected
    assert entry.date_raw == raw_date


@pytest.mark.parametrize(
    "raw_date",
    [
        "May 1, 2026",
        "October 12, 2026",
        "Sept. 12, 2026",
        "Abc. 12, 2026",
        "oct. 12, 2026",
        "Oct.  12, 2026",
        "Oct. 12,26",
        "Oct. 12, 2026 trailing",
        "2026/10/1",
        "Oct. 0, 2026",
        "Apr. 31, 2026",
        "Feb. 29, 2025",
        "Jan. 12, 0000",
        "Unknown date",
    ],
)
def test_en_qa_unknown_date_remains_raw(raw_date: str) -> None:
    raw, pin = regional_page("en")
    raw = raw.replace(b"Oct. 10, 2026", raw_date.encode())
    pin = pin.model_copy(update={"sha256": digest(raw)})
    entry = parse_card_page(raw, pin, region="en").qa[0]
    assert entry.published_on is None
    assert entry.date_raw == raw_date


def test_en_qa_absent_date_remains_unknown() -> None:
    raw, pin = regional_page("en")
    raw = raw.replace(b" (Oct. 10, 2026)", b"")
    pin = pin.model_copy(update={"sha256": digest(raw)})
    entry = parse_card_page(raw, pin, region="en").qa[0]
    assert entry.published_on is None
    assert entry.date_raw is None


def test_jp_does_not_adopt_en_date_format() -> None:
    raw = RAW.replace(b"2026/10/1", b"Oct. 10, 2026")
    entry = parse_card_page(raw, source(raw=raw)).qa[0]
    assert entry.published_on is None
    assert entry.date_raw == "Oct. 10, 2026"


@pytest.mark.parametrize("region", ["jp", "en"])
@pytest.mark.parametrize("fault", ["container", "stray-block"])
def test_regional_qa_rejects_unrecognized_layout(region: Region, fault: str) -> None:
    raw, pin = regional_page(region)
    if fault == "container":
        raw = raw.replace(b'class="cardlist-Detail_QA"', b'class="unknown"')
    else:
        raw += b'<div class="qa-List_Item">Synthetic unrelated block</div>'
    pin = pin.model_copy(update={"sha256": digest(raw)})
    with pytest.raises(ValueError, match=r"^Unrecognized card-page Q&A layout$"):
        parse_card_page(raw, pin, region=region)


def test_jp_rejects_en_style_qa_without_jp_wrapper() -> None:
    raw = RAW.replace(b'class="cardlist-Under"', b'class="synthetic-en-wrapper"')
    with pytest.raises(ValueError, match=r"^Unrecognized card-page Q&A layout$"):
        parse_card_page(raw, source(raw=raw))


@pytest.mark.parametrize("region", ["jp", "en"])
@pytest.mark.parametrize("node", ["qa-List_Ttl", "qa-List_Txt-Q", "qa-List_Txt-A"])
def test_regional_qa_missing_node_is_not_empty(region: Region, node: str) -> None:
    raw, pin = regional_page(region)
    raw = raw.replace(f'class="{node}"'.encode(), b'class="missing"')
    pin = pin.model_copy(update={"sha256": digest(raw)})
    with pytest.raises(MissingElementError, match=f"^no element matches '\\.{node}'$"):
        parse_card_page(raw, pin, region=region)


@pytest.mark.parametrize("region", ["jp", "en"])
@pytest.mark.parametrize("fault", ["url", "parser"])
def test_regional_qa_rejects_cross_region_pin(region: Region, fault: str) -> None:
    raw, pin = regional_page(region)
    other_region: Region = "en" if region == "jp" else "jp"
    if fault == "url":
        pin = pin.model_copy(update={"url": source(region=other_region).url})
        message = "^Card extras source URL/region/media mismatch$"
    else:
        pin = pin.model_copy(
            update={"parser_version": EN_PARSER if region == "jp" else PARSER}
        )
        message = "^Card extras raw hash/parser pin mismatch$"
    with pytest.raises(ValueError, match=message):
        parse_card_page(raw, pin, region=region)


@pytest.fixture(scope="module")
def parsed() -> CardPage:
    return parse_card_page(RAW, source(raw=RAW))


class TestParsedPage:
    def test_exact_number_and_qa(self, parsed: CardPage) -> None:
        assert parsed.card_no == "TEST-001Ⓢa"
        assert parsed.qa[0].official_number == "Q900000"
        assert parsed.qa[0].published_on == "2026-10-01"
        assert parsed.qa[0].updated_on is None
        assert parsed.qa[0].question == "Synthetic question?\nNext {synthetic.icon}"
        assert parsed.qa[0].answer == "Synthetic answer."

    def test_unnumbered_anchor(self, parsed: CardPage) -> None:
        assert parsed.qa[1].official_number is None
        assert parsed.qa[1].stable_source_key == source().url + "#synthetic-anchor"

    def test_reference_preservation(self, parsed: CardPage) -> None:
        assert parsed.related[0].href_raw == "?cardno=TEST-002"
        assert parsed.errata_urls == (
            "https://shadowverse-evolve.com/errata/synthetic/",
        )


@pytest.mark.parametrize(
    ("before", "after"), [("2026/10/1", "2026/2/30"), ("2026/10/1", "Unknown date")]
)
def test_unknown_dates_remain_raw(before: str, after: str) -> None:
    raw = RAW.replace(before.encode(), after.encode())
    parsed = parse_card_page(raw, source(raw=raw))
    assert parsed.qa[0].published_on is None
    assert parsed.qa[0].date_raw == after


@pytest.mark.parametrize(
    "fault", ["hash", "parser", "region", "media", "identity", "layout"]
)
def test_parser_rejects_wrong_boundary(fault: str) -> None:
    raw = RAW
    original = source(raw=raw)
    if fault == "hash":
        original = original.model_copy(update={"sha256": "sha256:" + "0" * 64})
    elif fault == "parser":
        original = original.model_copy(update={"parser_version": "wrong"})
    elif fault == "region":
        original = source(region="en", raw=raw)
    elif fault == "media":
        original = original.model_copy(update={"kind": "official_pdf"})
    elif fault == "identity":
        original = source("TEST-002", raw=raw)
    else:
        raw = raw.replace(b'class="cardlist-Under"', b'class="unknown"')
        original = original.model_copy(update={"sha256": digest(raw)})
    with pytest.raises(
        ValueError, match=r"hash/parser|URL/region/media|page shows|layout"
    ):
        parse_card_page(raw, original)


def test_number_resolution_rejects_duplicates_and_cross_region() -> None:
    assert card_number(official_jp.card_url("TEST-001Ⓢa"), "jp") == "TEST-001Ⓢa"
    assert card_number(official_jp.card_url("TEST-001Ⓢa"), "en") is None
    assert (
        card_number(official_jp.card_url("TEST-001Ⓢa") + "&cardno=other", "jp") is None
    )
    assert card_number("https://example.invalid/?cardno=", "jp") is None


@pytest.fixture(scope="module")
def sealed(tmp_path_factory: pytest.TempPathFactory) -> tuple[ArchiveStore, str]:
    store = _store(tmp_path_factory.mktemp("extras-sealed"))
    _put(
        store,
        _resource(official_jp.card_url("TEST-001Ⓢa"), "raw/jp.html", RAW, Kind.CARD),
        RAW,
    )
    return store, seal_batch(store).batch_id


def test_frozen_adapter_uses_no_live_manifest_or_network(
    sealed: tuple[ArchiveStore, str], monkeypatch: pytest.MonkeyPatch
) -> None:
    store, batch = sealed

    def forbidden(*_args: object, **_kwargs: object) -> None:
        raise AssertionError("No live access permitted")

    monkeypatch.setattr(Manifest, "open", forbidden)
    monkeypatch.setattr(Manifest, "open_live", forbidden)
    monkeypatch.setattr(httpx.Client, "send", forbidden)
    provider = FrozenCardExtras(store.root, store.store_id, batch)
    pages = tuple(provider.pages())
    assert len(pages) == 1
    assert pages[0].source.archive.batch_id == batch
    assert pages[0].source.parser_version == PARSER
    assert pages[0].qa[0].official_number == "Q900000"


def test_corrupt_copy_of_sealed_input_is_rejected(
    sealed: tuple[ArchiveStore, str], tmp_path: Path
) -> None:
    store, batch = sealed
    copied = tmp_path / "copy"
    shutil.copytree(store.root, copied)
    provider = FrozenCardExtras(copied, store.store_id, batch)
    (copied / provider.sources.inventory.entries[0].blob.path).write_bytes(
        b"corrupted synthetic bytes"
    )
    with pytest.raises(ArchiveError, match="hash"):
        tuple(provider.pages())


@pytest.fixture(scope="module")
def sealed_en(tmp_path_factory: pytest.TempPathFactory) -> tuple[ArchiveStore, str]:
    store = _store(tmp_path_factory.mktemp("extras-en-sealed"))
    resource = replace(
        _resource(official_en.card_url("TEST-001Ⓢa"), "raw/en.html", EN_RAW, Kind.CARD),
        region=ManifestRegion.EN,
    )
    _put(store, resource, EN_RAW)
    return store, seal_batch(store).batch_id


def test_frozen_en_region_and_parser_are_explicit(
    sealed_en: tuple[ArchiveStore, str], monkeypatch: pytest.MonkeyPatch
) -> None:
    store, batch = sealed_en

    def forbidden(*_args: object, **_kwargs: object) -> None:
        raise AssertionError("No live access permitted")

    monkeypatch.setattr(Manifest, "open", forbidden)
    monkeypatch.setattr(Manifest, "open_live", forbidden)
    pages = tuple(
        FrozenCardExtras(store.root, store.store_id, batch, region="en").pages()
    )
    assert len(pages) == 1
    assert pages[0].region == "en"
    assert pages[0].source.parser_version == EN_PARSER
    assert pages[0].qa[0].published_on == "2026-10-10"
    with pytest.raises(
        ValueError, match=r"^Card extras batch source identity mismatch$"
    ):
        tuple(FrozenCardExtras(store.root, store.store_id, batch).pages())


def test_frozen_jp_does_not_accept_en_scope(sealed: tuple[ArchiveStore, str]) -> None:
    store, batch = sealed
    with pytest.raises(
        ValueError, match=r"^Card extras batch source identity mismatch$"
    ):
        tuple(FrozenCardExtras(store.root, store.store_id, batch, region="en").pages())


def test_archive_requires_an_explicit_supported_region(tmp_path: Path) -> None:
    with pytest.raises(
        ValueError, match=r"^Card extras require an explicit JP or EN region$"
    ):
        FrozenCardExtras(
            tmp_path, "unused", "sha256:" + "a" * 64, region=cast("Region", "invented")
        )
