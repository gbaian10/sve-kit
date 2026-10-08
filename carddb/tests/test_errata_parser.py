"""Invented notices exercise evidence boundaries without official wording."""

import shutil
from dataclasses import replace
from typing import TYPE_CHECKING, cast

import httpx
import pytest

from sve_carddb.card_extras.errata_archive import FrozenErrataNotices
from sve_carddb.card_extras.errata_parser import PARSER, associate_blocks, parse_notice
from sve_carddb.core.json import digest
from sve_carddb.manifest import Kind, Manifest
from sve_carddb.manifest import Region as ManifestRegion
from sve_carddb.source_archive import ArchiveError, seal_batch

from .card_extras_fixtures import source
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.provenance import Source
    from sve_carddb.registry.records import Region
    from sve_carddb.source_archive import ArchiveStore


def page(
    region: Region = "en", *, content: str | None = None, date: str = "Oct. 03, 2026"
) -> tuple[bytes, Source]:
    prefix = "errata" if region == "en" else "eratta"
    labels = (
        ("Card Number(s)", "Card Name")
        if region == "en"
        else ("カード番号", "カード名")
    )
    heading = (
        "<section class=inh1>Changes</section>"
        if region == "en"
        else "<p>▼修正内容</p>"
    )
    old, new = ("(Incorrect)", "(Correct)") if region == "en" else ("(誤)", "(正)")
    body = (
        content
        if content is not None
        else f"{heading}<p>{old}<br>Earlier synthetic text.</p><p>↓<br>{new}<br>Correct synthetic <span style='color:red'>文字 &amp; text</span>.</p><p>&nbsp;</p><p>Unrelated trailing prose.</p>"
    )
    html = f"""<html><body><div class=st-Container><div class=st-Container_Inner><div class=sw-Lower><div class=sw-Lower_Wrapper><div class=sw-Lower_Container><div class={prefix}-Detail><div class={prefix}-Detail_Inner>
    <div class=heading><h1 class=ttl>Synthetic notice</h1><div class=heading-top><time class='time Sans'>{date}</time></div></div>
    <div class=sw-Info><dl><dt>{labels[0]}</dt><dd>TEST-001、TEST-002</dd></dl><dl><dt>{labels[1]}</dt><dd>Synthetic names</dd></dl></div><div class='contents sw-Txtarea'>{body}</div>
    </div></div></div></div></div></div></div></body></html>"""
    raw = html.encode()
    host = "en.shadowverse-evolve.com" if region == "en" else "shadowverse-evolve.com"
    return raw, source(raw=raw, region=region).model_copy(
        update={"url": f"https://{host}/errata/synthetic/", "parser_version": PARSER}
    )


@pytest.mark.parametrize("region", ["jp", "en"])
def test_notice_raw_evidence_and_scope(region: Region) -> None:
    raw, pin = page(region, date="2026.10.03" if region == "jp" else "Oct. 03, 2026")
    notice = parse_notice(raw, pin, region=region)
    assert notice.heading_date == "2026-10-03"
    assert notice.listed.numbers == ("TEST-001", "TEST-002")
    assert notice.issues == ()
    assert len(notice.blocks) == 1
    block = notice.blocks[0]
    assert "Unrelated" not in "".join(piece.value for piece in block.after.pieces)
    assert any(piece.spans for piece in block.after.pieces)
    expected = (
        b"Earlier synthetic text.",
        "Correct synthetic <span style='color:red'>文字 &amp; text</span>.".encode(),
    )
    for fragment, content in zip((block.before, block.after), expected, strict=True):
        assert raw[fragment.raw.start : fragment.raw.end] == content
        assert fragment.raw.sha256 == digest(content)
    assert block.end_basis == "nbsp_separator"
    assert notice.unused_trailing_lines == 1
    assert "Correct synthetic" not in repr(notice)
    associations = associate_blocks(notice, {})
    assert [(item.card_no, item.basis) for item in associations.matched] == [
        ("TEST-001", "single_shared_block"),
        ("TEST-002", "single_shared_block"),
    ]
    assert associations.unassigned_cards == ()


def test_inline_nested_markup_entities_and_images_remain_evidence() -> None:
    body = """<p>▼修正内容</p><p><strong>(誤)</strong></p><div class='”box”'>Old<br>next <img src='/icon.png' alt='Synthetic icon'><p>↓<br><strong>(正)</strong><br>New &amp; <span>文字</span><img src='/icon2.png' alt='Other synthetic icon'></p></div>"""
    raw, pin = page("jp", content=body)
    notice = parse_notice(raw, pin, region="jp")
    block = notice.blocks[0]
    assert any(piece.kind == "image" for piece in block.before.pieces)
    assert any(piece.kind == "break" for piece in block.before.pieces)
    assert "New & 文字" in "".join(piece.value for piece in block.after.pieces)
    assert notice.images == (
        ("https://shadowverse-evolve.com/icon.png", "Synthetic icon"),
        ("https://shadowverse-evolve.com/icon2.png", "Other synthetic icon"),
    )


def test_field_label_is_transcribed_without_rewriting_raw_evidence() -> None:
    raw, pin = page(
        content="<section class=inh1>Changes</section><p>(Incorrect)</p><p>Card Name: Old synthetic name</p><p>↓</p><p>(Correct)</p><p>Card Name: New synthetic name</p>"
    )
    block = parse_notice(raw, pin, region="en").blocks[0]
    assert block.before.field_label == block.after.field_label == "name"
    assert "Card Name:" not in "".join(piece.value for piece in block.after.pieces)
    assert b"Card Name:" in raw[block.after.raw.start : block.after.raw.end]


def test_multiple_blocks_require_explicit_card_context() -> None:
    body = "<section class=inh1>Changes</section><p>TEST-001</p><p>(Incorrect)</p><p>Old one.</p><p>↓</p><p>(Correct)</p><p>New one.</p><p>&nbsp;</p><p>Synthetic second</p><p>(Incorrect)</p><p>Old two.</p><p>↓</p><p>(Correct)</p><p>New two.</p>"
    raw, pin = page(content=body)
    notice = parse_notice(raw, pin, region="en")
    result = associate_blocks(notice, {"TEST-002": "Synthetic second"})
    assert [
        (item.card_no, item.block_ordinal, item.basis) for item in result.matched
    ] == [("TEST-001", 0, "explicit_number"), ("TEST-002", 1, "explicit_name")]
    assert result.unassigned_blocks == ()
    assert result.unassigned_cards == ()
    unnamed = associate_blocks(notice, {})
    assert unnamed.unassigned_cards == ("TEST-002",)
    assert unnamed.unassigned_blocks == (1,)


def test_ambiguous_names_and_unknown_scope_never_use_wording_to_assign_cards() -> None:
    body = "<section class=inh1>Synthetic ambiguous</section><p>(Incorrect)</p><p>Old.</p><p>↓</p><p>(Correct)</p><p>Correct.</p>"
    raw, pin = page(content=body)
    notice = parse_notice(raw, pin, region="en")
    assert (
        associate_blocks(
            notice,
            {"TEST-001": "Synthetic ambiguous", "TEST-002": "Synthetic ambiguous"},
        ).matched
        == ()
    )
    assert associate_blocks(notice, {}).matched == ()


@pytest.mark.parametrize(
    ("body", "issues"),
    [
        ("<p>Synthetic image-only description.</p>", ("no_paired_changes",)),
        (
            "<p>(Incorrect)</p><p>Old.</p>",
            ("missing_correct_marker", "no_paired_changes"),
        ),
        ("<p>(Correct)</p><p>New.</p>", ("no_paired_changes", "orphan_correct_marker")),
    ],
)
def test_unrecognized_notice_is_reported_not_empty_success(
    body: str, issues: tuple[str, ...]
) -> None:
    raw, pin = page(content=body)
    notice = parse_notice(raw, pin, region="en")
    assert notice.blocks == ()
    assert notice.issues == issues


@pytest.mark.parametrize(
    ("content", "message"),
    [
        (
            "<p>(Incorrect)</p><p>↓</p><p>(Correct)</p><p>New.</p>",
            "Errata pair has an empty side",
        ),
        (
            "<p>(Incorrect)</p><p>Old.</p><p>(Correct)</p><p>New.</p>",
            "Errata pair requires exactly one change arrow",
        ),
        (
            "<p>(Incorrect)</p><p>Old.</p><p>↓</p><p>↓</p><p>(Correct)</p><p>New.</p>",
            "Errata pair requires exactly one change arrow",
        ),
    ],
)
def test_broken_pair_is_rejected(content: str, message: str) -> None:
    raw, pin = page(content=content)
    with pytest.raises(ValueError, match="^" + message + "$"):
        parse_notice(raw, pin, region="en")


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("hash", "Errata raw hash/parser pin mismatch"),
        ("parser", "Errata raw hash/parser pin mismatch"),
        ("host", "Errata source URL/region/media mismatch"),
        ("media", "Errata source URL/region/media mismatch"),
        ("container", "Errata announcement container is missing or ambiguous"),
        ("duplicate", "Errata announcement container is missing or ambiguous"),
        ("cards", "Unrecognized errata card-number list"),
    ],
)
def test_notice_boundaries(fault: str, message: str) -> None:
    raw, pin = page()
    if fault in {"hash", "parser", "host", "media"}:
        update = {
            "hash": {"sha256": "sha256:" + "0" * 64},
            "parser": {"parser_version": "other"},
            "host": {"url": "https://shadowverse-evolve.com/errata/synthetic/"},
            "media": {"kind": "official_pdf"},
        }[fault]
        pin = pin.model_copy(update=update)
    else:
        raw = (
            raw.replace(b"errata-Detail_Inner", b"unrecognized")
            if fault == "container"
            else raw + raw
            if fault == "duplicate"
            else raw.replace(b"TEST-001", b"unknown range")
        )
        pin = pin.model_copy(update={"sha256": digest(raw)})
    with pytest.raises(ValueError, match="^" + message + "$"):
        parse_notice(raw, pin, region="en")


@pytest.mark.parametrize("value", ["Unknown date", "2026.02.30", "2026/10/03"])
def test_unparsed_heading_date_remains_raw(value: str) -> None:
    raw, pin = page("jp", date=value)
    notice = parse_notice(raw, pin, region="jp")
    assert notice.heading_date is None
    assert notice.heading_date_raw == value


@pytest.fixture(scope="module")
def sealed(tmp_path_factory: pytest.TempPathFactory) -> tuple[ArchiveStore, str]:
    store = _store(tmp_path_factory.mktemp("errata-staging"))
    raw, pin = page()
    resource = replace(
        _resource(pin.url, "raw/notice.html", raw, Kind.ERRATA),
        region=ManifestRegion.EN,
    )
    _put(store, resource, raw)
    return store, seal_batch(store).batch_id


def test_archive_is_offline_and_corruption_is_rejected(
    sealed: tuple[ArchiveStore, str], monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    store, batch = sealed

    def forbidden(*_args: object, **_kwargs: object) -> None:
        raise AssertionError("No live access permitted")

    monkeypatch.setattr(Manifest, "open", forbidden)
    monkeypatch.setattr(Manifest, "open_live", forbidden)
    monkeypatch.setattr(httpx.Client, "send", forbidden)
    provider = FrozenErrataNotices(store.root, store.store_id, batch, region="en")
    (notice,) = provider.notices()
    assert notice.source.parser_version == PARSER
    assert notice.source.archive.batch_id == batch
    with pytest.raises(ValueError, match=r"^Errata batch source identity mismatch$"):
        tuple(
            FrozenErrataNotices(
                store.root, store.store_id, batch, region="jp"
            ).notices()
        )
    copied = tmp_path / "copied"
    shutil.copytree(store.root, copied)
    provider = FrozenErrataNotices(copied, store.store_id, batch, region="en")
    blob = copied / provider.sources.inventory.entries[0].blob.path
    blob.write_bytes(b"corrupted synthetic bytes")
    with pytest.raises(ArchiveError, match=r"^Frozen source content hash mismatch$"):
        tuple(provider.notices())


def test_archive_and_parser_reject_unknown_region(tmp_path: Path) -> None:
    invalid = cast("Region", "invented")
    with pytest.raises(
        ValueError, match=r"^Errata archive requires an explicit JP or EN region$"
    ):
        FrozenErrataNotices(tmp_path, "unused", "sha256:" + "0" * 64, region=invalid)
    raw, pin = page()
    with pytest.raises(
        ValueError, match=r"^Errata parser requires an explicit JP or EN region$"
    ):
        parse_notice(raw, pin, region=invalid)


@pytest.mark.parametrize(
    ("before", "after", "message"),
    [
        (
            "<dd>TEST-001、TEST-002</dd>",
            "<dd>TEST-001、TEST-001</dd>",
            "Unrecognized errata card-number list",
        ),
        (
            "<dt>Card Name</dt>",
            "<dt>Unexpected</dt>",
            "Errata card-information labels are missing",
        ),
        (
            "</dl></div><div class='contents",
            "</dl><dl><dt>Card Name</dt><dd>Duplicate</dd></dl></div><div class='contents",
            "Duplicate errata card-information label",
        ),
        ("<dt>Card Name</dt>", "", "Errata layout requires exactly one dt"),
        (
            "<time class='time Sans'>Oct. 03, 2026</time>",
            "<time class='time Sans'>Oct. 03, 2026</time><time class='time Sans'>Oct. 04, 2026</time>",
            "Ambiguous errata heading date",
        ),
    ],
)
def test_ambiguous_or_missing_metadata_is_rejected(
    before: str, after: str, message: str
) -> None:
    raw, pin = page()
    raw = raw.replace(before.encode(), after.encode())
    pin = pin.model_copy(update={"sha256": digest(raw)})
    with pytest.raises(ValueError, match="^" + message + "$"):
        parse_notice(raw, pin, region="en")


def test_no_heading_date_and_script_markers_are_not_guessed() -> None:
    raw, pin = page(
        content="<p><script>(Incorrect)</script></p><p>Old synthetic.</p><p>↓</p><p><style>(Correct)</style></p><p>New synthetic.</p>"
    )
    raw = raw.replace(b"<time class='time Sans'>Oct. 03, 2026</time>", b"")
    notice = parse_notice(
        raw, pin.model_copy(update={"sha256": digest(raw)}), region="en"
    )
    assert notice.heading_date is notice.heading_date_raw is None
    assert notice.blocks == ()
    assert notice.issues == ("no_paired_changes", "orphan_change_arrow")


def test_original_unicode_byte_ranges_survive_dom_repair_and_unrelated_dates() -> None:
    raw, pin = page(
        "jp",
        content="<p>▼修正内容</p><p>（誤）<br>合成旧值。</p><div><p>↓<br>（正）<br>合成<span>新值。</span>&#x21;<img src='/icon.png' alt='Synthetic'/></div>",
        date="2026.10.03",
    )
    notice = parse_notice(raw, pin, region="jp")
    fragment = notice.blocks[0].after
    expected = (
        "合成<span>新值。</span>&#x21;<img src='/icon.png' alt='Synthetic'/>".encode()
    )
    assert raw[fragment.raw.start : fragment.raw.end] == expected
    assert fragment.raw.sha256 == digest(expected)
    changed = raw.replace(b"2026.10.03", b"2026.10.04")
    again = parse_notice(
        changed, pin.model_copy(update={"sha256": digest(changed)}), region="jp"
    )
    assert again.blocks[0].after.raw.sha256 == fragment.raw.sha256
    assert "合成新值。!" in "".join(piece.value for piece in fragment.pieces)


def test_only_lf_advances_html_parser_line_positions() -> None:
    raw, pin = page(
        content="<section class=inh1>Changes</section>\r\n<p>\r\v\f</p>\r\n<p>(Incorrect)</p><p>Old synthetic.</p><p>↓</p><p>(Correct)</p><p>New synthetic.</p>"
    )
    notice = parse_notice(raw, pin, region="en")
    for fragment in (notice.blocks[0].before, notice.blocks[0].after):
        expected = (
            b"Old synthetic."
            if fragment is notice.blocks[0].before
            else b"New synthetic."
        )
        assert raw[fragment.raw.start : fragment.raw.end] == expected
        assert fragment.raw.sha256 == digest(expected)


@pytest.mark.parametrize("reference", ["&amp", "&#33"])
def test_unterminated_reference_does_not_hash_the_following_tag(reference: str) -> None:
    raw, pin = page(
        content=f"<section class=inh1>Changes</section><p>(Incorrect)</p><p>Old synthetic.</p><p>↓</p><p>(Correct)</p><p>New synthetic {reference}</p>"
    )
    fragment = parse_notice(raw, pin, region="en").blocks[0].after
    expected = ("New synthetic " + reference).encode()
    assert raw[fragment.raw.start : fragment.raw.end] == expected
    assert fragment.raw.sha256 == digest(expected)
