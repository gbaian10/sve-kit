"""Derive and verify the complete sealed source construction."""

from dataclasses import dataclass, field, replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    FaceRevisionOwner,
    PrintingFaceOwner,
    hash_payload,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_analysis import (
    SourceAnalysis,
    analyze_source,
)
from sve_carddb.domains.translations.four_layer_choices import constant_choice_interface
from sve_carddb.domains.translations.four_layer_context import (
    DEFAULT_REGISTRY,
    Descriptor,
    Registry,
)
from sve_carddb.domains.translations.four_layer_damage import prepare_damage
from sve_carddb.domains.translations.four_layer_destroy import prepare_destroy
from sve_carddb.domains.translations.four_layer_draw import prepare_draw
from sve_carddb.domains.translations.four_layer_expressions import (
    DOMAIN,
    DOMAIN_CODE,
    target_dependencies,
    verify_quantities,
)
from sve_carddb.domains.translations.four_layer_fixed_actions import (
    prepare_fixed_actions,
)
from sve_carddb.domains.translations.four_layer_identity import seal
from sve_carddb.domains.translations.four_layer_matching import bind_recognized
from sve_carddb.domains.translations.four_layer_n2_counters import prepare_capacities
from sve_carddb.domains.translations.four_layer_n2_parameters import prepare_parameters
from sve_carddb.domains.translations.four_layer_n3_context import DOMAINS as N3_DOMAINS
from sve_carddb.domains.translations.four_layer_n3_ports import verify_pool_allocation
from sve_carddb.domains.translations.four_layer_normalizer import VERSION
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
    from sve_carddb.contracts.source_binding import (
        ClosedDomain,
        SourceBinding,
        SourceDescriptor,
    )
    from sve_carddb.domains.translations.four_layer_classification import (
        Classifier,
        Recognized,
    )
    from sve_carddb.domains.translations.four_layer_normalizer import SourceField
    from sve_carddb.domains.translations.four_layer_semantics import CardContext


@dataclass(frozen=True)
class DerivedField:
    derived: SourceAnalysis
    interfaces: tuple[Projection | None, ...]
    mode_rules: tuple[tuple[ModeCountRule, ...], ...] = ()
    families: tuple[SourceFamily, ...] = ()
    source_domains: Mapping[str, ClosedDomain] = field(default_factory=dict)

    @property
    def field(self) -> SourceField:
        """Retain the exact transformed source partition."""
        return self.derived.field

    @property
    def recognized(self) -> tuple[Recognized, ...]:
        """Expose sealed source leaves to the normal compiler."""
        return self.derived.recognized

    @property
    def pending_causes(self) -> tuple[tuple[str, ...], ...]:
        """Unresolved semantics remain visible to the build report."""
        return self.derived.pending_causes

    def verify_target(self, binding: SourceBinding, target: Target) -> None:
        """Keep existing operand constraints on selected sentence targets."""
        self.derived.verify_target(binding, target)

    @property
    def registry(self) -> Registry:
        """The source providers and renderer share one sealed descriptor registry."""
        return DEFAULT_REGISTRY.extend(
            tuple(d for family in self.families for d in family.descriptors)
        )

    def bind(self, ordinal: int) -> tuple[Frame, SourceBinding]:
        """Build identities from independently derived source evidence."""
        frame, binding = self.derived.recognized[ordinal].bind(
            self.derived.field.source,
            self.derived.field.parts[ordinal],
            normalizer_version=VERSION,
            semantic_catalog=(self.source_domains, self.registry),
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
        return seal(
            frame,
            binding,
            self.field.parts[ordinal].canonical_source,
            self.source_domains,
            self.registry,
        )

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
        """Reconstruct exact source evidence; a version label never proves validity."""
        if frame.source.normalizer_version != VERSION:
            raise ValueError("Unsupported four-layer normalizer version")
        expected = derive_source(
            raw,
            self.derived.field.source,
            classifier,
            context=context,
        )
        if self != expected:
            raise ValueError(
                "Source derivation differs from reconstructed source evidence"
            )
        expected.verify_binding(
            raw, classifier, frame, binding, target=target, lang=lang, forms=forms
        )

    def verify_binding(  # ruff: ignore[too-many-arguments] -- selected targets need their language and forms at the source boundary
        self,
        raw: str,
        classifier: Classifier,
        frame: Frame,
        binding: SourceBinding,
        *,
        target: Target | None = None,
        lang: str = "zh-Hant",
        forms: Mapping[tuple[str, str], FormDefinition] | None = None,
    ) -> None:
        """Reuse this field's independently derived evidence for each source occurrence."""
        registry = self.registry
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
            {
                **classifier.domains,
                DOMAIN_CODE: DOMAIN,
                **N3_DOMAINS,
                **registry.domains,
            },
            normalizer_version=VERSION,
        )
        reconstructed = bind_recognized(frame, self.derived.field, part, found)
        if (
            reconstructed != binding
            or canonical(frame.source.semantic_definitions)
            != canonical(expected_frame.source.semantic_definitions)
            or frame.semantic_variant != expected_frame.semantic_variant
            or frame.projection != expected_frame.projection
            or frame.mode_rules != expected_frame.mode_rules
        ):
            raise ValueError(
                "Binding differs from the complete current source construction"
            )
        verify_quantities(frame, binding, scopes={}, registry=registry)
        verify_pool_allocation(frame, binding)
        if target is not None:
            self.derived.verify_target(binding, target)
            if forms is not None:
                target.verify(
                    frame.leaf_schema,
                    lang,
                    forms,
                    dependencies=target_dependencies(frame, binding, registry=registry),
                )


@dataclass(frozen=True)
class SourceFamily:
    name: str
    descriptors: tuple[Descriptor, ...]
    prepare: Callable[[str, DerivedField, Classifier, CardContext | None], DerivedField]


def source_families() -> tuple[SourceFamily, ...]:
    """Lexical and complete bindings must resolve names from the same providers."""
    from sve_carddb.domains.translations.four_layer_integrated import FAMILY  # ruff: ignore[import-outside-top-level] -- providers share this source construction type
    from sve_carddb.domains.translations.four_layer_lexical import FAMILY as LEXICAL  # ruff: ignore[import-outside-top-level] -- lexical completion depends on the shared derivation boundary
    from sve_carddb.domains.translations.four_layer_n3_sources import FAMILY as N3  # ruff: ignore[import-outside-top-level] -- source adapters depend on the shared derivation boundary

    return FAMILY, N3, LEXICAL


def derive_source(
    raw: str,
    source: SourceDescriptor,
    classifier: Classifier,
    *,
    context: CardContext | None = None,
) -> DerivedField:
    """Apply every sealed family through the sole production source entry."""
    derived = analyze_source(raw, source, classifier, context=context)
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
    families = source_families()
    prepared = DerivedField(derived, interfaces, source_domains=classifier.domains)
    for family in families:
        prepared = family.prepare(raw, prepared, classifier, context)
    prepared = prepare_draw(raw, prepared, context)
    prepared = prepare_damage(raw, prepared, context)
    prepared = prepare_destroy(raw, prepared, context)
    prepared = prepare_fixed_actions(raw, prepared, context)
    if len(prepared.interfaces) != len(prepared.derived.field.parts):
        raise ValueError("Source interfaces must preserve source part order")
    if prepared.mode_rules and len(prepared.mode_rules) != len(
        prepared.derived.field.parts
    ):
        raise ValueError("Mode rules must preserve source part order")
    return replace(prepared, families=families)
