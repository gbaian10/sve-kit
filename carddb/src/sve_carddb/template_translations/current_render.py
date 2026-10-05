"""Render only complete current fields, using finite placeholders and exact concepts."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.template_parameters.models import SourceSpan
from sve_carddb.template_parameters.spans import Located, verify
from sve_carddb.template_translations.definitions import payload
from sve_carddb.template_translations.text import Literal, Parameter
from sve_carddb.template_translations.text import parse as parse_text

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.template_translations.current import Validated
    from sve_carddb.template_translations.current_models import (
        DefinitionRecord,
        TranslationRecord,
        VariantRecord,
    )
    from sve_carddb.template_translations.members import Reconstructed

    type Labels = Mapping[tuple[str, str, str], Label]


@dataclass(frozen=True)
class Label:
    kind: str
    identifier: str
    lang: str
    text: str
    origin: str
    low_confidence: bool
    emphasis: bool | None
    variant_key: str = "default"

    def __post_init__(self) -> None:
        """Reject invalid quality and empty reference labels before selection."""
        if self.origin not in {"official", "project", "machine"} or not self.text:
            raise ValueError("Invalid current reference label")


@dataclass(frozen=True)
class Binding:
    identifier: str
    ordinal: int
    definition: DefinitionRecord
    member: Reconstructed
    params: bytes

    def source_span(self) -> dict[str, JsonValue]:
        """Keep the actual member span, not the representative definition span."""
        return object_value(
            parse(canonical(self.member.candidate.source_span.model_dump(mode="json")))
        )


@dataclass(frozen=True)
class ReferenceUse:
    label: Label
    start: int
    end: int


@dataclass(frozen=True)
class Rendered:
    context_id: str
    target_lang: str
    source_hash: str
    text: str
    bindings: tuple[Binding, ...]
    templates: tuple[TranslationRecord | VariantRecord, ...]
    references: tuple[ReferenceUse, ...]
    dependency_key: bytes
    origin: str
    low_confidence: bool

    def identity(self) -> tuple[str, int]:
        """Use render-v2 without private notes or approval dependencies."""
        payload = {
            "recipe": "render-v2",
            "context_id": self.context_id,
            "target_lang": self.target_lang,
            "dependency_key": parse(self.dependency_key),
            "text": self.text,
            "origin": self.origin,
            "authority": "unofficial",
            "low_confidence": self.low_confidence,
        }
        checksum = digest(canonical(payload))[7:]
        return "tr:" + checksum, int(checksum[:13], 16)


@dataclass(frozen=True)
class Result:
    rendered: Rendered | None
    issues: tuple[str, ...]


def bindings(
    validated: Validated, ref: SourceRef, context_id: str, text: str
) -> tuple[Binding, ...] | None:
    """Require an exact whole source field; an unresolved position blocks partial translation."""
    if digest(text.encode()) != ref.text_hash:
        raise ValueError("Template field source hash differs from its context")
    field = validated.field_members.get(ref, ())
    if not text:
        if field:
            raise ValueError("Empty template field cannot have source bindings")
        return ()
    if not field:
        return None
    if any(m.field_text != text for m in field):
        raise ValueError("Template field bytes differ from verified source")
    matches = validated.matched_definitions
    if any(m.entry.id not in matches for m in field):
        return None
    verify(
        text,
        tuple(
            Located(
                i,
                m.entry.line_ordinal,
                SourceSpan.model_validate(m.candidate.source_span.model_dump()),
            )
            for i, m in enumerate(field)
        ),
    )
    result = []
    for ordinal, member in enumerate(field):
        definition = matches[member.entry.id]
        member.verify_schema(definition.data.parameter_schema)
        params = _params(member, definition)
        payload: dict[str, JsonValue] = {
            "recipe": "binding-v1",
            "context_id": context_id,
            "ordinal": ordinal,
            "template_id": definition.data.id,
            "params": parse(params),
            "source_span": member.candidate.source_span.model_dump(mode="json"),
        }
        result.append(
            Binding(
                "bind:" + digest(canonical(payload))[7:],
                ordinal,
                definition,
                member,
                params,
            )
        )
    return tuple(result)


def _params(member: Reconstructed, definition: DefinitionRecord) -> bytes:
    result: dict[str, JsonValue] = {}
    for slot in definition.data.parameter_schema.slots:
        hint = next(h for h in member.hints if h.occurrence == slot.occurrences[0])
        if slot.type == "uint":
            result[slot.name] = hint.value
        elif slot.type == "literal":
            result[slot.name] = "".join(
                member.field_text[s.start : s.end] for s in hint.source_segments
            )
        else:
            target = hint.target
            if target is None:
                raise ValueError("Verified template reference has no target")
            if "card_name" in target:
                result[slot.name] = {"kind": "card_name", "text": target["card_name"]}
                continue
            keys = (
                ("kind", "vocabulary_kind", "vocabulary_code")
                if slot.reference_kind == "vocabulary"
                else ("kind", "id")
            )
            result[slot.name] = {key: target[key] for key in keys}
    return canonical(result)


def render(  # ruff: ignore[too-many-arguments] -- complete source, selected targets and referenced labels jointly determine one context
    validated: Validated,
    ref: SourceRef,
    context_id: str,
    source_text: str,
    lang: str,
    labels: Labels,
    *,
    variants: tuple[tuple[str, str], ...] = (),
) -> Result:
    """Missing labels or any fragment return the original context; low confidence stays active."""
    plan = bindings(validated, ref, context_id, source_text)
    if plan is None:
        return Result(None, ("unmatched_template_source", *_pending(validated, ref)))
    selected = dict(variants)
    if len(selected) != len(variants) or not set(selected) <= {
        b.definition.data.id for b in plan
    }:
        raise ValueError("Template pin must refer to an actual unique field dependency")
    targets: list[TranslationRecord | VariantRecord] = []
    chunks: dict[int, tuple[str, tuple[ReferenceUse, ...]]] = {}
    for binding in plan:
        chunk = _chunk(validated, binding, selected, lang, labels)
        if isinstance(chunk, str):
            return Result(None, (chunk,))
        targets.append(chunk[0])
        chunks[binding.ordinal] = chunk[1]
    text, uses = _assemble(plan, chunks)
    low = (
        any(b.definition.low_confidence or b.member.low_confidence for b in plan)
        or any(t.low_confidence for t in targets)
        or any(u.label.low_confidence for u in uses)
    )
    origins = [t.origin for t in targets] + [u.label.origin for u in uses]
    origin = "machine" if "machine" in origins else "project"
    dependencies: list[JsonValue] = []
    for binding, target in zip(plan, targets, strict=True):
        dependencies.extend(
            [
                [
                    "definition",
                    binding.definition.data.id,
                    parse(payload(binding.member, binding.definition)),
                    binding.definition.origin,
                    binding.definition.low_confidence,
                ],
                [
                    "binding",
                    binding.identifier,
                    parse(binding.params),
                    binding.source_span(),
                ],
                [
                    "translation",
                    target.record_key,
                    target.data.model_dump(mode="json"),
                    target.origin,
                    target.low_confidence,
                ],
            ]
        )
    for use in uses:
        label = use.label
        dependencies.append(
            [
                "label",
                label.kind,
                label.identifier,
                label.lang,
                label.variant_key,
                label.text,
                label.origin,
                label.low_confidence,
                label.emphasis,
            ]
        )
    dependency = canonical(sorted(dependencies, key=canonical))
    return Result(
        Rendered(
            context_id,
            lang,
            ref.text_hash,
            text,
            plan,
            tuple(targets),
            uses,
            dependency,
            origin,
            low,
        ),
        (),
    )


def _pending(validated: Validated, ref: SourceRef) -> list[str]:
    """Report why unmatched positions stayed unresolved, without their source text."""
    return sorted(
        {
            cause
            for member in validated.field_members.get(ref, ())
            if member.entry.id not in validated.matched_definitions
            for cause in member.pending
        }
    )


def _chunk(
    validated: Validated,
    binding: Binding,
    selected: dict[str, str],
    lang: str,
    labels: Labels,
) -> (
    tuple[TranslationRecord | VariantRecord, tuple[str, tuple[ReferenceUse, ...]]] | str
):
    """Render one binding or name the reason the whole field keeps its original."""
    definition = binding.definition
    if lang == definition.data.source_lang:
        return "same_source_language"
    variant = selected.get(definition.data.id, "default")
    target = validated.target_records.get((definition.data.id, lang, variant))
    if target is None:
        if variant != "default":
            raise ValueError("Template pin references a missing current variant")
        return "missing_template_translation"
    try:
        parts = parse_text(target.data.text, definition.data.parameter_schema)
    except ValueError:
        return "invalid_template_translation"
    rendered = _fragment(binding, parts, lang, labels)
    return "missing_term_translation" if rendered is None else (target, rendered)


def _fragment(
    binding: Binding,
    parts: tuple[Literal | Parameter, ...],
    lang: str,
    labels: Labels,
) -> tuple[str, tuple[ReferenceUse, ...]] | None:
    params = object_value(parse(binding.params))
    output = ""
    uses: list[ReferenceUse] = []
    for part in parts:
        if isinstance(part, Literal):
            output += part.text
        elif isinstance(part, Parameter):
            value = params[part.name]
            if isinstance(value, dict):
                kind = str(value["kind"])
                if kind == "card_name":
                    name = str(value["text"])
                    # Without an existing translation the reader still gets the exact source name.
                    label = labels.get((kind, name, lang)) or Label(
                        kind, name, lang, name, "official", True, None
                    )
                else:
                    identifier = (
                        str(value["id"])
                        if "id" in value
                        else str(value["vocabulary_kind"])
                        + ":"
                        + str(value["vocabulary_code"])
                    )
                    found = labels.get((kind, identifier, lang))
                    if found is None:
                        return None
                    label = found
                uses.append(
                    ReferenceUse(label, len(output), len(output) + len(label.text))
                )
                output += label.text
            elif type(value) is int or isinstance(value, str):
                output += str(value)
            else:
                raise ValueError("Invalid verified template parameter value")
    return output, tuple(uses)


def _assemble(
    plan: tuple[Binding, ...], chunks: dict[int, tuple[str, tuple[ReferenceUse, ...]]]
) -> tuple[str, tuple[ReferenceUse, ...]]:
    output = ""
    uses: list[ReferenceUse] = []
    for binding in plan:
        if binding.member.candidate.source_span.anchor is not None:
            continue
        order = [
            binding.ordinal,
            *(
                b.ordinal
                for b in plan
                if b.member.candidate.source_span.anchor == binding.ordinal
            ),
        ]
        for ordinal in order:
            fragment, references = chunks[ordinal]
            uses.extend(
                ReferenceUse(u.label, u.start + len(output), u.end + len(output))
                for u in references
            )
            output += fragment
    return output, tuple(uses)
