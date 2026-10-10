"""Typed context aggregation keeps assignment names distinct from derived variants."""

from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import Code, FrameId, hash_payload
from sve_carddb.contracts.source_binding import SourceSpan, TypedValue
from sve_carddb.core.models import RecordData

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.contracts.source_binding import SourceBinding


class ContextBinding(RecordData):
    frame_id: FrameId
    values: dict[Code, TypedValue]
    source_span: SourceSpan


class ContextVariant(RecordData):
    assignment: Code
    bindings: tuple[ContextBinding, ...]

    def identity(self) -> str:
        """Only the pinned source projection enters the context-variant-v2 recipe."""
        payload: dict[str, JsonValue] = {
            "recipe": "context-variant-v2",
            **self.model_dump(mode="json"),
        }
        return "cv:" + hash_payload(payload)


def context_variant(
    bindings: tuple[SourceBinding, ...], assignment: str = "default"
) -> str:
    """A single owner field aggregates in source order, regardless of caller iteration order."""
    ordered = tuple(sorted(bindings, key=lambda binding: binding.ordinal))
    if tuple(binding.ordinal for binding in ordered) != tuple(range(len(ordered))):
        raise ValueError("Context bindings must have unique continuous source ordinals")
    if ordered and any(binding.source != ordered[0].source for binding in ordered):
        raise ValueError("Context bindings must belong to the same exact owner field")
    return ContextVariant(
        assignment=assignment,
        bindings=tuple(
            ContextBinding(
                frame_id=binding.frame_id,
                values=dict(binding.values),
                source_span=binding.source_span,
            )
            for binding in ordered
        ),
    ).identity()
