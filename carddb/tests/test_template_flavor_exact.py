"""Synthetic narrative counterexamples for the independent exact-flavor recipe."""

from dataclasses import FrozenInstanceError
from typing import cast

import pytest

from sve_carddb.template_sources.flavor import (
    CODE_PATH,
    VERSION,
    WHITE_SPACE,
    Segment,
    normalize,
    partition,
)


def test_unknown_empty_and_whitespace_do_not_create_a_narrative_part() -> None:
    for raw, state in (
        (None, "unknown"),
        ("", "empty"),
        (" \r\n\u3000", "whitespace_only"),
    ):
        result = partition(raw)
        assert result.state == state
        assert result.parts == ()
        assert result.part is None


def test_whitespace_set_is_the_contracts_fixed_25_code_points() -> None:
    expected = {
        0x0009,
        0x000A,
        0x000B,
        0x000C,
        0x000D,
        0x0020,
        0x0085,
        0x00A0,
        0x1680,
        0x2000,
        0x2001,
        0x2002,
        0x2003,
        0x2004,
        0x2005,
        0x2006,
        0x2007,
        0x2008,
        0x2009,
        0x200A,
        0x2028,
        0x2029,
        0x202F,
        0x205F,
        0x3000,
    }
    assert expected == WHITE_SPACE
    for code_point in expected:
        raw = chr(code_point)
        assert normalize(raw) == raw
        assert partition(raw).state == "whitespace_only"
        assert partition(raw).parts == ()
    assert (
        partition("".join(chr(cp) for cp in sorted(expected))).state
        == "whitespace_only"
    )


@pytest.mark.parametrize(
    "raw", ["\u200b", "\ufeff", "\x1c", "\x1d", "\x1e", "\x1f", "\u180e"]
)
def test_characters_outside_fixed_whitespace_remain_narrative(raw: str) -> None:
    result = partition(raw)
    assert result.state == "present"
    assert result.part is not None
    assert result.part.normalized == raw
    assert result.part.segments == (Segment(0, 1),)
    assert partition(" \n" + raw + "\u3000").state == "present"


def test_complete_narrative_keeps_layout_quotes_width_and_literal_symbols() -> None:
    raw = " \t甲２\r\n（乙『丙３』）\n\nＮＸ\\{自編}\u3000 "
    assert normalize(raw) == raw
    result = partition(raw)
    assert result.state == "present"
    assert len(result.parts) == 1
    part = result.part
    assert part is not None
    assert part.normalized == raw
    assert part.normalized.encode() == raw.encode()
    assert part.segments == (Segment(0, len(raw)),)
    assert part.member_source == raw
    assert part.role == "flavor"
    assert part.line_ordinal == 0
    assert part.anchor is None
    assert part.legacy_id is None
    assert part.template is None
    assert part.slots == ()
    assert (
        part.normalized_hash
        == "sha256:2f5b48c33d0048e625bfab10ed2bf0fdabc0f586e86c22cb1ac0ea8e55591d49"
    )


def test_span_uses_code_points_not_utf8_bytes_or_utf16_units() -> None:
    raw = "甲🌙２\r\n乙"
    part = partition(raw).part
    assert part is not None
    assert len(raw) == 6
    assert len(raw.encode("utf-8")) != 6
    assert len(raw.encode("utf-16-le")) // 2 != 6
    assert part.segments == (Segment(0, 6),)
    assert raw[part.segments[0].start : part.segments[0].end] == raw


@pytest.mark.parametrize(
    ("left", "right"),
    [
        ("甲２", "甲３"),
        ("甲２", "甲2"),
        ("甲『乙』", "甲『丙』"),
        ("甲\r\n乙", "甲\n乙"),
        ("甲\n\n乙", "甲\n乙"),
        (" 甲 ", "甲"),
        ("ＡＮＸ", "ANX"),
    ],
)
def test_semantically_distinct_raw_representations_never_share_normalized_hash(
    left: str,
    right: str,
) -> None:
    first, second = partition(left).part, partition(right).part
    assert first is not None
    assert second is not None
    assert first.normalized == left
    assert second.normalized == right
    assert first.normalized_hash != second.normalized_hash


@pytest.mark.parametrize(
    "raw",
    [
        pytest.param(False, id="boolean"),
        pytest.param(1, id="integer"),
        pytest.param(b"Synthetic", id="bytes"),
        pytest.param([], id="list"),
        pytest.param({}, id="mapping"),
    ],
)
def test_nonstring_source_is_rejected_before_stringification(raw: object) -> None:
    with pytest.raises(ValueError, match=r"\AFlavor text must be a string\Z"):
        normalize(cast("str", raw))
    with pytest.raises(ValueError, match=r"\AFlavor text must be a string\Z"):
        partition(cast("str", raw))


def test_normalize_does_not_accept_unknown_as_an_empty_string() -> None:
    with pytest.raises(ValueError, match=r"\AFlavor text must be a string\Z"):
        normalize(cast("str", None))


@pytest.mark.parametrize(
    "raw",
    [
        pytest.param(chr(0xD800), id="high-surrogate"),
        pytest.param(chr(0xDFFF), id="low-surrogate"),
        pytest.param("Synthetic" + chr(0xD800) + chr(0xDC00), id="surrogate-pair"),
        pytest.param(chr(0xDC00) + "Private", id="surrogate-with-narrative"),
    ],
)
def test_invalid_utf8_is_rejected_without_echoing_source(raw: str) -> None:
    with pytest.raises(
        ValueError, match=r"\AFlavor text must be valid UTF-8\Z"
    ) as error:
        normalize(raw)
    assert raw not in str(error.value)
    with pytest.raises(ValueError, match=r"\AFlavor text must be valid UTF-8\Z"):
        partition(raw)


def test_exact_recipe_metadata_does_not_reuse_classification() -> None:
    assert VERSION == "flavor-exact-v1"
    assert CODE_PATH == "carddb/src/sve_carddb/template_sources/flavor.py"
    assert not normalize("")


def test_partition_results_and_source_coordinates_are_frozen() -> None:
    result = partition("Synthetic２")
    part = result.part
    assert part is not None
    for item, field, replacement in (
        (result, "state", "empty"),
        (part, "normalized", "changed"),
        (part.segments[0], "end", 0),
    ):
        with pytest.raises(FrozenInstanceError):
            setattr(item, field, replacement)
