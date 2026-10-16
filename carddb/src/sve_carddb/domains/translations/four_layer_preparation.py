"""Isolated v3 preparation; production workflows keep their v2 entry and readers."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_expressions import DOMAIN, DOMAIN_CODE
from sve_carddb.domains.translations.four_layer_matching import bind_recognized
from sve_carddb.domains.translations.four_layer_normalizer import PREPARATION_VERSION

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Frame, Target
    from sve_carddb.contracts.source_binding import SourceBinding, SourceDescriptor
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField
    from sve_carddb.domains.translations.four_layer_semantics import CardContext


@dataclass(frozen=True)
class PreparedField:
    derived: DerivedField

    def bind(self, ordinal: int) -> tuple[Frame, SourceBinding]:
        """Build preparation identities from the independently derived source evidence."""
        return self.derived.recognized[ordinal].bind_version(
            self.derived.field.source,
            self.derived.field.parts[ordinal],
            normalizer_version=PREPARATION_VERSION,
        )

    def verify(
        self,
        raw: str,
        classifier: Classifier,
        frame: Frame,
        binding: SourceBinding,
        *,
        context: CardContext | None = None,
        target: Target | None = None,
    ) -> None:
        """Reconstruct exact source evidence; a version label never proves v3 validity."""
        if frame.source.normalizer_version != PREPARATION_VERSION:
            raise ValueError("Preparation requires the explicit v3 normalizer version")
        expected = derive_preparation(
            raw, self.derived.field.source, classifier, context=context
        )
        if self != expected:
            raise ValueError("Preparation differs from reconstructed source evidence")
        ordinal = binding.ordinal
        if ordinal >= len(self.derived.field.parts):
            raise ValueError("Binding ordinal lies outside the exact field")
        part = self.derived.field.parts[ordinal]
        found = self.derived.recognized[ordinal]
        expected_frame, _ = self.bind(ordinal)
        part.verify(
            raw,
            frame,
            binding,
            {**classifier.domains, DOMAIN_CODE: DOMAIN},
            normalizer_version=PREPARATION_VERSION,
        )
        reconstructed = bind_recognized(
            frame, self.derived.field, part, found, mixed=True
        )
        if (
            reconstructed != binding
            or frame.semantic_variant != expected_frame.semantic_variant
            or frame.projection != expected_frame.projection
        ):
            raise ValueError(
                "Preparation binding differs from exact source construction"
            )
        if target is not None:
            self.derived.verify_target(binding, target)


def derive_preparation(
    raw: str,
    source: SourceDescriptor,
    classifier: Classifier,
    *,
    context: CardContext | None = None,
) -> PreparedField:
    """Prepare v3 without changing production derivation or enabling new source families."""
    return PreparedField(derive_source(raw, source, classifier, context=context))
