"""Closed format-three translation inputs shared by glossary, forms and frame consumers."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Annotated, Literal, Self, override

from pydantic import Field, computed_field, field_validator, model_validator

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    Code,
    FormDefinition,
    Frame,
    FrameId,
    GlossaryReference,
    Lang,
    Target,
)
from sve_carddb.contracts.source_binding import SourceSpan, TypedValue
from sve_carddb.core.authored import check_path, read, require_directory, shards
from sve_carddb.core.json import canonical, parse
from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.domains.translations.four_layer_normalizer import VERSION
from sve_carddb.domains.translations.four_layer_render import validate_form
from sve_carddb.domains.translations.glossary.records import (
    AssignmentRecord,
    ChoiceData,
    ChoiceRecord,
    ConceptEvidence,
    ConceptRecord,
    EmphasisRecord,
    TermData,
    TermRecord,
)
from sve_carddb.domains.translations.models import AuthoredValue, Quality, SourceValue
from sve_carddb.domains.translations.templates.records import CandidateRecord

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from pydantic import JsonValue


VariantKey = Annotated[str, Field(pattern=r"^[a-z][a-z0-9_.-]*\Z")]


class _Record[T: RecordData](Quality):
    kind: Code
    data: T

    def _selection_key(self) -> list[JsonValue]:
        raise NotImplementedError

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes the property; mypy cannot compose decorators.
    @property
    def record_key(self) -> str:
        """Selection keys exclude quality flags and editable display content."""
        return canonical(self._selection_key()).decode()


class FrameRecord(_Record[Frame]):
    kind: Literal["sentence_template"]

    @override
    def _selection_key(self) -> list[JsonValue]:
        return [self.kind, self.data.id]


class TemplateTarget(RecordData):
    template_id: FrameId
    lang: Lang
    target: Target


class TargetRecord(_Record[TemplateTarget]):
    kind: Literal["template_translation"]

    @override
    def _selection_key(self) -> list[JsonValue]:
        return [self.kind, self.data.template_id, self.data.lang]


class TemplateVariant(TemplateTarget):
    variant_key: VariantKey

    @model_validator(mode="after")
    def _named(self) -> Self:
        if self.variant_key == "default":
            raise ValueError("Named template variant cannot be default")
        return self


class TargetVariantRecord(_Record[TemplateVariant]):
    kind: Literal["template_translation_variant"]

    @override
    def _selection_key(self) -> list[JsonValue]:
        return [self.kind, self.data.template_id, self.data.lang, self.data.variant_key]


class FormRecord(_Record[FormDefinition]):
    kind: Literal["translation_form"]

    @override
    def _selection_key(self) -> list[JsonValue]:
        return [self.kind, self.data.id, self.data.lang]


class ChoiceVariant(ChoiceData):
    variant_key: VariantKey

    @model_validator(mode="after")
    def _named(self) -> Self:
        if self.variant_key == "default" or self.value is None:
            raise ValueError("Named glossary variant must contain a usable value")
        return self


class ChoiceVariantRecord(_Record[ChoiceVariant]):
    kind: Literal["glossary_choice_variant"]

    @override
    def _selection_key(self) -> list[JsonValue]:
        return [self.kind, self.data.term_id, self.data.lang, self.data.variant_key]


class ContextKey(RecordData):
    source_unit_id: Text
    variant: VariantKey


class Match(RecordData):
    frame_id: FrameId
    source_span: SourceSpan
    values: dict[Code, TypedValue]


class TemplateMatch(RecordData):
    context_key: ContextKey
    source_hash: Hash
    matches: tuple[Match, ...] | None


class MatchRecord(_Record[TemplateMatch]):
    kind: Literal["template_match"]

    @override
    def _selection_key(self) -> list[JsonValue]:
        return [self.kind, self.data.context_key.model_dump(mode="json")]


class PinnedTemplate(RecordData):
    template_id: FrameId
    lang: Lang
    variant_key: VariantKey


class PinnedTerm(RecordData):
    term_id: Text
    lang: Lang
    variant_key: VariantKey


class Override(RecordData):
    context_key: ContextKey
    lang: Lang
    action: Literal["suppress", "pin", "default"]
    templates: tuple[PinnedTemplate, ...]
    terms: tuple[PinnedTerm, ...]
    reason: Text

    @model_validator(mode="after")
    def _action(self) -> Self:
        if not self.reason.strip():
            raise ValueError("Translation override reason must be nonblank")
        if self.action != "pin" and (self.templates or self.terms):
            raise ValueError("Only pin overrides can select reusable values")
        for selections in (self.templates, self.terms):
            keys = tuple(canonical(s.model_dump(mode="json")) for s in selections)
            if len(keys) != len(set(keys)):
                raise ValueError("Pin selections must be unique")
            if any(s.lang != self.lang for s in selections):
                raise ValueError("Pin selection language differs from override")
        return self


class OverrideRecord(_Record[Override]):
    kind: Literal["translation_override"]

    @override
    def _selection_key(self) -> list[JsonValue]:
        return [
            self.kind,
            self.data.context_key.model_dump(mode="json"),
            self.data.lang,
        ]


ChoiceValue = Annotated[AuthoredValue | SourceValue, Field(discriminator="kind")]


class SymbolBasis(RecordData):
    code: Code
    parameter_schema_hash: Hash
    source_localization_hash: Hash


class SymbolChoiceValues(RecordData):
    name: ChoiceValue
    tooltip: ChoiceValue
    copy_pattern: ChoiceValue


class SymbolEvidence(RecordData):
    name: tuple[ConceptEvidence, ...]
    tooltip: tuple[ConceptEvidence, ...]
    copy_pattern: tuple[ConceptEvidence, ...]


class SymbolChoice(RecordData):
    symbol_id: Text
    lang: Lang
    symbol_basis: SymbolBasis
    value: SymbolChoiceValues | None
    concept_evidence: SymbolEvidence


class SymbolChoiceRecord(_Record[SymbolChoice]):
    kind: Literal["symbol_localization_choice"]

    @override
    def _selection_key(self) -> list[JsonValue]:
        return [self.kind, self.data.symbol_id, self.data.lang]


Record = Annotated[
    TermRecord
    | ChoiceRecord
    | EmphasisRecord
    | AssignmentRecord
    | ConceptRecord
    | FrameRecord
    | TargetRecord
    | TargetVariantRecord
    | CandidateRecord
    | FormRecord
    | ChoiceVariantRecord
    | MatchRecord
    | OverrideRecord
    | SymbolChoiceRecord,
    Field(discriminator="kind"),
]


class Shard(RecordData):
    format: Literal[3]
    kind: Literal["translation_shard"]
    records: tuple[Record, ...]

    @field_validator("format", mode="before")
    @classmethod
    def _integer(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Four-layer authored format must be integer three")
        return value


def shard(raw: bytes) -> Shard:
    """Strict JSON parsing rejects duplicate keys before Pydantic can replace a value."""
    try:
        result = Shard.model_validate_json(canonical(parse(raw)))
    except ValueError, TypeError:
        raise ValueError("Invalid four-layer authored shard") from None
    keys = tuple(r.record_key for r in result.records)
    if len(set(keys)) != len(keys):
        raise ValueError("Duplicate four-layer authored selection key")
    return result


@dataclass(frozen=True)
class Inputs:
    files: tuple[tuple[str, bytes, bytes], ...]
    records: tuple[Record, ...]


_PATH = re.compile(
    r"translations/(?:(glossary|overrides)/[A-Za-z0-9_-]+|templates/(definitions|values|candidates)|(forms))/[0-9]{3,}\.yaml\Z"
)
_KINDS = {
    "glossary": frozenset(
        {
            "glossary_term",
            "glossary_choice",
            "glossary_choice_variant",
            "glossary_emphasis_choice",
            "symbol_localization_choice",
        }
    ),
    "overrides": frozenset(
        {
            "context_assignment",
            "card_name_concept",
            "template_match",
            "translation_override",
        }
    ),
    "definitions": frozenset({"sentence_template"}),
    "values": frozenset({"template_translation", "template_translation_variant"}),
    "candidates": frozenset({"template_translation_candidate"}),
    "forms": frozenset({"translation_form"}),
}


def from_files(files: tuple[tuple[str, bytes, bytes], ...]) -> Inputs:
    """A single loaded closure supplies every consumer; filenames do not resolve duplicate keys."""
    selected: dict[str, Record] = {}
    for path, _, content in files:
        match = _PATH.fullmatch(path)
        if match is None:
            raise ValueError("Unsupported four-layer authored shard path")
        area = next(g for g in match.groups() if g is not None)
        for record in shard(content).records:
            if record.kind not in _KINDS[area]:
                raise ValueError(
                    "Four-layer authored record is outside its kind's area"
                )
            if record.record_key in selected:
                raise ValueError("Duplicate four-layer authored selection key")
            selected[record.record_key] = record
    records = tuple(selected[key] for key in sorted(selected))
    _references(records)
    return Inputs(files, records)


def read_inputs(root: Path) -> Inputs:
    """Only declared working-tree areas and numeric shards enter the build."""
    areas = (
        "translations/glossary",
        "translations/overrides",
        "translations/templates",
    )
    files = list(shards(root, areas, optional=areas))
    forms = root / "translations/forms"
    check_path(root, forms)
    if forms.exists():
        require_directory(root, forms)
        for path in sorted(forms.glob("*.yaml")):
            if re.fullmatch(r"[0-9]{3,}\.yaml", path.name):
                raw, content = read(path, root=root)
                files.append((path.relative_to(root).as_posix(), raw, content))
    return from_files(tuple(sorted(files)))


def _references(records: tuple[Record, ...]) -> None:
    frames = {r.data.id: r.data for r in records if isinstance(r, FrameRecord)}
    forms = {
        (r.data.id, r.data.lang): r.data for r in records if isinstance(r, FormRecord)
    }
    for definition in frames.values():
        if definition.source.normalizer_version != VERSION:
            raise ValueError("Unsupported four-layer authored normalizer version")
    for form in forms.values():
        validate_form(form)
    for record in records:
        if isinstance(record, (TargetRecord, TargetVariantRecord)):
            frame = frames.get(record.data.template_id)
            if frame is None:
                raise ValueError("Four-layer target requires its frame definition")
            if record.data.lang == frame.source.source_lang:
                raise ValueError(
                    "Target language must differ from frame source language"
                )
            record.data.target.verify(frame.leaf_schema, record.data.lang, forms)
        elif isinstance(record, MatchRecord):
            _match_references(record.data, frames)
    _glossary_references(records)
    _pins(records)


def _match_references(data: TemplateMatch, frames: Mapping[str, Frame]) -> None:
    for selection in data.matches or ():
        frame = frames.get(selection.frame_id)
        if frame is None or selection.source_span.role != frame.role:
            raise ValueError("Manual template match requires its exact frame role")
        slots = {s.name: s for s in frame.leaf_schema.slots}
        if set(selection.values) - slots.keys() or any(
            s.required and s.name not in selection.values for s in slots.values()
        ):
            raise ValueError("Manual template match has missing or unknown leaf values")


def _glossary_references(records: tuple[Record, ...]) -> None:  # ruff: ignore[complex-structure, too-many-branches] -- each closed record kind preserves its distinct existing concept and evidence constraints
    terms = {r.data.id: r.data for r in records if isinstance(r, TermRecord)}
    if len({t.concept_key for t in terms.values()}) != len(terms):
        raise ValueError("Duplicate glossary concept key")
    for record in records:
        if isinstance(record, FrameRecord):
            _leaf_references(record.data, terms)
        elif isinstance(record, TermRecord):
            if record.data.id != "term:" + record.data.concept_key:
                raise ValueError("Glossary ID differs from its concept key")
        elif isinstance(record, (ChoiceRecord, ChoiceVariantRecord)):
            if record.data.term_id not in terms:
                raise ValueError("Glossary choice references an absent concept")
            if (
                record.origin == "official"
                and record.data.value is not None
                and not record.data.concept_evidence
            ):
                raise ValueError(
                    "Official glossary choice requires same-concept evidence"
                )
        elif isinstance(record, EmphasisRecord):
            term = terms.get(record.data.term_id)
            if term is None or term.category != "rule_term":
                raise ValueError("Emphasis requires a rule-term concept")
        elif isinstance(record, ConceptRecord):
            term = terms.get(record.data.term_id or "")
            if record.data.term_id is not None and (
                term is None or term.category != "card_name"
            ):
                raise ValueError("Name concept requires a card-name glossary term")
        elif isinstance(record, AssignmentRecord):
            term = terms.get("term:" + (record.data.concept_key or ""))
            if record.data.concept_key is not None and (
                term is None or term.category != "card_name"
            ):
                raise ValueError("Name assignment requires a card-name concept")
            if record.data.variant != "default" and (
                not record.data.reason.strip() or term is None
            ):
                raise ValueError(
                    "Nondefault name assignment requires a reason and concept"
                )


def _leaf_references(frame: Frame, terms: Mapping[str, TermData]) -> None:
    for slot in frame.leaf_schema.slots:
        for reference in slot.domain.values:
            if isinstance(reference, GlossaryReference) and reference.key not in terms:
                raise ValueError("Frame leaf domain references an absent glossary term")
            if isinstance(reference, CardNameReference):
                term = terms.get(reference.term_id)
                if term is None or term.category != "card_name":
                    raise ValueError(
                        "Frame card-name leaf requires a card-name concept"
                    )


def _pins(records: tuple[Record, ...]) -> None:
    templates = {
        (
            r.data.template_id,
            r.data.lang,
            r.data.variant_key if isinstance(r, TargetVariantRecord) else "default",
        )
        for r in records
        if isinstance(r, (TargetRecord, TargetVariantRecord))
    }
    terms = {
        (
            r.data.term_id,
            r.data.lang,
            r.data.variant_key if isinstance(r, ChoiceVariantRecord) else "default",
        )
        for r in records
        if isinstance(r, (ChoiceRecord, ChoiceVariantRecord))
        and r.data.value is not None
    }
    for record in records:
        if isinstance(record, OverrideRecord) and record.data.action == "pin":
            if any(
                (v.template_id, v.lang, v.variant_key) not in templates
                for v in record.data.templates
            ):
                raise ValueError(
                    "Pin requires an existing reusable template translation"
                )
            if any(
                (v.term_id, v.lang, v.variant_key) not in terms
                for v in record.data.terms
            ):
                raise ValueError("Pin requires an existing usable glossary choice")
