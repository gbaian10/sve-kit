"""Compile exact card fields through one adopted source and authored frame closure."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.domains.translations.four_layer_classification import (
    Classifier,
    SourceReferences,
    Term,
)
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_kinds import adopted_kinds
from sve_carddb.domains.translations.four_layer_sources import (
    card_source,
    semantic_context,
)
from sve_carddb.domains.translations.recognition.adopted_references import adopted

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.contracts.source_binding import SourceDescriptor
    from sve_carddb.domains.text_observations.vocabulary import Vocabulary
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField
    from sve_carddb.domains.translations.four_layer_matching import Frames, Matched
    from sve_carddb.domains.translations.four_layer_normalizer import SourceField
    from sve_carddb.domains.translations.four_layer_sources import CardSource
    from sve_carddb.domains.translations.inputs import Snapshot
    from sve_carddb.domains.translations.recognition.rules import Rules
    from sve_carddb.domains.translations.sources import Sources


def prepare_classifier(
    db: Database,
    snapshot: Snapshot,
    sources: Sources,
    vocabulary: Vocabulary,
    rules: Rules,
) -> Classifier:
    """Display labels cannot substitute for the exact adopted glossary and kind evidence."""
    found = adopted(snapshot, sources)
    references = SourceReferences(
        card_names=found.card_names,
        terms=found.terms,
        vocabulary=vocabulary,
        pins=found.pins,
        kind_facts=adopted_kinds(db, snapshot, sources),
    )
    terms = tuple(
        Term(identifier, category, raw)
        for raw, members in sorted(found.terms.items())
        for identifier, category in members
    )
    return Classifier(terms, references, rules)


@dataclass(frozen=True)
class CompiledField:
    source: CardSource
    derived: DerivedField
    matches: tuple[Matched | None, ...]
    low_confidence: tuple[bool, ...]
    issues: tuple[str, ...]

    @property
    def field(self) -> SourceField:
        """Expose the same partition used for source matching and target checks."""
        return self.derived.field

    @property
    def complete(self) -> bool:
        """A usable partial render must never conceal an unclassified source part."""
        return bool(self.matches) and all(self.matches) and not self.issues


def compile_card_field(
    db: Database,
    sources: Sources,
    descriptor: SourceDescriptor,
    classifier: Classifier,
    frames: Frames,
) -> CompiledField:
    """Frozen inventory and exact owner checks precede normalization or target selection."""
    source = card_source(db, sources, descriptor)
    context = semantic_context(db, source)
    derived = derive_source(source.text, descriptor, classifier, context=context)
    field = derived.field
    matches: list[Matched | None] = []
    quality = []
    issues: set[str] = set()
    for part, recognized in zip(field.parts, derived.recognized, strict=True):
        quality.append(recognized.low_confidence)
        if recognized.issues:
            issues.update(recognized.issues)
            matches.append(None)
            continue
        matched = frames.match(derived, part.ordinal)
        if matched is None:
            issues.add("unmatched_source_frame")
        matches.append(matched)
    if not field.parts:
        issues.add("empty_source_field")
    return CompiledField(
        source,
        derived,
        tuple(matches),
        tuple(quality),
        tuple(sorted(issues)),
    )
