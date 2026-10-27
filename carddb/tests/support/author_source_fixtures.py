"""Build source contexts for tests of the production operand and expansion interfaces."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.domains.effects.author_sources import SourceContext
from sve_carddb.domains.translations.four_layer_derivation import derive_source

if TYPE_CHECKING:
    from sve_carddb.contracts.author_evidence import SourceUse
    from sve_carddb.contracts.source_binding import SourceDescriptor
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_semantics import CardContext


@dataclass(frozen=True)
class SourceCandidate:
    raw: str
    descriptor: SourceDescriptor
    classifier: Classifier
    context: CardContext | None = None


def source_context(
    candidate: SourceCandidate, use: SourceUse | None = None
) -> SourceContext:
    derived = derive_source(
        candidate.raw,
        candidate.descriptor,
        candidate.classifier,
        context=candidate.context,
    )
    index = (
        0
        if use is None
        else next(
            i
            for i, part in enumerate(derived.field.parts)
            if part.line_ordinal == use.occurrence.line_ordinal
        )
    )
    frame, binding = derived.bind(index)
    return SourceContext(
        frame,
        binding,
        derived.field.parts[index].canonical_source,
        candidate.classifier,
    )
