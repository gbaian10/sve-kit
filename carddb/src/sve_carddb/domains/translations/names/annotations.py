"""A resolved name concept covers the exact whole name in either language."""

from typing import TYPE_CHECKING

from sve_carddb.build.rows import insert_exact
from sve_carddb.contracts.annotations import Annotation, AnnotationSet
from sve_carddb.contracts.four_layer import CardNameReference, Span, hash_payload
from sve_carddb.domains.translations.four_layer_storage import (
    read_annotation,
    write_annotation,
)

if TYPE_CHECKING:
    from sve_carddb.build import Database


def annotate_name(
    db: Database, owner_id: str, unit_id: str, term_id: str, *, translated: bool = False
) -> None:
    """The caller supplies its verified concept; internal words never become separate references."""
    rows = db.select("text_unit", db.columns("text_unit"), where={"id": unit_id})
    if len(rows) != 1:
        raise ValueError("Name annotation requires its exact text unit")
    text = rows[0].values["text"]
    if not isinstance(text, str):
        raise TypeError("Name annotation requires a text string")
    if not text:
        raise ValueError("Name annotation cannot cover an empty name")
    occurrence = Annotation(
        ordinal=0,
        reference=CardNameReference(kind="card_name", term_id=term_id),
        ranges=(Span(start=0, end=len(text)),),
        bold=True,
    )
    identifier = "ann:" + hash_payload(
        {
            "recipe": "annotation-v1",
            "text_unit_id": unit_id,
            "occurrences": [occurrence.model_dump(mode="json")],
        }
    )
    annotation = AnnotationSet(
        id=identifier, text_unit_id=unit_id, occurrences=(occurrence,)
    )
    existing = db.select("annotation_set", ("id",), where={"id": identifier})
    if existing:
        if read_annotation(db, identifier) != annotation:
            raise ValueError("Name annotation identity collision")
    else:
        write_annotation(db, annotation)
    insert_exact(
        db,
        "translation_annotation" if translated else "translation_use_annotation",
        {
            "translation_id" if translated else "use_id": owner_id,
            "annotation_set_id": identifier,
        },
        ("translation_id" if translated else "use_id",),
    )
