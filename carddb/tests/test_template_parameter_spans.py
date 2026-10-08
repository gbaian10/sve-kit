"""Invented complete fields, disjoint reminder anchors and many-to-many NFKC."""

import re
import unicodedata
from dataclasses import replace

import pytest

from sve_carddb.contracts.template_parameters import Range
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


@pytest.mark.parametrize(
    "change", ["wrong_role", "other_line", "unanchored", "reordered"]
)
def test_anchor_target_and_first_position_order_are_independent_guards(
    change: str,
) -> None:
    text = "A（Synthetic）B\nC"
    located = spans.locate(text, partition(text))
    if change == "reordered":
        damaged = tuple(
            replace(item, ordinal=index) for index, item in enumerate(reversed(located))
        )
        message = "Source bindings must follow their first source position"
    else:
        reminder = next(item for item in located if item.source_span.role == "reminder")
        target = (
            reminder.ordinal
            if change == "wrong_role"
            else next(
                item.ordinal
                for item in located
                if item.source_span.role == "body" and item.line_ordinal == 1
            )
            if change == "other_line"
            else None
        )
        damaged = tuple(
            replace(
                item, source_span=item.source_span.model_copy(update={"anchor": target})
            )
            if item == reminder
            else item
            for item in located
        )
        message = (
            "Inline reminder must anchor to its source-line body"
            if change == "unanchored"
            else "Reminder anchor must locate a body in the same line"
        )
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        spans.verify(text, damaged)


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


def test_utf8_roundtrip_rejects_a_valid_prefix_with_the_final_character_missing() -> (
    None
):
    text = "試験"
    located = spans.locate(text, partition(text))
    shortened = replace(
        located[0],
        source_span=located[0].source_span.model_copy(
            update={"segments": (Range(start=0, end=1),)}
        ),
    )
    with pytest.raises(
        ValueError, match=r"\ASource bindings must roundtrip exact field UTF-8 bytes\Z"
    ):
        spans.verify(text, (shortened,))


def test_one_line_cannot_produce_two_body_candidates() -> None:
    parts = partition("A B")
    with pytest.raises(
        ValueError, match=r"\AA source line must not contain multiple body candidates\Z"
    ):
        spans.locate("A B", (parts[0], parts[0]))


def test_whole_nfkc_provenance_must_agree_with_prefix_normalization(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls = iter(("A", "B"))
    with monkeypatch.context() as patch:
        patch.setattr(unicodedata, "normalize", lambda *_: next(calls))
        with pytest.raises(
            ValueError,
            match=r"\ANFKC provenance differs from whole-string normalization\Z",
        ):
            provenance.nfkc("A", (0,))


def test_matching_normalized_bytes_do_not_hide_a_lost_source_position(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    part = partition("A")[0]
    monkeypatch.setattr(provenance, "nfkc", lambda *_: (provenance.Unit("A", ()),))
    with pytest.raises(
        ValueError,
        match=r"\AParameter provenance must retain every selected raw position\Z",
    ):
        provenance.trace("A", part)
