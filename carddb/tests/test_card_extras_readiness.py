"""Reviewed supplemental checks expire with any source or stored-text graph change."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_minimum
from sve_carddb.card_extras import (
    ErrataPage,
    ErrataPrinting,
    plan_card_extras,
    populate_card_extras,
    require_card_extras_ready,
)
from sve_carddb.card_extras.readiness import ErrataConfirmation, review_context
from sve_carddb.snapshot.values import canonical, digest

from .build_db_fixtures import rows
from .card_extras_fixtures import context, page, seed, source

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build_db import CompiledSchema, Database


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_minimum(include_en=True)


@pytest.fixture
def reviewed(schema: CompiledSchema) -> Iterator[tuple[Database, ErrataConfirmation]]:
    with create_database(schema) as db:
        seed(db)
        values = rows()
        url = "https://shadowverse-evolve.com/errata/synthetic/"
        notice = ErrataPage(
            source=source().model_copy(
                update={"id": "src:v1:" + digest(url.encode())[7:], "url": url}
            ),
            region="jp",
            official_url=url,
            printings=(ErrataPrinting(card_no="TEST-001Ⓢa"),),
        )
        with db.transaction():
            db.insert("face_revision", values["face_revision"])
            db.insert("face_current", values["face_current"])
            db.insert(
                "printing_face_observation",
                {
                    "printing_id": "printing",
                    "face_id": "face",
                    "source_id": "source",
                    "revision_id": "revision",
                    "observed_at": "2026-10-01T00:00:00Z",
                },
            )
            plan = plan_card_extras(
                db,
                (page().model_copy(update={"errata_urls": (url,)}),),
                errata=(notice,),
            )
            populate_card_extras(db, plan, build=context(plan))
        restriction = require_card_extras_ready(db, (("jp", "card"),))[0]
        fingerprint = review_context(db, restriction)
        assert fingerprint is not None
        check = ErrataConfirmation(
            issue_id=restriction.issue_id,
            decision_id="decision",
            context_hash=fingerprint,
        )
        with db.transaction():
            for row in db.rows("source_record"):
                db.insert(
                    "decision_source",
                    {
                        "decision_id": "decision",
                        "source_id": row.values["id"],
                        "role": "errata_current_evidence",
                    },
                )
            db.insert(
                "decision_source",
                {
                    "decision_id": "decision",
                    "source_id": notice.source.id,
                    "role": "errata_current_checked",
                    "locator": canonical(
                        {"issue_id": check.issue_id, "context_hash": fingerprint}
                    ).decode(),
                },
            )
        yield db, check


def test_only_the_exact_reviewed_issue_can_clear(
    reviewed: tuple[Database, ErrataConfirmation],
) -> None:
    db, check = reviewed
    original = require_card_extras_ready(db, (("jp", "card"),))
    assert len(original) == 2
    remaining = require_card_extras_ready(db, (("jp", "card"),), confirmations=(check,))
    assert len(remaining) == 1
    assert remaining[0].issue_id != check.issue_id
    with pytest.raises(
        ValueError,
        match=r"^Unresolved supplemental evidence blocks automation or confirmed current$",
    ):
        require_card_extras_ready(
            db, (("jp", "card"),), confirmations=(check,), strict=True
        )


def test_invalid_issue_target_is_rejected_at_the_boundary(
    reviewed: tuple[Database, ErrataConfirmation],
) -> None:
    db, check = reviewed
    with db.transaction():
        db.update(
            "build_issue",
            {"id": check.issue_id},
            {
                "message": canonical(
                    {"region": "jp", "card_id": "card", "target": []}
                ).decode()
            },
        )
    with pytest.raises(TypeError, match=r"^Invalid card extras pending issue target$"):
        require_card_extras_ready(db, (("jp", "card"),))


@pytest.mark.parametrize(
    "fault",
    [
        "current_missing",
        "revision_changed",
        "stat_changed",
        "effect_changed",
        "section_added",
        "printing_source_changed",
        "observed_source_changed",
        "observed_missing",
        "source_hash_changed",
        "announcement_changed",
        "decision_unconfirmed",
        "receipt_missing",
        "evidence_missing",
    ],
)
def test_confirmation_expires_when_reviewed_graph_changes(  # ruff: ignore[complex-structure] -- independent graph mutations share the same small baseline
    reviewed: tuple[Database, ErrataConfirmation], fault: str
) -> None:
    db, check = reviewed
    with db.transaction():
        if fault == "current_missing":
            db.delete("face_current", {"face_id": "face", "region": "jp"})
        elif fault == "revision_changed":
            db.insert(
                "face_revision",
                dict(db.rows("face_revision")[0].values)
                | {"id": "replacement", "revision": 9},
            )
            db.update(
                "face_current",
                {"face_id": "face", "region": "jp"},
                {"revision_id": "replacement"},
            )
        elif fault == "stat_changed":
            db.update("face_revision", {"id": "revision"}, {"cost": 9})
        elif fault == "effect_changed":
            db.insert(
                "text_unit",
                dict(db.rows("text_unit")[0].values)
                | {
                    "id": "changed",
                    "text": "Synthetic changed effect",
                    "content_hash": digest(b"Synthetic changed effect"),
                },
            )
            db.update(
                "face_revision", {"id": "revision"}, {"effect_unit_id": "changed"}
            )
        elif fault == "section_added":
            db.insert(
                "face_text_section",
                {
                    "revision_id": "revision",
                    "ordinal": 0,
                    "text_unit_id": "text",
                    "kind": "unknown",
                },
            )
        elif fault == "printing_source_changed":
            db.update("printing", {"id": "printing"}, {"source_id": source().id})
        elif fault == "observed_source_changed":
            row = dict(db.rows("printing_face_observation")[0].values)
            db.delete(
                "printing_face_observation",
                {"printing_id": "printing", "face_id": "face", "source_id": "source"},
            )
            db.insert("printing_face_observation", row | {"source_id": source().id})
        elif fault == "observed_missing":
            db.delete(
                "printing_face_observation",
                {"printing_id": "printing", "face_id": "face", "source_id": "source"},
            )
        elif fault == "source_hash_changed":
            db.update(
                "source_record", {"id": "source"}, {"sha256": "sha256:" + "b" * 64}
            )
        elif fault == "announcement_changed":
            db.update(
                "errata_version",
                {"id": db.rows("errata_version")[0].values["id"]},
                {"date_raw": "Synthetic changed date"},
            )
        elif fault == "decision_unconfirmed":
            db.update("decision", {"id": "decision"}, {"state": "proposed"})
        else:
            role = (
                "errata_current_checked"
                if fault == "receipt_missing"
                else "errata_current_evidence"
            )
            evidence_row = next(
                row.values
                for row in db.rows("decision_source")
                if row.values["role"] == role
            )
            db.delete(
                "decision_source",
                {
                    "decision_id": "decision",
                    "source_id": evidence_row["source_id"],
                    "role": role,
                },
            )
    remaining = require_card_extras_ready(db, (("jp", "card"),), confirmations=(check,))
    assert len(remaining) == 2


def test_no_announcement_or_duplicate_check_can_clear(
    reviewed: tuple[Database, ErrataConfirmation],
) -> None:
    db, check = reviewed
    with pytest.raises(ValueError, match=r"^Duplicate errata confirmation issue$"):
        require_card_extras_ready(db, (("jp", "card"),), confirmations=(check, check))
    with db.transaction():
        for row in db.rows("errata_printing"):
            db.delete(
                "errata_printing",
                {
                    "errata_version_id": row.values["errata_version_id"],
                    "printing_id": row.values["printing_id"],
                },
            )
        db.delete("errata_version", {"id": db.rows("errata_version")[0].values["id"]})
    assert (
        len(require_card_extras_ready(db, (("jp", "card"),), confirmations=(check,)))
        == 2
    )


def test_mutation_of_text_under_same_id_invalidates_review(
    reviewed: tuple[Database, ErrataConfirmation],
) -> None:
    db, check = reviewed
    with db.transaction():
        db.update(
            "text_unit", {"id": "text"}, {"text": "Synthetic reused identity mutation"}
        )
    assert (
        len(require_card_extras_ready(db, (("jp", "card"),), confirmations=(check,)))
        == 2
    )


@pytest.mark.parametrize("added", ["face", "printing"])
def test_added_unreviewed_physical_scope_invalidates_confirmation(
    reviewed: tuple[Database, ErrataConfirmation], added: str
) -> None:
    db, check = reviewed
    with db.transaction():
        if added == "face":
            db.update("card", {"id": "card"}, {"layout": "double_faced"})
            db.insert(
                "face",
                dict(db.rows("face")[0].values)
                | {"id": "new-face", "ordinal": 1, "side": "back"},
            )
            db.insert(
                "printing_face",
                dict(db.rows("printing_face")[0].values) | {"face_id": "new-face"},
            )
        else:
            db.insert(
                "printing",
                dict(db.rows("printing")[0].values)
                | {"id": "new-printing", "card_no": "TEST-NEW"},
            )
            db.insert(
                "printing_face",
                dict(db.rows("printing_face")[0].values)
                | {"printing_id": "new-printing"},
            )
    assert (
        len(require_card_extras_ready(db, (("jp", "card"),), confirmations=(check,)))
        == 2
    )


def test_unrelated_announcement_cannot_clear_a_card_check(
    reviewed: tuple[Database, ErrataConfirmation],
) -> None:
    db, check = reviewed
    with db.transaction():
        db.update(
            "errata_printing",
            {
                "errata_version_id": db.rows("errata_version")[0].values["id"],
                "printing_id": "printing",
            },
            {"printing_id": "printing1"},
        )
    assert (
        len(require_card_extras_ready(db, (("jp", "card"),), confirmations=(check,)))
        == 2
    )
