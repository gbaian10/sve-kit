"""Explicit A1/A4 preparation family; production and default preparation stay unchanged."""

from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    ContextExpression,
    Expression,
    FaceRevisionOwner,
    LeafSchema,
    PrintingFaceOwner,
    Projection,
    Scope,
)
from sve_carddb.domains.translations.four_layer_n1_leaves import Leaf
from sve_carddb.domains.translations.four_layer_n1_transform import transform
from sve_carddb.domains.translations.four_layer_n2_read_descriptors import DESCRIPTORS
from sve_carddb.domains.translations.four_layer_n2_read_sources import (
    ReadSources,
    regions,
)
from sve_carddb.domains.translations.four_layer_normalizer import SourceField
from sve_carddb.domains.translations.four_layer_preparation import PreparationFamily
from sve_carddb.domains.translations.recognition.provenance import merged

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Port
    from sve_carddb.domains.translations.four_layer_classification import (
        Classifier,
        Recognized,
    )
    from sve_carddb.domains.translations.four_layer_preparation import PreparedField
    from sve_carddb.domains.translations.four_layer_semantics import CardContext


def _retained(found: Recognized, additions: list[Leaf]) -> list[Leaf]:
    repaired = tuple(s for item in additions for group in item.positions for s in group)
    leaves = []
    for slot in found.schema.slots:
        if any(
            a.start < b.end and b.start < a.end
            for a in slot.occurrences
            for b in repaired
        ):
            continue
        occurrences = tuple(o for o in found.occurrences if o.slot == slot.name)
        leaves.append(
            Leaf(
                slot,
                found.values[slot.name],
                tuple(o.canonical_spans for o in occurrences if o.canonical_spans),
                "retained",
                abstract=False,
                unit=occurrences[0].source_unit,
                resolution_rule=occurrences[0].resolution_rule,
                use=next(
                    (
                        o.raw_spans[0].start
                        for o in found.occurrences[
                            found.occurrences.index(occurrences[0]) :
                        ]
                        if o.raw_spans
                    ),
                    0,
                ),
            )
        )
    return [*leaves, *additions]


def _issues(builder: ReadSources, leaves: list[Leaf]) -> tuple[str, ...]:
    covered = merged(
        tuple(
            s
            for item in leaves
            for group in item.positions
            for span in group
            for unit in builder.current.units[span.start : span.end]
            for s in unit.origins
        )
    )
    candidate = builder.classifier.candidate(builder.raw, builder.original)
    issues = set(builder.found.issues)
    reasons = {
        "numeric_role_requires_review",
        "signed_numeric_requires_review",
        "numeric_identifier_requires_review",
        "n0_numeric_construction_unresolved",
        "n0_vocabulary_construction_unresolved",
    }
    for reason in issues & reasons:
        hints = tuple(
            h
            for h in candidate.slots
            if reason in h.issues
            or (reason == "n0_numeric_construction_unresolved" and h.type == "uint")
            or (
                reason == "n0_vocabulary_construction_unresolved"
                and h.target
                and h.target.get("kind") == "vocabulary"
            )
        )
        if hints and all(
            h.source_segments
            and all(
                any(a.start <= b.start < b.end <= a.end for a in covered)
                for b in h.source_segments
            )
            for h in hints
        ):
            issues.discard(reason)
    return tuple(sorted(issues))


def _rename(found: Recognized, leaves: list[Leaf], before: Recognized) -> Recognized:
    names = {item.slot.name: f"leaf_{i}" for i, item in enumerate(leaves)}
    values = {
        name: value.model_copy(update={"leaves": tuple(names[n] for n in value.leaves)})
        if isinstance(value, (Expression, ContextExpression))
        else value
        for name, value in found.values.items()
    }
    schema = LeafSchema(
        format=2,
        slots=tuple(
            slot.model_copy(update={"expression": values[slot.name]})
            if isinstance(values[slot.name], ContextExpression)
            else slot
            for slot in found.schema.slots
        ),
    )
    originals = {
        f"leaf_{i}": tuple(o for o in before.occurrences if o.slot == item.slot.name)
        for i, item in enumerate(leaves)
        if item.row == "retained"
    }
    occurrences = tuple(
        o.model_copy(update={"raw_spans": originals[o.slot][o.ordinal].raw_spans})
        if o.slot in originals
        else o
        for o in found.occurrences
    )
    order = {s.name: i for i, s in enumerate(schema.slots)}
    occurrences = tuple(
        sorted(
            occurrences,
            key=lambda o: (
                o.raw_spans[0].start if o.raw_spans else order[o.slot],
                o.source_presence != "explicit",
                o.slot,
            ),
        )
    )
    return replace(found, values=values, schema=schema, occurrences=occurrences)


def _prepare(
    raw: str,
    prepared: PreparedField,
    classifier: Classifier,
    context: CardContext | None,
) -> PreparedField:
    del context
    derived = prepared.derived
    if derived.field.source.field not in {"effect", "section"} or not isinstance(
        derived.field.source.owner, (FaceRevisionOwner, PrintingFaceOwner)
    ):
        return prepared
    parts, recognized, rows, interfaces = (
        list(derived.field.parts),
        list(derived.recognized),
        list(derived.registry_rows),
        list(prepared.interfaces),
    )
    for i, original in enumerate(derived.original.parts):
        if original.source_span.role != "body":
            continue
        builder = ReadSources(
            raw, derived.field.source, original, parts[i], recognized[i], classifier
        )
        builder.collect()
        if not builder.leaves:
            continue
        leaves = _retained(recognized[i], builder.leaves)
        updated, result, registry = transform(
            raw,
            parts[i],
            leaves,
            (),
            replace(recognized[i], issues=_issues(builder, leaves), semantics=None),
            separate_occurrences=True,
        )
        result = _rename(result, leaves, recognized[i])
        names = {item.slot.name: f"leaf_{j}" for j, item in enumerate(leaves)}
        ports = tuple(
            p.model_copy(
                update={
                    "evidence": p.evidence.model_copy(
                        update={
                            "source_leaves": tuple(
                                names[n] for n in p.evidence.source_leaves
                            )
                        }
                    )
                }
            )
            if p.evidence
            else p
            for p in builder.ports
        )
        previous = interfaces[i]
        scopes = {s.id: s for s in previous.scopes} if previous else {}
        for region in regions(original.canonical_source):
            scopes[region.id] = Scope(
                id=region.id,
                parent=region.parent,
                kind="ability" if region.kind == "ability" else "branch",
            )
        imports: tuple[Port, ...] = (*(previous.imports if previous else ()), *ports)
        interfaces[i] = Projection(
            projection_kind="pending",
            discriminator=None,
            scopes=tuple(sorted(scopes.values(), key=lambda s: s.id)),
            imports=tuple(sorted(imports, key=lambda p: p.name)),
            exports=previous.exports if previous else (),
        )
        parts[i], recognized[i], rows[i] = updated, result, registry
    return replace(
        prepared,
        derived=replace(
            derived,
            field=SourceField(derived.field.source, tuple(parts)),
            recognized=tuple(recognized),
            registry_rows=tuple(rows),
        ),
        interfaces=tuple(interfaces),
    )


FAMILY = PreparationFamily("A1_A4", DESCRIPTORS, _prepare)
