"""Independent counterexamples for conservative announcement evidence."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.card_extras.errata_archive import FrozenErrataNotices
from sve_carddb.card_extras.errata_markup import NoticeMarkup
from sve_carddb.card_extras.errata_parser import associate_blocks, parse_notice
from sve_carddb.core.json import digest
from sve_carddb.manifest import Kind
from sve_carddb.manifest import Region as ManifestRegion
from sve_carddb.source_archive import seal_batch

from .test_errata_parser import page
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from sve_carddb.source_archive import ArchiveStore


def pair() -> str:
    return "<p>(Incorrect)</p><p>Old synthetic.</p><p>↓</p><p>(Correct)</p><p>New synthetic.</p>"


@pytest.mark.parametrize("context", ["TEST-001", "Synthetic first"])
def test_finished_pair_does_not_reuse_card_context(context: str) -> None:
    raw, pin = page(content=f"<p>{context}</p>{pair()}<p>&nbsp;</p>{pair()}")
    notice = parse_notice(raw, pin, region="en")
    assert notice.issues == ()
    assert notice.blocks[1].context == ()
    result = associate_blocks(notice, {"TEST-001": "Synthetic first"})
    assert [(item.card_no, item.block_ordinal) for item in result.matched] == [
        ("TEST-001", 0)
    ]
    assert result.unassigned_blocks == (1,)


def test_incorrect_marker_truncation_is_an_issue_and_prevents_all_associations() -> (
    None
):
    raw, pin = page(content=f"<p>TEST-001</p>{pair()}<p>TEST-002</p>{pair()}")
    notice = parse_notice(raw, pin, region="en")
    assert notice.issues == ("pair_truncated_by_incorrect_marker",)
    assert notice.blocks[0].end_basis == "next_incorrect_marker"
    assert notice.blocks[1].context == ()
    assert associate_blocks(notice, {}).matched == ()


def test_empty_name_never_supplies_explicit_context() -> None:
    raw, pin = page(content=pair())
    result = associate_blocks(parse_notice(raw, pin, region="en"), {"TEST-001": ""})
    assert result.matched == ()
    assert result.unassigned_cards == ("TEST-001", "TEST-002")


def test_multiple_shared_blocks_are_not_a_cartesian_product() -> None:
    raw, pin = page(
        content=f"<section class=inh1>Changes</section>{pair()}<p>&nbsp;</p><section class=inh1>Changes</section>{pair()}<p>&nbsp;</p>"
    )
    notice = parse_notice(raw, pin, region="en")
    assert notice.issues == ()
    assert len(notice.blocks) == 2
    result = associate_blocks(notice, {})
    assert result.matched == ()
    assert result.unassigned_blocks == (0, 1)


@pytest.mark.parametrize("context", ["Changes", "TEST-001"])
def test_notice_issues_prevent_shared_and_explicit_associations(context: str) -> None:
    raw, pin = page(content=f"<p>↓</p><p>{context}</p>{pair()}<p>&nbsp;</p>")
    notice = parse_notice(raw, pin, region="en")
    assert notice.issues == ("orphan_change_arrow",)
    assert associate_blocks(notice, {}).matched == ()


def test_card_number_substring_is_not_explicit_context() -> None:
    raw, pin = page(content=f"<p>TEST-001 additional synthetic words</p>{pair()}")
    assert associate_blocks(parse_notice(raw, pin, region="en"), {}).matched == ()


def test_heading_replaces_previous_card_context_before_a_pair() -> None:
    raw, pin = page(
        content=f"<p>TEST-001</p><section class=inh1>Unknown heading</section>{pair()}"
    )
    notice = parse_notice(raw, pin, region="en")
    assert notice.blocks[0].context == ("Unknown heading",)
    assert associate_blocks(notice, {}).matched == ()


@pytest.mark.parametrize(
    ("label", "field"),
    [("カード名", "name"), ("タイプ", "traits"), ("カード種類", "card_type")],
)
def test_jp_labels_match_the_main_extractor(label: str, field: str) -> None:
    raw, pin = page(
        "jp",
        content=f"<p>▼修正内容</p><p>(誤)</p><p>{label}：合成旧值</p><p>↓</p><p>(正)</p><p>{label}：合成新值</p>",
    )
    block = parse_notice(raw, pin, region="jp").blocks[0]
    assert block.before.field_label == block.after.field_label == field
    assert (
        raw[block.after.raw.start : block.after.raw.end]
        == f"{label}：合成新值".encode()
    )


@pytest.mark.parametrize(
    "label", ["Traits", "Trait", "Effect", "Cost", "Attack", "Defense"]
)
def test_unobserved_en_labels_remain_uninterpreted(label: str) -> None:
    raw, pin = page(
        content=f"<p>(Incorrect)</p><p>{label}: Old</p><p>↓</p><p>(Correct)</p><p>{label}: New</p>"
    )
    block = parse_notice(raw, pin, region="en").blocks[0]
    assert block.before.field_label is block.after.field_label is None


@pytest.mark.parametrize("label", [" Card Name:", "<strong>Card Name</strong>:"])
def test_label_layout_outside_the_supported_prefix_is_preserved(label: str) -> None:
    raw, pin = page(
        content=f"<p>(Incorrect)</p><p>{label} Old synthetic.</p><p>↓</p><p>(Correct)</p><p>{label} New synthetic.</p>"
    )
    block = parse_notice(raw, pin, region="en").blocks[0]
    assert block.before.field_label is block.after.field_label is None
    assert "Card Name" in "".join(piece.value for piece in block.after.pieces)


def test_inconsistent_field_labels_are_reported_and_not_associated() -> None:
    raw, pin = page(
        content="<p>TEST-001</p><p>(Incorrect)</p><p>Card Name: Old</p><p>↓</p><p>(Correct)</p><p>New</p>"
    )
    notice = parse_notice(raw, pin, region="en")
    assert notice.issues == ("inconsistent_field_label",)
    assert associate_blocks(notice, {}).matched == ()


@pytest.mark.parametrize(
    "url",
    [
        "https://en.shadowverse-evolve.com/errata/synthetic/?unexpected=1",
        "https://en.shadowverse-evolve.com/errata/synthetic/#unexpected",
        "https://en.shadowverse-evolve.com/cards/synthetic/",
        "http://en.shadowverse-evolve.com/errata/synthetic/",
    ],
)
def test_source_url_components_are_individually_rejected(url: str) -> None:
    raw, pin = page()
    with pytest.raises(ValueError, match=r"^Errata source URL/region/media mismatch$"):
        parse_notice(raw, pin.model_copy(update={"url": url}), region="en")


def test_raw_duplicate_container_outside_dom_announcement_is_rejected() -> None:
    raw, pin = page()
    raw += b"<div class='contents sw-Txtarea'><p>Unrelated</p></div>"
    with pytest.raises(ValueError, match=r"^Errata raw body container is ambiguous$"):
        parse_notice(raw, pin.model_copy(update={"sha256": digest(raw)}), region="en")


def test_contents_without_textarea_is_not_lexical_evidence() -> None:
    raw, pin = page()
    raw += f"<div class=contents>{pair()}</div>".encode()
    notice = parse_notice(
        raw, pin.model_copy(update={"sha256": digest(raw)}), region="en"
    )
    assert notice.issues == ()
    assert len(notice.blocks) == 1


def test_ordinary_blank_lines_and_closing_tags_preserve_fragment_layout() -> None:
    raw, pin = page(
        content="<p>(Incorrect)</p><p>Old synthetic.</p>Old continuation<p>↓</p><p>(Correct)</p><p>New synthetic.</p><p> </p><p>New continuation</p><p>&nbsp;</p>"
    )
    block = parse_notice(raw, pin, region="en").blocks[0]
    assert (
        "".join(p.value for p in block.before.pieces)
        == "Old synthetic.\nOld continuation"
    )
    assert (
        raw[block.after.raw.start : block.after.raw.end]
        == b"New synthetic.</p><p> </p><p>New continuation"
    )
    assert block.end_basis == "nbsp_separator"


def test_orphan_correct_discards_its_phase_instead_of_continuing_a_pair() -> None:
    raw, pin = page(content=pair() + "<p>(Correct)</p><p>Orphan text.</p><p>↓</p>")
    notice = parse_notice(raw, pin, region="en")
    assert notice.issues == ("orphan_change_arrow", "orphan_correct_marker")
    assert len(notice.blocks) == 1
    assert notice.blocks[0].end_basis == "orphan_correct_marker"
    assert associate_blocks(notice, {}).matched == ()


def test_missing_announcement_title_is_not_an_empty_title() -> None:
    raw, pin = page()
    raw = raw.replace(b"<h1 class=ttl>Synthetic notice</h1>", b"")
    with pytest.raises(
        ValueError, match=r"^Errata layout requires exactly one \.heading > h1\.ttl$"
    ):
        parse_notice(raw, pin.model_copy(update={"sha256": digest(raw)}), region="en")


def test_staging_reports_end_basis_and_unused_trailing_lines() -> None:
    raw, pin = page(
        content=f"<p>TEST-001</p>{pair()}<p>&nbsp;</p><p>Unused synthetic note</p><p>Second unused line</p>"
    )
    notice = parse_notice(raw, pin, region="en")
    assert notice.blocks[0].end_basis == "nbsp_separator"
    assert notice.unused_trailing_lines == 2
    raw, pin = page(content=pair())
    assert parse_notice(raw, pin, region="en").blocks[0].end_basis == "end_of_document"
    raw, pin = page(content=f"{pair()}<section class=inh1>Later heading</section>")
    notice = parse_notice(raw, pin, region="en")
    assert notice.blocks[0].end_basis == "heading"
    assert notice.unused_trailing_lines == 1
    raw, pin = page(
        content="<p>Earlier unused line</p><section class=inh1>Later heading</section>"
    )
    notice = parse_notice(raw, pin, region="en")
    assert notice.blocks == ()
    assert notice.unused_trailing_lines == 1


def test_heading_card_image_is_not_a_shared_scope_proof() -> None:
    raw, pin = page(
        content=f"<section class=inh1>Changes</section><p><img src='/synthetic.png'></p>{pair()}"
    )
    assert associate_blocks(parse_notice(raw, pin, region="en"), {}).matched == ()


def test_entity_terminator_is_part_of_the_fixed_raw_slice() -> None:
    raw, pin = page(
        content="<p>(Incorrect)</p><p>Old synthetic.</p><p>↓</p><p>(Correct)</p><p>New synthetic &amp;</p>"
    )
    fragment = parse_notice(raw, pin, region="en").blocks[0].after
    assert raw[fragment.raw.start : fragment.raw.end] == b"New synthetic &amp;"
    assert fragment.raw.sha256 == digest(b"New synthetic &amp;")


def test_lexical_void_and_self_closing_tags_keep_fixed_original_ranges() -> None:
    raw, pin = page(
        content=f"<p/><area><base><col><embed><param><source><track>{pair()}"
    )
    block = parse_notice(raw, pin, region="en").blocks[0]
    assert raw[block.after.raw.start : block.after.raw.end] == b"New synthetic."
    markup = NoticeMarkup(
        "<div class='contents sw-Txtarea'><area><base><col><embed><param><source><track>Text"
    )
    markup.feed(markup.html)
    assert [tag for tag, _, _ in markup.stack] == ["div"]
    markup.feed("</div>")
    assert markup.stack == []


@pytest.fixture(scope="module")
def wrong_kind_batch(
    tmp_path_factory: pytest.TempPathFactory,
) -> tuple[ArchiveStore, str]:
    store = _store(tmp_path_factory.mktemp("errata-wrong-kind"))
    raw, pin = page()
    resource = replace(
        _resource(pin.url, "raw/notice.html", raw, Kind.RULES), region=ManifestRegion.EN
    )
    _put(store, resource, raw)
    return store, seal_batch(store).batch_id


def test_archive_rejects_non_errata_kind(
    wrong_kind_batch: tuple[ArchiveStore, str],
) -> None:
    store, batch = wrong_kind_batch
    with pytest.raises(ValueError, match=r"^Errata batch source identity mismatch$"):
        tuple(
            FrozenErrataNotices(
                store.root, store.store_id, batch, region="en"
            ).notices()
        )


@pytest.fixture(scope="module")
def historical_batch(
    tmp_path_factory: pytest.TempPathFactory,
) -> tuple[ArchiveStore, str]:
    store = _store(tmp_path_factory.mktemp("errata-history"))
    batches: list[str] = []
    for value in ("Oct. 03, 2026", "Oct. 04, 2026"):
        raw, pin = page(date=value)
        resource = replace(
            _resource(pin.url, "raw/notice.html", raw, Kind.ERRATA),
            region=ManifestRegion.EN,
        )
        _put(store, resource, raw)
        batches.append(seal_batch(store).batch_id)
    return store, batches[-1]


def test_archive_retains_every_sealed_historical_version(
    historical_batch: tuple[ArchiveStore, str],
) -> None:
    store, batch = historical_batch
    notices = tuple(
        FrozenErrataNotices(store.root, store.store_id, batch, region="en").notices()
    )
    assert len(notices) == 2
    assert {notice.heading_date for notice in notices} == {"2026-10-03", "2026-10-04"}
    assert len({notice.source.id for notice in notices}) == 2
