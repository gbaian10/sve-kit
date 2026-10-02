"""Invented complete fields, disjoint reminder anchors and many-to-many NFKC."""

import re
import unicodedata
from dataclasses import replace

import pytest

from sve_carddb.template_parameters import provenance, spans
from sve_carddb.template_sources.normalizer import partition


def test_reminder_anchor_and_multirange_body_preserve_all_crlf_bytes() -> None:
    text = " \r\nA（Synthetic）B\n（Standalone）\n"
    parts = partition(text)
    located = spans.locate(text, parts)
    body = next(item for item in located if item.source_span.role == "body")
    assert len(body.source_span.segments) == 2
    reminders = [item for item in located if item.source_span.role == "reminder"]
    assert [item.source_span.anchor for item in reminders] == [body.ordinal, None]
    assert all(
        item.source_span.anchor is None
        for item in located
        if item.source_span.role != "reminder"
    )
    ordered = sorted(
        (segment for item in located for segment in item.source_span.segments),
        key=lambda segment: segment.start,
    )
    assert b"".join(text[s.start : s.end].encode() for s in ordered) == text.encode()
    assert spans.locate("", partition("")) == ()


def test_token_header_is_a_separate_unanchored_binding() -> None:
    text = "『Synthetic』{Neutral}フォロワー{コスト１}{攻撃力}２/{体力}３ Body\r\n"
    parts = partition(text, section=0)
    located = spans.locate(text, parts)
    header = next(item for item in located if item.source_span.role == "token_header")
    assert header.source_span.anchor is None
    assert header.source_span.segments[0].start == 0
    assert any(item.source_span.role == "body" for item in located)
    spans.verify(text, located)


@pytest.mark.parametrize("change", ["newline", "anchor", "line", "order", "literal"])
def test_span_verifier_rejects_independently_damaged_coverage_and_anchors(
    change: str,
) -> None:
    text = "A（Synthetic）B\r\nC"
    located = spans.locate(text, partition(text))
    if change == "newline":
        newline = next(
            item.ordinal
            for item in located
            if text[
                item.source_span.segments[0].start : item.source_span.segments[0].end
            ]
            == "\n"
        )
        damaged = tuple(
            replace(item, ordinal=index)
            for index, item in enumerate(
                item for item in located if item.ordinal != newline
            )
        )
        message = "Source bindings must partition every raw code point"
    elif change == "anchor":
        damaged = tuple(
            replace(
                item,
                source_span=item.source_span.model_copy(
                    update={"anchor": len(located)}
                ),
            )
            if item.source_span.role == "reminder"
            else item
            for item in located
        )
        message = "Reminder anchor must locate a body in the same line"
    elif change == "line":
        damaged = tuple(replace(item, line_ordinal=99) for item in located)
        message = "Source binding line must agree with its raw position"
    elif change == "order":
        damaged = tuple(reversed(located))
        message = "Source binding ordinals must be continuous from zero"
    else:
        damaged = tuple(
            replace(
                item, source_span=item.source_span.model_copy(update={"role": "layout"})
            )
            if item.source_span.role == "body"
            else item
            for item in located
        )
        message = "Layout candidates may contain only source whitespace"
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        spans.verify(text, damaged)


def test_placeholder_provenance_separates_literal_letters_quotes_and_digits() -> None:
    text = "N test２ 『Synthetic ３』 X"
    part = partition(text)[0]
    units = provenance.trace(text, part)
    assert "".join(unit.text for unit in units) == "N testN 『X』 X"
    replaced = [
        (unit.transformation, provenance.raw_value(text, unit))
        for unit in units
        if unit.transformation != "literal"
    ]
    assert replaced == [("digits", "２"), ("quoted", "Synthetic ３")]
    assert units[0].transformation == "literal"
    assert units[-1].transformation == "literal"
    assert provenance.value_hash(text, units[0]) != provenance.value_hash(
        text, units[6]
    )


@pytest.mark.parametrize("text", ["Ａ\u030a", "ﬁ", "가", "a\u0315\u0300", "『』"])
def test_nfkc_expansion_composition_reordering_and_empty_quote_keep_raw_positions(
    text: str,
) -> None:
    part = partition(text)[0]
    units = provenance.trace(text, part)
    assert "".join(unit.text for unit in units) == part.normalized
    raw_positions = {
        i
        for unit in units
        for segment in unit.origins
        for i in range(segment.start, segment.end)
    }
    assert raw_positions == set(range(len(text)))
    if text == "ﬁ":
        assert units[0].origins == units[1].origins
    elif text == "Ａ\u030a":
        assert len(units) == 1
        assert units[0].origins[0].end == 2
    assert unicodedata.normalize("NFKC", text) == "".join(
        unit.text for unit in provenance.nfkc(text, tuple(range(len(text))))
    )


def test_composition_across_removed_reminder_never_fills_its_raw_gap() -> None:
    text = "A（Synthetic）\u030a"
    body = next(part for part in partition(text) if part.role == "body")
    unit = provenance.trace(text, body)[0]
    assert unit.text == "Å"
    assert [(span.start, span.end) for span in unit.origins] == [
        (0, 1),
        (len(text) - 1, len(text)),
    ]
    assert provenance.raw_value(text, unit) == "A\u030a"


def test_provenance_refuses_a_normalized_payload_that_does_not_replay() -> None:
    text = "Synthetic２"
    part = replace(partition(text)[0], normalized="Unrelated")
    with pytest.raises(
        ValueError,
        match=r"\AParameter provenance must reproduce the pinned normalized bytes\Z",
    ):
        provenance.trace(text, part)
    with pytest.raises(
        ValueError,
        match=r"\ANFKC provenance requires one position per raw code point\Z",
    ):
        provenance.nfkc(text, ())
