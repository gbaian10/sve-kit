"""Import and independently validate correction records, evidence and applications."""

from typing import TYPE_CHECKING

from sve_carddb.build import Json
from sve_carddb.products.models import LocalizedText
from sve_carddb.source_corrections.plan import verify_applications

if TYPE_CHECKING:
    from sve_carddb.build import Database, Value
    from sve_carddb.source_corrections.plan import Application
    from sve_carddb.text_observations.intern import TextInterner
    from sve_carddb.text_observations.plan import TextPlan
    from sve_carddb.text_observations.vocabulary import Vocabulary


def _unit(application: Application, value: str) -> LocalizedText:
    return LocalizedText(
        lang="ja" if application.observation.region == "jp" else "en", text=value
    )


def _text_values(application: Application, value: str) -> dict[str, Value]:
    from sve_carddb.text_observations.intern import text_values  # ruff: ignore[import-outside-top-level] -- retain the shared text boundary without initializing the text package during import

    return text_values(_unit(application, value))


def correction_values(application: Application) -> dict[str, Value]:
    """Preserve authored exact values while recording this build's upstream/conflict state."""
    data = application.data
    return {
        "id": data.id,
        "printing_id": data.printing_id,
        "face_id": data.face_id,
        "field": data.field,
        "expected_source_unit_id": _text_values(application, data.expected_raw_value)[
            "id"
        ]
        if data.field == "effect"
        else None,
        "expected_raw_value": Json(data.expected_raw_value),
        "corrected_value": Json(data.corrected_value),
        "expected_source_hash": data.expected_source_hash,
        "reason": data.reason,
        "reported_to_official": data.reported_to_official,
        "reported_on": data.reported_on,
        "report_url": data.report_url,
        "state": "upstream_fixed"
        if application.status == "already_fixed"
        else "needs_review"
        if application.status == "conflict"
        else data.state,
    }


def evidence_values(application: Application) -> tuple[dict[str, Value], ...]:
    """Bind each locator to the independently verified regional image version."""
    return tuple(
        {
            "correction_id": application.data.id,
            "source_id": source.id,
            "kind": evidence.kind,
            "locator": evidence.locator,
            "quote": None,
        }
        for evidence, source in zip(
            application.data.evidence, application.images, strict=True
        )
    )


def application_values(application: Application, plan: TextPlan) -> dict[str, Value]:
    """A successful application always links its actual post-correction candidate."""
    from sve_carddb.text_observations.importer import revision_id  # ruff: ignore[import-outside-top-level] -- the text importer invokes this layer after creating revisions

    data = application.data
    success = application.status in {"applied", "already_fixed"}
    candidate = next(
        item
        for item in plan.candidates()
        if (item.printing_id, item.face_id, item.card.source.id)
        == (data.printing_id, data.face_id, application.observation.card.source.id)
    )
    return {
        "correction_id": data.id,
        "source_id": application.observation.card.source.id,
        "result_unit_id": _text_values(application, data.corrected_value)["id"]
        if success and data.field == "effect"
        else None,
        "face_revision_id": revision_id(candidate) if success else None,
        "status": application.status,
    }


def populate_corrections(db: Database, plan: TextPlan, texts: TextInterner) -> None:
    """Populate inside the observation transaction; proposed records have no application."""
    if plan.corrections is None:
        return
    verify_applications(plan.identity, plan.observations, plan.corrections)
    for application in plan.corrections:
        if application.data.field == "effect":
            texts.intern(_unit(application, application.data.expected_raw_value))
            if application.status in {"applied", "already_fixed"}:
                texts.intern(_unit(application, application.data.corrected_value))
        db.insert("source_correction", correction_values(application))
        for values in evidence_values(application):
            db.insert("correction_evidence", values)
        if application.status is not None:
            db.insert("correction_application", application_values(application, plan))


def verify_corrections(db: Database, plan: TextPlan, vocabulary: Vocabulary) -> None:
    """Check domain values and raw/result separation beyond SQLite FK/adoption checks."""
    from sve_carddb.text_observations.importer import revision_id  # ruff: ignore[import-outside-top-level] -- avoids the importer dependency cycle

    if plan.corrections is None:
        raise ValueError("Correction verification requires an explicitly pinned plan")
    verify_applications(plan.identity, plan.observations, plan.corrections)
    expected = {
        "source_correction": tuple(correction_values(a) for a in plan.corrections),
        "correction_evidence": tuple(
            v for a in plan.corrections for v in evidence_values(a)
        ),
        "correction_application": tuple(
            application_values(a, plan)
            for a in plan.corrections
            if a.status is not None
        ),
    }
    for table, values in expected.items():
        if sorted((dict(row.values) for row in db.rows(table)), key=str) != sorted(
            values, key=str
        ):
            raise ValueError(
                "Correction database inventory or values mismatch: " + table
            )
    revisions = {row.values["id"]: row.values for row in db.rows("face_revision")}
    observations = {
        (
            row.values["printing_id"],
            row.values["face_id"],
            row.values["source_id"],
        ): row.values
        for row in db.rows("printing_face_observation")
    }
    units = {row.values["id"]: row.values for row in db.rows("text_unit")}
    for application in plan.corrections:
        if application.status not in {"applied", "already_fixed"}:
            continue
        item = application.observation
        physical = observations.get(
            (item.printing_id, item.face_id, item.card.source.id)
        )
        if physical is None or physical["revision_id"] != revision_id(item):
            raise ValueError("Correction lost the original physical observation")
        candidate = next(
            c
            for c in plan.candidates()
            if (c.printing_id, c.face_id) == (item.printing_id, item.face_id)
        )
        revision = revisions.get(revision_id(candidate))
        effect = _text_values(application, candidate.content.effect or "")
        if (
            revision is None
            or any(
                revision[key] != value
                for key, value in {
                    "face_id": item.face_id,
                    "region": item.region,
                    "effect_unit_id": effect["id"],
                    "type_code": vocabulary.lookup(
                        item.region, "type", candidate.content.type_raw
                    ).code,
                }.items()
            )
            or units.get(effect["id"]) != effect
        ):
            raise ValueError(
                "Correction result revision does not contain the corrected content"
            )
