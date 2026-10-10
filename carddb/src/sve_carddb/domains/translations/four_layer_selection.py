"""Explicit matches and reusable wording choices retain exact source applicability."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import CardNameReference, GlossaryReference
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_authored import (
    ChoiceVariantRecord,
    FrameRecord,
    MatchRecord,
    OverrideRecord,
    TargetRecord,
    TargetVariantRecord,
)
from sve_carddb.domains.translations.four_layer_matching import Frames
from sve_carddb.domains.translations.four_layer_render import (
    Label,
    Renderer,
    SelectedTarget,
)
from sve_carddb.domains.translations.glossary.evidence import validate_choice
from sve_carddb.domains.translations.glossary.records import ChoiceRecord

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database
    from sve_carddb.domains.translations.four_layer_authored import Inputs
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_pipeline import CompiledField
    from sve_carddb.domains.translations.sources import Sources


@dataclass(frozen=True)
class Selection:
    renderer: Renderer
    targets: Mapping[tuple[str, str], SelectedTarget]
    suppress: bool


class Controls:
    def __init__(self, inputs: Inputs) -> None:
        self.frames = {
            r.data.id: r.data for r in inputs.records if isinstance(r, FrameRecord)
        }
        self.matches = {
            (r.data.context_key.source_unit_id, r.data.context_key.variant): r.data
            for r in inputs.records
            if isinstance(r, MatchRecord)
        }
        self.overrides = {
            (
                r.data.context_key.source_unit_id,
                r.data.context_key.variant,
                r.data.lang,
            ): r.data
            for r in inputs.records
            if isinstance(r, OverrideRecord)
        }
        self.targets = {
            (
                r.data.template_id,
                r.data.lang,
                r.data.variant_key if isinstance(r, TargetVariantRecord) else "default",
            ): SelectedTarget(
                r.data.target,
                r.origin,
                r.low_confidence,
                r.data.variant_key if isinstance(r, TargetVariantRecord) else "default",
            )
            for r in inputs.records
            if isinstance(r, (TargetRecord, TargetVariantRecord))
        }
        self.choices = tuple(
            r for r in inputs.records if isinstance(r, ChoiceVariantRecord)
        )
        self.seen: set[str] = set()
        self.variant_labels: dict[tuple[str, str, str], Label] = {}

    def frames_for(
        self, source_unit_id: str, source_hash: str, automatic: Frames
    ) -> Frames:
        """Manual selection narrows candidates but supplies neither source roles nor leaf values."""
        self.seen.add(source_unit_id)
        manual = self.matches.get((source_unit_id, "default"))
        if manual is None:
            return automatic
        if manual.source_hash != "sha256:" + source_hash:
            raise ValueError("Manual template match has stale source bytes")
        return (
            automatic
            if manual.matches is None
            else Frames(
                {
                    self.frames[m.frame_id].id: self.frames[m.frame_id]
                    for m in manual.matches
                }.values()
            )
        )

    def verify(self, field: CompiledField) -> None:
        """The selected ordered matches must equal independently verified bindings."""
        manual = self.matches.get((field.source.descriptor.source_unit_id, "default"))
        if manual is None or manual.matches is None:
            return
        if not field.complete or len(manual.matches) != len(field.matches):
            raise ValueError(
                "Manual template match does not cover the complete exact field"
            )
        for authored, matched in zip(manual.matches, field.matches, strict=True):
            if matched is None or (
                authored.frame_id,
                authored.source_span,
                authored.values,
            ) != (
                matched.frame.id,
                matched.binding.source_span,
                matched.binding.values,
            ):
                raise ValueError(
                    "Manual template match differs from verified source leaves"
                )

    def verify_closure(self) -> None:
        """Missing or unsupported assigned source contexts are errors, never ignored controls."""
        if any(
            unit not in self.seen or variant != "default"
            for unit, variant in self.matches
        ) or any(
            unit not in self.seen or variant != "default"
            for unit, variant, _lang in self.overrides
        ):
            raise ValueError(
                "Translation selection has no exact current source context"
            )

    def prepare_labels(
        self, db: Database, classifier: Classifier, sources: Sources
    ) -> None:
        """Unused named wording must still prove its exact source and official concept claims."""
        for record in self.choices:
            choice = ChoiceRecord.model_validate_json(
                canonical(
                    record.model_dump(mode="json", exclude={"record_key"})
                    | {
                        "kind": "glossary_choice",
                        "data": record.data.model_dump(
                            mode="json", exclude={"variant_key"}
                        ),
                    }
                )
            )
            term = classifier.terms[record.data.term_id]
            resolved = validate_choice(
                choice, original=term.source_ja, sources=sources, db=db
            )
            if resolved is None:
                raise ValueError("Pinned glossary variant has no usable wording")
            row = db.select(
                "glossary_term", ("emphasis", "low_confidence"), where={"id": term.id}
            )[0]
            bold = row.values["emphasis"] if term.category == "rule_term" else True
            if bold is not None and not isinstance(bold, bool):
                raise TypeError("Glossary emphasis requires a nullable Bool")
            reference = (
                CardNameReference(kind="card_name", term_id=term.id)
                if term.category == "card_name"
                else GlossaryReference(kind="glossary", key=term.id)
            )
            self.variant_labels[term.id, record.data.lang, record.data.variant_key] = (
                Label(
                    reference,
                    record.data.lang,
                    resolved[0],
                    record.origin,
                    record.low_confidence or bool(row.values["low_confidence"]),
                    bold,
                    record.data.variant_key,
                )
            )

    def select(
        self,
        field: CompiledField,
        renderer: Renderer,
        default: Mapping[tuple[str, str], SelectedTarget],
        lang: str,
    ) -> Selection:
        """Only an explicit context pin can select named reusable wording."""
        override = self.overrides.get(
            (field.source.descriptor.source_unit_id, "default", lang)
        )
        if override is None or override.action == "default":
            return Selection(renderer, default, False)
        if override.action == "suppress":
            return Selection(renderer, default, True)
        targets = dict(default)
        selected_labels = dict(renderer.labels)
        frames = {m.frame.id for m in field.matches if m is not None}
        references = {
            canonical(v.model_dump(mode="json"))
            for m in field.matches
            if m is not None
            for v in m.binding.values.values()
            if isinstance(v, (CardNameReference, GlossaryReference))
        }
        for pin in override.templates:
            if pin.template_id not in frames:
                raise ValueError("Pinned template is outside the exact source context")
            targets[pin.template_id, lang] = self.targets[
                pin.template_id, lang, pin.variant_key
            ]
        for term_pin in override.terms:
            if term_pin.variant_key == "default":
                candidates = [
                    v
                    for v in renderer.labels.values()
                    if v.lang == lang
                    and isinstance(v.reference, (CardNameReference, GlossaryReference))
                    and (
                        v.reference.term_id
                        if isinstance(v.reference, CardNameReference)
                        else v.reference.key
                    )
                    == term_pin.term_id
                ]
                if len(candidates) != 1:
                    raise ValueError("Pinned glossary default has no usable label")
                value = candidates[0]
            else:
                value = self.variant_labels[
                    term_pin.term_id, lang, term_pin.variant_key
                ]
            key = canonical(value.reference.model_dump(mode="json"))
            if key not in references:
                raise ValueError(
                    "Pinned glossary term is outside the exact source context"
                )
            selected_labels[key, lang] = value
        return Selection(
            Renderer(
                selected_labels,
                renderer.code_references,
                renderer.forms,
                domains=renderer.domains,
            ),
            targets,
            False,
        )
