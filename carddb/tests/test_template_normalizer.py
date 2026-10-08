"""Synthetic counterexamples for the complete legacy recipe and exact partition."""

import re
from dataclasses import replace

import pytest

from sve_carddb.domains.translations.source_inventory.normalizer import (
    Segment,
    normalize,
    partition,
    verify_partition,
)


def bodies(text: str, *, section: int | None = None) -> list[str]:
    return [
        part.normalized
        for part in partition(text, section=section)
        if part.role == "body"
    ]


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("  Sample２（Synthetic note）  \n\tOther３\r\n\n", ["SampleN", "OtherN"]),
        (" \u3000（Synthetic only）\t\n", []),
        ("A（outer（inner）tail）B", ["A(outertail)B"]),
        ("A（one） B（two）C", ["A BC"]),
        ("A(Synthetic note)２", ["A(Synthetic note)N"]),
        ("（one）  Body２  （two）", ["BodyN"]),
        ("Ｎ９『Name１２』⑧", ["NN『X』N"]),
        ("literalN『X』", ["literalN『X』"]),
        ("Ａ\vＢ\fＣ\u2028Ｄ", ["A\vB\fC\u2028D"]),
        ("", []),
        (" ", []),
    ],
)
def test_complete_recipe(raw: str, expected: list[str]) -> None:
    assert bodies(raw) == expected


def test_normalize_does_not_add_preprocessing_to_its_input() -> None:
    assert normalize("  Ａ２（Synthetic）\n") == "  AN(Synthetic)\n"


@pytest.mark.parametrize(
    "header",
    [
        "『Synthetic』{Synthetic}フォロワー{コスト２}{攻撃力}１/{体力}３",
        "『Synthetic』{Synthetic}クレスト",
        "『Synthetic』{Synthetic}スペル{コスト２}",
    ],
)
def test_headers_with_and_without_body_preserve_all_text(header: str) -> None:
    parts = partition(header + "Body２\nNext３\n" + header, section=4)
    assert [part.normalized for part in parts if part.role == "body"] == [
        "BodyN",
        "NextN",
    ]
    assert len([part for part in parts if part.role == "token_header"]) == 2


def test_header_pattern_runs_only_on_sections_and_before_trimming() -> None:
    header = "『Synthetic』{Synthetic}クレスト"
    assert bodies(header, section=0) == []
    assert bodies(header) == ["『X』{Synthetic}クレスト"]
    assert bodies(" " + header, section=0) == ["『X』{Synthetic}クレスト"]


def test_unknown_header_is_rejected_without_echoing_source() -> None:
    with pytest.raises(ValueError, match=r"\AUnrecognized legacy token header\Z"):
        partition("『Private synthetic name』{Synthetic}UnknownType", section=0)


def test_header_activate_cost_is_body_when_header_has_no_stats() -> None:
    text = "『Synthetic』{Synthetic}クレスト{起動}{コスト０}：Test２"
    assert bodies(text, section=0) == ["{起動}{コストN}:TestN"]


def test_noncontiguous_body_and_crlf_use_original_code_point_coordinates() -> None:
    text = " Ａ２（note）Ｂ３ \r\n\t"
    parts = partition(text)
    body = next(part for part in parts if part.role == "body")
    assert body.normalized == "ANBN"
    assert body.segments == (Segment(1, 3), Segment(9, 11))
    spans = sorted(
        (span for part in parts for span in part.segments), key=lambda span: span.start
    )
    assert (
        "".join(text[span.start : span.end] for span in spans).encode() == text.encode()
    )
    assert any(part.normalized == "\n" for part in parts)
    assert any("\r" in part.normalized for part in parts if part.role == "layout")


@pytest.mark.parametrize(
    "change", ["gap", "overlap", "negative", "outside", "empty", "tail"]
)
def test_independent_trace_verifier_rejects_incomplete_or_overlapping_ranges(
    change: str,
) -> None:
    parts = partition(" A（note）B ")
    body = next(part for part in parts if part.role == "body")
    malformed = {
        "gap": body.segments[1:],
        "overlap": (Segment(0, 2), *body.segments[1:]),
        "negative": (Segment(-1, 2), *body.segments[1:]),
        "outside": (Segment(1, 200),),
        "empty": (Segment(1, 1), *body.segments[1:]),
        "tail": body.segments,
    }[change]
    bad = tuple(
        replace(part, segments=malformed) if part is body else part for part in parts
    )
    if change == "tail":
        bad = tuple(part for part in bad if part is not parts[-1])
        message = "Legacy trace must roundtrip exact source UTF-8 bytes"
    else:
        message = "Legacy trace must partition every source code point exactly once"
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        verify_partition(" A（note）B ", bad)


def test_empty_trace_cannot_cover_nonempty_source() -> None:
    with pytest.raises(
        ValueError, match=r"\ALegacy trace must roundtrip exact source UTF-8 bytes\Z"
    ):
        verify_partition(" ", ())
    verify_partition("", ())


def test_same_normalized_text_does_not_mean_same_source_trace() -> None:
    one = partition("２（note）２")
    two = partition("３２")
    assert one[0].normalized_hash == two[0].normalized_hash
    assert one[0].normalized == two[0].normalized == "N"
    assert one[0].segments != two[0].segments
