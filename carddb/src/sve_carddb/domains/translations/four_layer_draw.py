"""Resolve three closed draw constructions without granting author admission."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    Domain,
    FaceRevisionOwner,
    LeafSchema,
    LeafSlot,
    Projection,
    Scope,
    Span,
)
from sve_carddb.domains.translations.four_layer_semantics import Classified

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField
    from sve_carddb.domains.translations.four_layer_semantics import CardContext

_DRAW = re.compile(r"(?P<head>【攻撃時】|【進化時】)?(?P<count>0|[1-9][0-9]*)枚引く。")
_HEADS = {
    None: "draw_exact_spell.v1",
    "【攻撃時】": "draw_exact_attack_time.v1",
    "【進化時】": "draw_exact_evolve_time.v1",
}
_OPENERS = "「『（(【［[｛{〈《"
_CLOSERS = "」』）)】］]｝}〉》"


def _balanced(line: str) -> bool:
    for opener, closer in zip(_OPENERS, _CLOSERS, strict=True):
        depth = 0
        for char in line:
            if char == opener:
                depth += 1
            elif char == closer:
                depth -= 1
            if depth < 0:
                return False
        if depth:
            return False
    return True


def _head_context(head: str | None, context: CardContext, raw: str, line: str) -> bool:
    if head is None:
        return (
            context.card_kind == "spell"
            and context.phase == "normal"
            and context.section_count == 0
            and raw == line
        )
    return context.card_kind == "follower" and (
        head != "【進化時】" or context.phase == "evolved"
    )


def _schema(position: int) -> LeafSchema:
    return LeafSchema(
        format=2,
        slots=(
            LeafSlot(
                name="leaf_0",
                type="Nat",
                role="count",
                domain=Domain(min=0, max=9007199254740991, values=()),
                required=True,
                occurrences=(Span(start=position, end=position + 1),),
            ),
        ),
    )


def prepare_draw(
    raw: str, prepared: DerivedField, context: CardContext | None
) -> DerivedField:
    """CR 3.1.2.4.1, 5.10.2, 12.6.2 and 12.7.2 fix actor, count and trigger."""
    source = prepared.field.source
    lines = raw.split("\n")
    # A bracket open across lines may make a whole line quoted or reminder text.
    if context is None or not all(map(_balanced, lines)):
        return prepared
    if (
        context.source != source
        or not context.current
        or not isinstance(source.owner, FaceRevisionOwner)
        or source.field != "effect"
        or source.ordinal is not None
    ):
        return prepared
    found = list(prepared.recognized)
    for index, part in enumerate(prepared.field.parts):
        recognized = found[index]
        line = lines[part.line_ordinal]
        match = _DRAW.fullmatch(line)
        if match is None or part.source_span.role != "body":
            continue
        start = sum(len(value) + 1 for value in lines[: part.line_ordinal])
        if part.source_span.segments != (Span(start=start, end=start + len(line)),):
            continue
        head = match["head"]
        if not _head_context(head, context, raw, line):
            continue
        # A separate Quick prefix has no admitted relationship to these triggers.
        if any(
            "{クイック}" in original.canonical_source
            for original in prepared.derived.original.parts
        ):
            continue
        if (
            recognized.issues
            or recognized.schema != _schema(len(head or ""))
            or type(recognized.values.get("leaf_0")) is not int
            or recognized.values.get("leaf_0") != int(match["count"])
            or prepared.interfaces[index] is not None
        ):
            continue
        if recognized.semantics is not None:
            raise ValueError("ambiguous_semantic_variant")
        key = _HEADS[head]
        found[index] = replace(
            recognized,
            semantics=Classified(
                key,
                Projection(
                    projection_kind="ability_body",
                    discriminator=key,
                    scopes=(Scope(id="ability_0", kind="ability", parent=None),),
                    imports=(),
                    exports=(),
                ),
            ),
        )
    return replace(prepared, derived=replace(prepared.derived, recognized=tuple(found)))
