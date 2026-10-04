"""Map name owners to independently verified publication and frozen printing evidence."""

from typing import TYPE_CHECKING

from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.digital_name_policies.evaluate import NameOwner as PolicyOwner
from sve_carddb.registry.records import PrintingData
from sve_carddb.text_observations.models import candidate_revision_id
from sve_carddb.text_observations.plan import verify_plan
from sve_carddb.translations.name_sources import NameOwner, name_source

if TYPE_CHECKING:
    from sve_carddb.build_db import Database
    from sve_carddb.text_observations.models import FaceObservation
    from sve_carddb.text_observations.plan import TextPlan


def publication_owners(
    db: Database, texts: TextPlan
) -> tuple[tuple[NameOwner, PolicyOwner], ...]:
    """Raw registry identity alone cannot authorize a corrected or withheld owner."""
    verify_plan(texts)
    published = {
        r.data.id
        for r in texts.publication_identity().included("printing")
        if isinstance(r.data, PrintingData)
    }
    candidates: dict[str, list[FaceObservation]] = {}
    # Projection retains raw revisions alongside corrected current candidates.
    for item in (*texts.materialized(), *texts.candidates()):
        if (
            item.region == "jp"
            and item.printing_id in published
            and item.content.effect is not None
        ):
            candidates.setdefault(candidate_revision_id(item), []).append(item)
    result = []
    for row in db.rows("face_revision"):
        if row.values["region"] != "jp":
            continue
        owner = NameOwner("face_revision", str(row.values["id"]))
        possible = candidates.get(owner.identifier, [])
        result.append((owner, _owner(db, owner, possible)))
    printings = {r.values["id"]: r.values for r in db.rows("printing")}
    for row in db.rows("printing_face"):
        printing = printings[row.values["printing_id"]]
        if printing["region"] != "jp":
            continue
        owner = NameOwner(
            "printing_face", str(printing["id"]), str(row.values["face_id"])
        )
        possible = [
            item
            for item in texts.observations
            if item.printing_id == owner.identifier
            and item.printing_id in published
            and item.face_id == owner.face_id
        ]
        result.append((owner, _owner(db, owner, possible)))
    return tuple(
        sorted(
            result,
            key=lambda pair: (pair[0].kind, pair[0].identifier, pair[0].face_id or ""),
        )
    )


def _owner(
    db: Database, owner: NameOwner, possible: list[FaceObservation]
) -> PolicyOwner:
    source = name_source(db, owner)
    if not possible:
        raise ValueError("Name owner is absent from verified publication candidates")
    first = min(possible, key=lambda item: (item.printing_id, item.card.source.id))
    actual_face = (
        owner.face_id
        if owner.kind == "printing_face"
        else str(
            db.select("face_revision", ("face_id",), where={"id": owner.identifier})[
                0
            ].values["face_id"]
        )
    )
    actual_card = str(
        db.select("face", ("card_id",), where={"id": actual_face})[0].values["card_id"]
    )
    if (actual_card, actual_face) != (first.card_id, first.face_id):
        raise ValueError("Name owner differs from its verified publication source")
    if source is None and owner.kind == "face_revision":
        raise ValueError("Name publication owner is not confirmed")
    if source is None:
        return PolicyOwner(
            kind=owner.kind,
            owner_id=owner.identifier,
            card_id=first.card_id,
            face_id=first.face_id,
            printing_id=first.printing_id,
            state="unknown",
            name_ref=None,
        )
    matches = [
        item
        for item in possible
        if item.card_id == source.card_id
        and item.face_id == source.face_id
        and item.content.name == source.text
        and item.card.faces[item.source_index].name == source.text
    ]
    if not matches:
        raise ValueError("Name owner differs from its verified publication source")
    item = min(matches, key=lambda item: (item.printing_id, item.card.source.id))
    archive = item.card.source.archive
    return PolicyOwner(
        kind=owner.kind,
        owner_id=owner.identifier,
        card_id=source.card_id,
        face_id=source.face_id,
        printing_id=item.printing_id,
        state="known",
        name_ref=SourceRef(
            batch_id=archive.batch_id,
            source_version_id=item.card.source.id,
            parser="translation-jp-v1",
            locator=f"/faces/{item.source_index}/name",
            text_hash=source.source_hash,
        ),
    )
