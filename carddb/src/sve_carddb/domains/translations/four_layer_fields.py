"""Persist one complete typed field without borrowing another owner's source binding."""

from typing import TYPE_CHECKING

from sve_carddb.build import Json
from sve_carddb.build.rows import insert_exact
from sve_carddb.contracts.annotations import Annotation, AnnotationSet
from sve_carddb.contracts.four_layer import (
    CardNameReference,
    FaceRevisionOwner,
    GlossaryReference,
    PrintingFaceOwner,
    VocabularyReference,
    hash_payload,
)
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.translations.four_layer_identities import context_variant
from sve_carddb.domains.translations.four_layer_render import BoundTarget, Result
from sve_carddb.domains.translations.four_layer_storage import (
    read_annotation,
    read_owned_render_occurrences,
    write_annotation,
    write_binding,
)

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.contracts.source_binding import SourceBinding, SourceDescriptor
    from sve_carddb.domains.translations.four_layer_pipeline import CompiledField
    from sve_carddb.domains.translations.four_layer_render import (
        Renderer,
        SelectedTarget,
    )


def render_field(  # ruff: ignore[too-many-arguments] -- explicit suppression preserves validated source bindings without selecting translated text
    db: Database,
    compiled: CompiledField,
    renderer: Renderer,
    targets: Mapping[tuple[str, str], SelectedTarget],
    lang: str,
    *,
    assignment: str = "default",
    suppress: bool = False,
) -> Result:
    """Persist only complete bindings; absent selected values preserve a whole-field fallback."""
    if not compiled.complete:
        raise ValueError(
            "An incomplete source field cannot activate its partial bindings"
        )
    matches = tuple(match for match in compiled.matches if match is not None)
    bindings = tuple(match.binding for match in matches)
    compiled.field.verify(
        compiled.source.text,
        tuple(match.frame for match in matches),
        bindings,
        renderer.domains,
    )
    variant = context_variant(bindings, assignment)
    context_id = "ctx:" + hash_payload(
        {
            "recipe": "context-v1",
            "source_unit_id": compiled.source.descriptor.source_unit_id,
            "semantic_variant": variant,
        }
    )
    plan = tuple(
        BoundTarget(
            match.frame,
            match.binding,
            targets.get((match.frame.id, lang)),
            low,
        )
        for match, low in zip(matches, compiled.low_confidence, strict=True)
    )
    result = (
        Result(None, ("suppressed_translation",))
        if suppress
        else renderer.render(context_id, lang, plan)
    )
    insert_exact(
        db,
        "translation_context",
        {
            "id": context_id,
            "source_unit_id": compiled.source.descriptor.source_unit_id,
            "semantic_variant": variant,
        },
        ("id",),
    )
    use_id = write_use(db, compiled.source.descriptor, context_id)
    for binding in bindings:
        write_binding(db, use_id, binding, renderer.domains)
    original = _source_annotation(
        db, bindings, compiled.source.descriptor.source_unit_id
    )
    if original.occurrences:
        _annotation(db, original)
        insert_exact(
            db,
            "translation_use_annotation",
            {"use_id": use_id, "annotation_set_id": original.id},
            ("use_id",),
        )
    if result.rendered is None:
        return result
    rendered = result.rendered
    identifier, revision = rendered.identity()
    insert_exact(
        db,
        "translation",
        {
            "id": identifier,
            "context_id": context_id,
            "target_lang": lang,
            "revision": revision,
            "text": rendered.text,
            "tokens": None,
            "origin": rendered.origin,
            "authority": "unofficial",
            "low_confidence": rendered.low_confidence,
            "source_hash": "sha256:" + compiled.source.descriptor.source_hash,
            "source_id": None,
        },
        ("id",),
    )
    for item in plan:
        assert item.selected is not None
        insert_exact(
            db,
            "translation_binding",
            {
                "translation_id": identifier,
                "binding_id": item.binding.id,
                "template_id": item.frame.id,
                "lang": lang,
                "variant_key": item.selected.variant_key,
            },
            ("translation_id", "binding_id"),
        )
    for occurrence in rendered.occurrences():
        data = occurrence.model_dump(mode="json")
        insert_exact(
            db,
            "render_leaf_occurrence",
            {
                "translation_id": identifier,
                "binding_id": occurrence.binding_id,
                "node_path": Json(data["node_path"]),
                "slot": occurrence.slot,
                "source_ordinals": Json(data["source_ordinals"]),
                "ranges": Json(data["ranges"]),
            },
            ("translation_id", "binding_id", "node_path"),
        )
    if rendered.annotation.occurrences:
        insert_exact(
            db,
            "text_unit",
            {
                "id": rendered.annotation.text_unit_id,
                "lang": lang,
                "text": rendered.text,
                "content_hash": digest(rendered.text.encode()),
            },
            ("id",),
        )
        _annotation(db, rendered.annotation)
        insert_exact(
            db,
            "translation_annotation",
            {"translation_id": identifier, "annotation_set_id": rendered.annotation.id},
            ("translation_id",),
        )
    for term_id in sorted(
        {
            reference.term_id
            if isinstance(reference, CardNameReference)
            else reference.key
            for annotation in rendered.annotation.occurrences
            if isinstance(
                reference := annotation.reference,
                (CardNameReference, GlossaryReference),
            )
        }
    ):
        insert_exact(
            db,
            "translation_term",
            {"translation_id": identifier, "term_id": term_id},
            ("translation_id", "term_id"),
        )
    insert_exact(
        db,
        "translation_selection",
        {"context_id": context_id, "target_lang": lang, "translation_id": identifier},
        ("context_id", "target_lang"),
    )
    read_owned_render_occurrences(db, identifier, renderer.domains, rendered)
    return result


def write_use(db: Database, descriptor: SourceDescriptor, context_id: str) -> str:
    """Keep exact owner fields distinct even when their source text is interned together."""
    owner = descriptor.owner
    if not isinstance(owner, (FaceRevisionOwner, PrintingFaceOwner)):
        raise TypeError("Compiled card field requires a card owner")
    identifier = "use:" + hash_payload(
        {
            "recipe": "use-v1",
            "owner": owner.model_dump(mode="json"),
            "field": descriptor.field,
            "ordinal": descriptor.ordinal,
            "context_id": context_id,
        }
    )
    values: dict[str, Value] = dict.fromkeys(db.columns("translation_use"))
    values.update(
        id=identifier,
        context_id=context_id,
        field=descriptor.field,
        ordinal=descriptor.ordinal,
    )
    if isinstance(owner, FaceRevisionOwner):
        values["face_revision_id"] = owner.revision_id
    else:
        values.update(printing_id=owner.printing_id, face_id=owner.face_id)
    insert_exact(db, "translation_use", values, ("id",))
    return identifier


def _source_annotation(
    db: Database, bindings: tuple[SourceBinding, ...], unit_id: str
) -> AnnotationSet:
    referenced = {
        value.key
        for binding in bindings
        for value in binding.values.values()
        if isinstance(value, GlossaryReference)
    }
    terms = {
        row.values["id"]: row.values
        for term_id in sorted(referenced)
        for row in db.select(
            "glossary_term", db.columns("glossary_term"), where={"id": term_id}
        )
    }
    occurrences: list[Annotation] = []
    for binding in bindings:
        for leaf in binding.occurrences:
            reference = binding.values[leaf.slot]
            if not isinstance(
                reference, (CardNameReference, GlossaryReference, VocabularyReference)
            ):
                continue
            bold = (
                terms[reference.key]["emphasis"]
                if isinstance(reference, GlossaryReference)
                and terms[reference.key]["category"] == "rule_term"
                else True
                if isinstance(reference, (CardNameReference, GlossaryReference))
                or reference.key[0] in {"class", "type"}
                else None
            )
            if bold is not None and type(bold) is not bool:
                raise TypeError("Source concept emphasis must be a nullable boolean")
            occurrences.append(
                Annotation(
                    ordinal=len(occurrences),
                    reference=reference,
                    ranges=leaf.raw_spans,
                    bold=bold,
                )
            )
    ordered = tuple(
        value.model_copy(update={"ordinal": ordinal})
        for ordinal, value in enumerate(
            sorted(
                occurrences,
                key=lambda value: (
                    value.ranges[0].start,
                    value.ranges[-1].end,
                    canonical(value.reference.model_dump(mode="json")),
                ),
            )
        )
    )
    return AnnotationSet(
        id="ann:"
        + hash_payload(
            {
                "recipe": "annotation-v1",
                "text_unit_id": unit_id,
                "occurrences": [a.model_dump(mode="json") for a in ordered],
            }
        ),
        text_unit_id=unit_id,
        occurrences=ordered,
    )


def _annotation(db: Database, value: AnnotationSet) -> None:
    if db.select("annotation_set", ("id",), where={"id": value.id}):
        if read_annotation(db, value.id) != value:
            raise ValueError("Annotation content-address collision")
    else:
        write_annotation(db, value)
