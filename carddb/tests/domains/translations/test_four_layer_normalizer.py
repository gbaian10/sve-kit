"""Exact source replay protects names and retains many-to-many Unicode positions."""

from dataclasses import replace
from unicodedata import normalize

import pytest

from sve_carddb.contracts.four_layer import (
    Domain,
    Frame,
    LeafSchema,
    LeafSlot,
    OccurrenceKey,
    Projection,
    SemanticVariant,
    Source,
    Span,
    hash_payload,
)
from sve_carddb.contracts.source_binding import (
    LeafOccurrence,
    SourceBinding,
    SourceDescriptor,
)
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.translations.four_layer_normalizer import (
    VERSION,
    SourceField,
    normalize_source,
)


def source(raw: str, field: str = "effect") -> SourceDescriptor:
    checksum = digest(raw.encode())[7:]
    return SourceDescriptor.model_validate_json(
        canonical(
            {
                "owner": {"kind": "face_revision", "revision_id": "revision:synthetic"},
                "field": field,
                "ordinal": 0 if field == "section" else None,
                "source_unit_id": "t:ja:" + checksum[:16],
                "source_hash": checksum,
                "source_ref": {
                    "batch_id": "fixture:jp",
                    "source_version_id": "src:fixture",
                    "parser": "fixture-parser-v1",
                    "locator": "/faces/0/text",
                    "text_hash": checksum,
                },
            }
        )
    )


def test_name_parentheses_and_inner_numbers_are_not_reminders_or_numeric_leaves() -> (
    None
):
    raw = "『仮（２）😀』を２枚選ぶ。（補足）\r\n"
    result = normalize_source(
        raw, source(raw), reminders=frozenset({"（補足）", "（２）"})
    )
    body, reminder, cr, lf = result.parts
    assert body.canonical_source == "『X』をN枚選ぶ。"
    assert body.source_span.segments == (Span(start=0, end=13),)
    assert reminder.canonical_source == "（補足）"
    assert reminder.source_span.anchor == body.ordinal
    assert cr.canonical_source == lf.canonical_source == "W"
    names = [u for u in body.units if u.transformation == "quoted"]
    numbers = [u for u in body.units if u.transformation == "digits"]
    assert len(names) == len(numbers) == 1
    assert names[0].origins[0].start == 1
    assert raw[numbers[0].origins[0].start : numbers[0].origins[0].end] == "２"


def test_nfkc_composition_across_removed_reminder_retains_both_raw_origins() -> None:
    raw = "e（補足）\u0301😀"
    body, reminder = normalize_source(
        raw, source(raw), reminders=frozenset({"（補足）"})
    ).parts
    assert body.canonical_source == "é😀"
    assert body.source_span.segments == (Span(start=0, end=1), Span(start=5, end=7))
    assert [p.canonical_spans for p in body.trace] == [
        (Span(start=0, end=1),),
        (Span(start=0, end=1),),
        (Span(start=1, end=2),),
    ]
    assert [p.raw_span for p in body.trace] == [
        Span(start=0, end=1),
        Span(start=5, end=6),
        Span(start=6, end=7),
    ]
    assert reminder.source_span.anchor == body.ordinal


@pytest.mark.parametrize("raw", ["㍑😀", "\ufb03\u0301", "A\u030a\u0323", "か\u3099"])
def test_nfkc_replay_keeps_every_raw_and_canonical_scalar(raw: str) -> None:
    (part,) = normalize_source(raw, source(raw)).parts
    assert part.canonical_source == normalize("NFKC", raw)
    assert [p.raw_span.start for p in part.trace] == list(range(len(raw)))
    assert {
        i
        for p in part.trace
        for span in p.canonical_spans
        for i in range(span.start, span.end)
    } == set(range(len(part.canonical_source)))


def test_named_owner_field_preserves_symbols_and_exact_whitespace() -> None:
    raw = " 『仮《概念》（２）』 \r\n"
    (part,) = normalize_source(raw, source(raw, "name")).parts
    assert part.source_span.role == "name"
    assert part.canonical_source == raw
    assert all(u.transformation == "literal" for u in part.units)


def test_blank_layout_and_empty_fields_do_not_fabricate_body_parts() -> None:
    raw = " \t\r\n\n"
    result = normalize_source(raw, source(raw))
    assert all(p.source_span.role == "layout" for p in result.parts)
    assert (
        "".join(
            raw[piece.raw_span.start : piece.raw_span.end]
            for part in result.parts
            for piece in part.trace
        )
        == raw
    )
    assert normalize_source("", source("")).parts == ()


def test_unknown_parentheses_remain_in_body_with_their_control_semantics() -> None:
    raw = "仮を選ぶ（しなくてもよい）。"
    (part,) = normalize_source(raw, source(raw)).parts
    assert part.source_span.role == "body"
    assert part.source_span.segments == (Span(start=0, end=len(raw)),)
    assert part.canonical_source == normalize("NFKC", raw)


def test_stale_source_is_rejected_before_any_normalization() -> None:
    with pytest.raises(ValueError, match="stale exact bytes"):
        normalize_source("新版仮文", source("旧版仮文"))


def bound_number(raw: str) -> tuple[SourceField, Frame, SourceBinding]:
    field = normalize_source(raw, source(raw))
    (part,) = field.parts
    numeric = next(
        i for i, unit in enumerate(part.units) if unit.transformation == "digits"
    )
    canonical_span = Span(start=numeric, end=numeric + 1)
    origins = tuple(Span(start=s.start, end=s.end) for s in part.units[numeric].origins)
    occurrence = OccurrenceKey(
        owner=field.source.owner,
        field=field.source.field,
        ordinal=field.source.ordinal,
        source_hash=field.source.source_hash,
        line_ordinal=part.line_ordinal,
        role=part.source_span.role,
        segments=part.source_span.segments,
    )
    frame = Frame(
        id="frame:" + "0" * 64,
        content_hash="0" * 64,
        source=Source(
            source_lang="ja",
            normalizer_version=VERSION,
            canonical_hash=digest(part.canonical_source.encode())[7:],
        ),
        role="body",
        leaf_schema=LeafSchema(
            format=2,
            slots=(
                LeafSlot(
                    name="n",
                    type="Nat",
                    role="count",
                    domain=Domain(values=(), min=0, max=9007199254740991),
                    required=True,
                    occurrences=(canonical_span,),
                ),
            ),
        ),
        semantic_variant=SemanticVariant(state="pending", key=None, scope=occurrence),
        projection=Projection(
            projection_kind="pending",
            discriminator=None,
            scopes=(),
            imports=(),
            exports=(),
        ),
    )
    checksum = hash_payload(frame.payload(part.canonical_source))
    frame = frame.model_copy(
        update={"id": "frame:" + checksum, "content_hash": checksum}
    )
    value = int(normalize("NFKC", "".join(raw[s.start : s.end] for s in origins)))
    binding = SourceBinding(
        id="bind:" + "0" * 64,
        source=field.source,
        ordinal=part.ordinal,
        line_ordinal=part.line_ordinal,
        frame_id=frame.id,
        source_span=part.source_span,
        values={"n": value},
        occurrences=(
            LeafOccurrence(
                slot="n",
                ordinal=0,
                raw_spans=origins,
                canonical_spans=(canonical_span,),
                source_unit=None,
                source_presence="explicit",
                resolution_rule=None,
            ),
        ),
        trace=part.trace,
    )
    return field, frame, rekey(binding)


def rekey(binding: SourceBinding) -> SourceBinding:
    return binding.model_copy(update={"id": "bind:" + hash_payload(binding.payload())})


def test_valid_binding_replays_raw_positions_and_exact_numeric_value() -> None:
    raw = "仮😀２枚"
    field, frame, binding = bound_number(raw)
    field.verify(raw, (frame,), (binding,), {})
    assert binding.occurrences[0].raw_spans == (Span(start=2, end=3),)
    invalid = rekey(binding.model_copy(update={"values": {"n": 9}}))
    with pytest.raises(ValueError, match="Numeric leaf value differs"):
        field.verify(raw, (frame,), (invalid,), {})
    shifted = binding.occurrences[0].model_copy(
        update={"raw_spans": (Span(start=3, end=4),)}
    )
    invalid = rekey(binding.model_copy(update={"occurrences": (shifted,)}))
    with pytest.raises(ValueError, match="Leaf raw positions differ"):
        field.verify(raw, (frame,), (invalid,), {})


def test_forged_trace_and_missing_owner_use_cannot_pass_normalization_replay() -> None:
    raw = "仮😀２枚"
    field, frame, binding = bound_number(raw)
    rewritten = binding.trace[2].model_copy(update={"rule": "identity"})
    invalid = rekey(
        binding.model_copy(
            update={"trace": (*binding.trace[:2], rewritten, *binding.trace[3:])}
        )
    )
    with pytest.raises(ValueError, match="replayed exact source partition"):
        field.verify(raw, (frame,), (invalid,), {})
    with pytest.raises(ValueError, match="complete exact source field"):
        field.verify(raw, (), (), {})
    forged = replace(field, parts=(replace(field.parts[0], canonical_source="異文"),))
    with pytest.raises(ValueError, match="pinned normalization replay"):
        forged.verify(raw, (frame,), (binding,), {})


def test_normalized_numeric_placeholder_cannot_be_hidden_in_a_literal_target() -> None:
    raw = "仮２枚"
    field, frame, binding = bound_number(raw)
    frame = frame.model_copy(update={"leaf_schema": LeafSchema(format=2, slots=())})
    checksum = hash_payload(frame.payload(field.parts[0].canonical_source))
    frame = frame.model_copy(
        update={"id": "frame:" + checksum, "content_hash": checksum}
    )
    invalid = rekey(
        binding.model_copy(
            update={"frame_id": frame.id, "values": {}, "occurrences": ()}
        )
    )
    with pytest.raises(ValueError, match="requires its typed leaf"):
        field.verify(raw, (frame,), (invalid,), {})
