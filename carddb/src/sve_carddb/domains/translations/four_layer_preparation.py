"""Isolated v3 preparation; production workflows keep their v2 entry and readers."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    FaceRevisionOwner,
    PrintingFaceOwner,
    hash_payload,
)
from sve_carddb.domains.translations.four_layer_choices import constant_choice_interface
from sve_carddb.domains.translations.four_layer_context import (
    DEFAULT_REGISTRY,
    Descriptor,
    Registry,
)
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_expressions import (
    DOMAIN,
    DOMAIN_CODE,
    target_dependencies,
    verify_quantities,
)
from sve_carddb.domains.translations.four_layer_matching import bind_recognized
from sve_carddb.domains.translations.four_layer_n2_counters import prepare_capacities
from sve_carddb.domains.translations.four_layer_n2_parameters import prepare_parameters
from sve_carddb.domains.translations.four_layer_normalizer import PREPARATION_VERSION
from sve_carddb.domains.translations.four_layer_reads import prepare_reads

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping

    from sve_carddb.contracts.four_layer import (
        FormDefinition,
        Frame,
        ModeCountRule,
        Projection,
        Target,
    )
    from sve_carddb.contracts.source_binding import SourceBinding, SourceDescriptor
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField
    from sve_carddb.domains.translations.four_layer_semantics import CardContext


@dataclass(frozen=True)
class PreparedField:
    derived: DerivedField
    interfaces: tuple[Projection | None, ...]
    mode_rules: tuple[tuple[ModeCountRule, ...], ...] = ()
    families: tuple[PreparationFamily, ...] = ()

    @property
    def registry(self) -> Registry:
        """Explicit family registration keeps parallel producers out of shared modules."""
        return DEFAULT_REGISTRY.extend(
            tuple(d for family in self.families for d in family.descriptors)
        )

    def bind(self, ordinal: int) -> tuple[Frame, SourceBinding]:
        """Build preparation identities from the independently derived source evidence."""
        frame, binding = self.derived.recognized[ordinal].bind_version(
            self.derived.field.source,
            self.derived.field.parts[ordinal],
            normalizer_version=PREPARATION_VERSION,
        )
        interface = self.interfaces[ordinal]
        if interface is not None:
            frame = frame.model_copy(update={"projection": interface})
        if self.mode_rules and self.mode_rules[ordinal]:
            frame = frame.model_copy(update={"mode_rules": self.mode_rules[ordinal]})
            content_hash = hash_payload(
                frame.payload(self.derived.field.parts[ordinal].canonical_source)
            )
            frame = frame.model_copy(
                update={"content_hash": content_hash, "id": "frame:" + content_hash}
            )
            binding = binding.model_copy(update={"frame_id": frame.id})
            binding = binding.model_copy(
                update={"id": "bind:" + hash_payload(binding.payload())}
            )
        return frame, binding

    def verify(  # ruff: ignore[too-many-arguments] -- optional authored targets need their own language and forms
        self,
        raw: str,
        classifier: Classifier,
        frame: Frame,
        binding: SourceBinding,
        *,
        context: CardContext | None = None,
        target: Target | None = None,
        lang: str = "zh-Hant",
        forms: Mapping[tuple[str, str], FormDefinition] | None = None,
    ) -> None:
        """Reconstruct exact source evidence; a version label never proves v3 validity."""
        if frame.source.normalizer_version != PREPARATION_VERSION:
            raise ValueError("Preparation requires the explicit v3 normalizer version")
        registry = self.registry
        expected = derive_preparation(
            raw,
            self.derived.field.source,
            classifier,
            context=context,
            families=self.families,
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
            {**classifier.domains, DOMAIN_CODE: DOMAIN, **registry.domains},
            normalizer_version=PREPARATION_VERSION,
        )
        reconstructed = bind_recognized(
            frame, self.derived.field, part, found, mixed=True
        )
        if (
            reconstructed != binding
            or frame.semantic_variant != expected_frame.semantic_variant
            or frame.projection != expected_frame.projection
            or frame.mode_rules != expected_frame.mode_rules
        ):
            raise ValueError(
                "Preparation binding differs from exact source construction"
            )
        verify_quantities(frame, binding, scopes={}, registry=registry)
        if target is not None:
            target.verify(
                frame.leaf_schema,
                lang,
                forms or {},
                dependencies=target_dependencies(frame, binding, registry=registry),
            )
            self.derived.verify_target(binding, target)


@dataclass(frozen=True)
class PreparationFamily:
    name: str
    descriptors: tuple[Descriptor, ...]
    prepare: Callable[
        [str, PreparedField, Classifier, CardContext | None], PreparedField
    ]


def derive_preparation(
    raw: str,
    source: SourceDescriptor,
    classifier: Classifier,
    *,
    context: CardContext | None = None,
    families: tuple[PreparationFamily, ...] = (),
) -> PreparedField:
    """Apply sealed source families only through the explicit preparation entry."""
    derived = derive_source(raw, source, classifier, context=context)
    derived = prepare_capacities(raw, derived, classifier)
    derived = prepare_parameters(raw, derived, classifier)
    derived = prepare_reads(raw, derived, classifier)
    interfaces = tuple(
        constant_choice_interface(
            raw, part, found, classifier.candidate(raw, original).slots
        )
        if part.source_span.role == "body"
        and not found.issues
        and {"choice_mode_count", "choice_index"}
        <= {slot.role for slot in found.schema.slots}
        and source.field in {"effect", "section"}
        and isinstance(source.owner, (FaceRevisionOwner, PrintingFaceOwner))
        else None
        for part, original, found in zip(
            derived.field.parts, derived.original.parts, derived.recognized, strict=True
        )
    )
    prepared = PreparedField(derived, interfaces)
    if len({family.name for family in families}) != len(families):
        raise ValueError("Preparation family names must be unique")
    for family in families:
        prepared = family.prepare(raw, prepared, classifier, context)
    if len(prepared.interfaces) != len(prepared.derived.field.parts):
        raise ValueError("Preparation interfaces must preserve source part order")
    if prepared.mode_rules and len(prepared.mode_rules) != len(
        prepared.derived.field.parts
    ):
        raise ValueError("Mode rules must preserve source part order")
    return replace(prepared, families=families)
