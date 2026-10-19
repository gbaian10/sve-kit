"""Frozen owner coordinates and exact bytes determine candidate status."""

from dataclasses import replace

import pytest

from sve_carddb.contracts.source_binding import SourceDescriptor, SourceSpan
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.effects.matching import FrameOccurrence, Matcher

from .test_classifications import member


def frame(
    *, raw: str = "synthetic", revision: str = "r1", current: bool = True
) -> FrameOccurrence:
    source = SourceDescriptor.model_validate_json(
        canonical(
            {
                "owner": {"kind": "face_revision", "revision_id": revision},
                "field": "effect",
                "ordinal": None,
                "source_unit_id": "t:fixture",
                "source_hash": digest(raw.encode())[7:],
                "source_ref": {
                    "batch_id": "batch:fixture",
                    "source_version_id": "source:fixture",
                    "parser": "fixture",
                    "locator": "/faces/0/text",
                    "text_hash": digest(raw.encode())[7:],
                },
            }
        )
    )
    return FrameOccurrence(
        position=member().position,
        card_id="card:fixture",
        face_id="face:fixture",
        frame_id="frame:" + digest(revision.encode())[7:],
        binding_id="bind:" + digest(revision.encode())[7:],
        normalizer="fixture-v1",
        source=source,
        span=SourceSpan.model_validate_json(
            canonical(
                {
                    "role": "body",
                    "anchor": None,
                    "segments": [{"start": 0, "end": len(raw)}],
                }
            )
        ),
        current=current,
        raw=raw,
    )


def test_current_preference_retains_all_history_and_deduplicates_evidence() -> None:
    present, history = frame(), frame(revision="old", current=False)
    result = Matcher((history, present, present)).match(member())
    assert result.status == "current"
    assert result.selected == (present,)
    assert result.exact == (history, present)
    assert result.candidates == (history, present)


def test_unique_history_survives_changed_current() -> None:
    history = frame(revision="old", current=False)
    result = Matcher((frame(raw="changed"), history)).match(member())
    assert result.status == "historical"
    assert result.selected == (history,)
    assert len(result.candidates) == 2


@pytest.mark.parametrize("current", [True, False])
def test_distinct_exact_revisions_are_ambiguous(current: bool) -> None:
    first, second = frame(current=current), frame(revision="r2", current=current)
    result = Matcher((first, second)).match(member())
    assert result.status == "ambiguous"
    assert result.selected == (first, second)


def test_different_card_identity_is_ambiguous_even_when_only_one_body_matches() -> None:
    other = replace(frame(raw="changed", revision="r2"), card_id="other", raw="changed")
    assert Matcher((frame(), other)).match(member()).status == "ambiguous"


def test_changed_source_is_not_an_exact_match() -> None:
    result = Matcher((frame(raw="different"),)).match(member())
    assert result.status == "source_changed"
    assert not result.exact
    assert not result.selected


def test_absent_position_has_no_candidate() -> None:
    assert Matcher(()).match(member()).status == "no_candidate"


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("card_no", "TEST-002"),
        ("face", 1),
        ("line", 1),
    ],
)
def test_same_card_wording_at_another_position_is_not_a_fallback(
    field: str, value: str | int
) -> None:
    changed = member().model_copy(
        update={"position": member().position.model_copy(update={field: value})}
    )
    assert Matcher((frame(),)).match(changed).status == "no_candidate"


def test_section_ordinal_never_falls_back_to_effect() -> None:
    changed = member().model_copy(
        update={
            "position": member().position.model_copy(
                update={"field": "section", "ordinal": 0}
            )
        }
    )
    assert Matcher((frame(),)).match(changed).status == "no_candidate"


def test_unicode_normalization_is_not_a_byte_match() -> None:
    changed = member().model_copy(update={"body_hash": digest("Ａ".encode())})
    assert Matcher((frame(raw="A"),)).match(changed).status == "source_changed"


def test_conflicting_binding_evidence_is_rejected() -> None:
    with pytest.raises(ValueError, match="Conflicting"):
        Matcher((frame(), replace(frame(), current=False, raw="synthetic")))


@pytest.mark.parametrize("change", ["hash", "field", "span", "line", "newline"])
def test_occurrence_rechecks_source_bytes_and_spans(change: str) -> None:
    with pytest.raises(ValueError, match="Frame occurrence"):
        invalid_occurrence(change)


def invalid_occurrence(change: str) -> None:
    target = frame()
    if change == "hash":
        replace(target, raw="changed")
    elif change == "field":
        replace(
            target,
            position=target.position.model_copy(
                update={"field": "section", "ordinal": 0}
            ),
            raw="synthetic",
        )
    elif change == "span":
        replace(
            target,
            span=SourceSpan.model_validate_json(
                canonical(
                    {
                        "role": "body",
                        "anchor": None,
                        "segments": [{"start": 0, "end": 100}],
                    }
                )
            ),
            raw="synthetic",
        )
    elif change == "line":
        replace(
            target,
            position=target.position.model_copy(update={"line": 1}),
            raw="synthetic",
        )
    else:
        frame(raw="one\ntwo")


@pytest.mark.parametrize("change", ["owner", "role", "ordinal"])
def test_occurrence_requires_revision_body_and_matching_ordinal(change: str) -> None:
    target = frame()
    source = target.source.model_dump(mode="json")
    span = target.span.model_dump(mode="json")
    position = target.position
    if change == "owner":
        source["owner"] = {
            "kind": "printing_face",
            "printing_id": "printing:fixture",
            "face_id": "face:fixture",
        }
    elif change == "role":
        span["role"] = "reminder"
    else:
        source.update(field="section", ordinal=0)
        position = position.model_copy(update={"field": "section", "ordinal": 1})
    descriptor = SourceDescriptor.model_validate_json(canonical(source))
    source_span = SourceSpan.model_validate_json(canonical(span))
    with pytest.raises(
        ValueError, match="Frame occurrence differs from exact source field"
    ):
        replace(
            target,
            source=descriptor,
            span=source_span,
            position=position,
            raw="synthetic",
        )


def test_section_ordinal_distinguishes_identical_source_bytes() -> None:
    target = frame()
    source = target.source.model_dump(mode="json")
    source.update(field="section", ordinal=0)
    position = target.position.model_copy(update={"field": "section", "ordinal": 0})
    occurrence = replace(
        target,
        source=SourceDescriptor.model_validate_json(canonical(source)),
        position=position,
        raw="synthetic",
    )
    first = member().model_copy(update={"position": position})
    second = first.model_copy(
        update={"position": position.model_copy(update={"ordinal": 1})}
    )
    matcher = Matcher((occurrence,))
    assert matcher.match(first).status == "current"
    assert matcher.match(second).status == "no_candidate"


def test_disjoint_body_segments_are_concatenated_without_gap_text() -> None:
    raw = "syn[reminder]thetic"
    target = frame(raw=raw)
    span = target.span.model_dump(mode="json")
    span["segments"] = [{"start": 0, "end": 3}, {"start": 13, "end": len(raw)}]
    occurrence = replace(
        target, span=SourceSpan.model_validate_json(canonical(span)), raw=raw
    )
    result = Matcher((occurrence,)).match(member())
    assert result.status == "current"
    assert result.exact == (occurrence,)
    assert occurrence.source.source_hash == digest(raw.encode())[7:]
