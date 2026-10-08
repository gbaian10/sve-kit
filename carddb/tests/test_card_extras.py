"""Independent synthetic counterexamples for first-release supplemental semantics."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import input_record
from sve_carddb.card_extras import (
    ErrataChange,
    ErrataPage,
    ErrataPrinting,
    QAEntry,
    RelatedLink,
    plan_card_extras,
    populate_card_extras,
    require_card_extras_ready,
)
from sve_carddb.snapshot.values import canonical

from .card_extras_fixtures import context, page, seed, source

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build_db import CompiledSchema, Database
    from sve_carddb.build_inputs import BuildContext, InputRecord, SourceUse


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_build(("qa", "errata", "related"))


@pytest.fixture
def db(schema: CompiledSchema) -> Iterator[Database]:
    with create_database(schema) as database:
        seed(database)
        yield database


class TestNumberedQA:
    @pytest.fixture(scope="class")
    def populated(self, schema: CompiledSchema) -> Iterator[Database]:
        with create_database(schema) as database:
            seed(database)
            second = page(
                "TEST-002",
                question=page()
                .qa[0]
                .model_copy(update={"stable_source_key": "different-page-local-key"}),
            )
            plan = plan_card_extras(database, (page(), second))
            with database.transaction():
                populate_card_extras(database, plan, build=context(plan))
            yield database

    def test_cross_card_dedup(self, populated: Database) -> None:
        assert len(populated.rows("qa")) == 1
        assert len(populated.rows("qa_version")) == 1
        assert {r.values["card_id"] for r in populated.rows("qa_card")} == {
            "card",
            "card1",
        }

    def test_both_raw_sources_survive(self, populated: Database) -> None:
        assert {r.values["id"] for r in populated.rows("source_record")} >= {
            source().id,
            source("TEST-002").id,
        }

    def test_no_source_windows(self, populated: Database) -> None:
        assert not populated.rows("source_coverage")


def test_same_day_change_has_distinct_versions(db: Database) -> None:
    original = page()
    changed = page(
        hour=1,
        question=original.qa[0].model_copy(
            update={"answer": "Different synthetic answer."}
        ),
    )
    plan = plan_card_extras(db, (changed, original))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    versions = sorted(
        (r.values for r in db.rows("qa_version")), key=lambda r: str(r["revision"])
    )
    assert len(versions) == 2
    assert [r["revision"] for r in versions] == [0, 1]
    assert versions[0]["published_on"] == versions[1]["published_on"]
    assert versions[1]["supersedes_id"] == versions[0]["id"]
    assert versions[0]["answer_unit_id"] != versions[1]["answer_unit_id"]


def test_unnumbered_key_is_not_an_official_number(db: Database) -> None:
    entry = QAEntry(
        stable_source_key="synthetic-page#anchor",
        locator="anchor",
        question="Unnumbered synthetic?",
        answer="Unnumbered answer.",
    )
    plan = plan_card_extras(db, (page(question=entry),))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    assert db.rows("qa")[0].values["official_number"] is None
    assert db.rows("qa")[0].values["stable_source_key"] == entry.stable_source_key


def test_distinct_unnumbered_blocks_keep_both_questions(db: Database) -> None:
    first = QAEntry(
        stable_source_key="synthetic-page#first",
        locator="first",
        question="First synthetic question?",
        answer="First synthetic answer.",
    )
    second = first.model_copy(
        update={
            "stable_source_key": "synthetic-page#second",
            "locator": "second",
            "question": "Second synthetic question?",
            "answer": "Second synthetic answer.",
        }
    )
    current = page().model_copy(update={"qa": (first, second)})
    plan = plan_card_extras(db, (current,))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    assert {row.values["stable_source_key"] for row in db.rows("qa")} == {
        first.stable_source_key,
        second.stable_source_key,
    }
    assert all(row.values["official_number"] is None for row in db.rows("qa"))
    versions = db.rows("qa_version")
    assert len(versions) == 2
    assert len({row.values["question_unit_id"] for row in versions}) == 2
    assert len({row.values["answer_unit_id"] for row in versions}) == 2
    assert len(db.rows("qa_card")) == 2


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("published_on", "2026-10-02"),
        ("updated_on", "2026-10-02"),
    ],
)
def test_parsed_date_changes_create_qa_version(
    db: Database, field: str, value: str
) -> None:
    first = page()
    later = page(
        "TEST-002",
        hour=1,
        question=first.qa[0].model_copy(update={field: value}),
    )
    plan = plan_card_extras(db, (later, first))
    with db.transaction():
        record = populate_card_extras(db, plan, build=context(plan))
    versions = sorted(
        db.rows("qa_version"), key=lambda row: str(row.values["revision"])
    )
    assert len(versions) == 2
    assert versions[0].values[field] == getattr(first.qa[0], field)
    assert versions[1].values[field] == value
    assert versions[1].values["supersedes_id"] == versions[0].values["id"]
    assert (
        versions[1].values["question_unit_id"] == versions[0].values["question_unit_id"]
    )
    assert versions[1].values["answer_unit_id"] == versions[0].values["answer_unit_id"]
    assert {row.values["card_id"] for row in db.rows("qa_card")} == {"card", "card1"}
    assert {use.source.id for use in record.uses} == {first.source.id, later.source.id}


def test_raw_date_spelling_with_same_parsed_dates_keeps_qa_version(
    db: Database,
) -> None:
    entry = (
        page()
        .qa[0]
        .model_copy(update={"updated_on": "2026-10-01", "date_raw": "2026/10/01"})
    )
    first = page(question=entry)
    later = page(
        "TEST-002",
        hour=1,
        question=entry.model_copy(update={"date_raw": "2026-10-01"}),
    )
    plan = plan_card_extras(db, (later, first))
    with db.transaction():
        record = populate_card_extras(db, plan, build=context(plan))
    versions = db.rows("qa_version")
    assert len(versions) == 1
    assert versions[0].values["published_on"] == "2026-10-01"
    assert versions[0].values["updated_on"] == "2026-10-01"
    assert versions[0].values["date_raw"] == "2026/10/01"
    assert {row.values["card_id"] for row in db.rows("qa_card")} == {"card", "card1"}
    assert {use.source.id for use in record.uses} == {first.source.id, later.source.id}


def test_withdrawal_only_creates_qa_version(db: Database) -> None:
    first = page()
    withdrawn = page(
        hour=1, question=first.qa[0].model_copy(update={"state": "withdrawn"})
    )
    plan = plan_card_extras(db, (withdrawn, first))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    versions = sorted(
        db.rows("qa_version"), key=lambda row: str(row.values["revision"])
    )
    assert len(versions) == 2
    assert [row.values["state"] for row in versions] == ["active", "withdrawn"]
    assert versions[1].values["supersedes_id"] == versions[0].values["id"]
    assert versions[1].values["published_on"] == versions[0].values["published_on"]
    assert (
        versions[1].values["question_unit_id"] == versions[0].values["question_unit_id"]
    )
    assert versions[1].values["answer_unit_id"] == versions[0].values["answer_unit_id"]


def test_regional_qa_identity_is_independent(db: Database) -> None:
    plan = plan_card_extras(db, (page(), page(region="en")))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    assert len(db.rows("qa")) == 2
    assert {r.values["region"] for r in db.rows("qa")} == {"jp", "en"}
    assert {r.values["lang"] for r in db.rows("text_unit")} == {"ja", "en"}


@pytest.mark.parametrize(
    ("href", "reason"),
    [
        ("?cardno=UNKNOWN", "related_target_missing"),
        ("https://example.invalid/?cardno=TEST-002", "related_link_unrecognized"),
        ("?cardno=TEST-001EN", "related_target_missing"),
        ("?cardno=TEST-001%E2%93%88a", "related_same_card"),
    ],
)
def test_unresolved_related_links_stay_staged(
    db: Database, href: str, reason: str
) -> None:
    current = page().model_copy(
        update={"related": (RelatedLink(locator="related:0", href_raw=href),)}
    )
    plan = plan_card_extras(db, (current,))
    assert any(g.category == reason for g in plan.gaps)
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    assert not db.rows("card_related")
    assert reason in canonical(plan.report()).decode()
    assert "Synthetic answer" not in canonical(plan.report()).decode()


def test_related_is_exact_regional_printing_without_token_guess(db: Database) -> None:
    current = page("TEST-002").model_copy(
        update={
            "related": (
                RelatedLink(locator="related:0", href_raw="?cardno=TEST-001%E2%93%88a"),
            )
        }
    )
    plan = plan_card_extras(db, (current,))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    relation = db.rows("card_related")[0].values
    assert relation["to_card_id"] == "card"
    assert relation["target_printing_id"] == "printing"
    assert relation["relation"] == "official_unspecified"
    assert relation["suggested_count"] is None
    assert relation["dsl_id"] is None


def test_errata_reference_reports_manual_faces_without_excluding_cards(
    db: Database,
) -> None:
    current = page().model_copy(
        update={"errata_urls": ("https://shadowverse-evolve.com/errata/synthetic/",)}
    )
    plan = plan_card_extras(db, (current,))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    assert plan.report()["source_windows"] == []
    visible = {table: db.rows(table) for table in ("card", "printing", "face")}
    unaffected = (("en", "card"), ("jp", "card1"))
    assert require_card_extras_ready(db, unaffected, strict=True) == ()
    scope = (*unaffected, ("jp", "card"))
    restrictions = require_card_extras_ready(db, scope)
    assert len(restrictions) == 1
    restriction = restrictions[0]
    assert (restriction.region, restriction.card_id, restriction.scope_key) == (
        "jp",
        "card",
        "card",
    )
    assert restriction.face_ids == ("face",)
    assert restriction.reason == "errata_current_pending"
    assert restriction.issue_id == db.rows("build_issue")[0].values["id"]
    with pytest.raises(ValueError, match="blocks automation or confirmed current"):
        require_card_extras_ready(db, scope, strict=True)
    assert {table: db.rows(table) for table in visible} == visible
    assert not db.rows("face_current")


def test_pending_faces_are_regional_even_for_shared_card(db: Database) -> None:
    printing = dict(db.rows("printing")[0].values)
    face = dict(db.rows("face")[0].values)
    pair = dict(db.rows("printing_face")[0].values)
    with db.transaction():
        db.update("card", {"id": "card"}, {"layout": "double_faced"})
        db.insert("face", face | {"id": "back", "ordinal": 1, "side": "back"})
        db.insert(
            "printing",
            printing | {"id": "shared_en", "region": "en", "card_no": "TEST-SHARED"},
        )
        db.insert(
            "printing_face", pair | {"printing_id": "shared_en", "face_id": "back"}
        )
    jp = page().model_copy(
        update={"errata_urls": ("https://shadowverse-evolve.com/errata/synthetic/",)}
    )
    en = page("TEST-SHARED", region="en").model_copy(
        update={"errata_urls": ("https://en.shadowverse-evolve.com/errata/synthetic/",)}
    )
    plan = plan_card_extras(db, (jp, en))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    restrictions = require_card_extras_ready(db, (("jp", "card"), ("en", "card")))
    assert len(restrictions) == 2
    assert {(item.region, item.card_id, item.face_ids) for item in restrictions} == {
        ("jp", "card", ("face",)),
        ("en", "card", ("back",)),
    }
    assert require_card_extras_ready(db, (("jp", "card"),))[0].face_ids == ("face",)
    assert require_card_extras_ready(db, (("en", "card"),))[0].face_ids == ("back",)


def notice() -> ErrataPage:
    url = "https://shadowverse-evolve.com/errata/synthetic/"
    return ErrataPage(
        source=source().model_copy(update={"url": url}),
        region="jp",
        official_url=url,
        announced_on="2026-09-30",
        effective_on="2026-10-02",
        changes=(
            ErrataChange(
                card_no="TEST-001Ⓢa",
                face_id="face",
                field="effect",
                before_value="Synthetic before fragment",
                after_value="Synthetic after fragment",
                locator="fragment:0",
            ),
        ),
        printings=(ErrataPrinting(card_no="TEST-001Ⓢa"),),
    )


def test_errata_dates_fragments_and_listing_are_independent(db: Database) -> None:
    plan = plan_card_extras(db, (), errata=(notice(),))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    version = db.rows("errata_version")[0].values
    assert version["announced_on"] == "2026-09-30"
    assert version["effective_on"] == "2026-10-02"
    change = db.rows("errata_change")[0].values
    assert change["before_revision_id"] is None
    assert change["after_revision_id"] is None
    assert not db.rows("face_revision")
    assert not db.rows("face_current")
    assert db.rows("errata_printing")[0].values["scope"] == "listed"
    assert db.rows("errata_printing")[0].values["decision_id"] is None
    assert require_card_extras_ready(db, (("jp", "card"),))[0].face_ids == ("face",)
    with pytest.raises(ValueError, match="blocks automation"):
        require_card_extras_ready(db, (("jp", "card"),), strict=True)


@pytest.mark.parametrize(
    "fault",
    [
        "unknown_printing",
        "wrong_face",
        "missing_printing_face",
        "unconfirmed",
        "unpinned",
    ],
)
def test_errata_rejects_invented_identity_or_confirmation(
    db: Database, fault: str
) -> None:
    item = notice()
    if fault == "unknown_printing":
        item = item.model_copy(
            update={"printings": (ErrataPrinting(card_no="UNKNOWN"),)}
        )
    elif fault == "wrong_face":
        item = item.model_copy(
            update={
                "changes": (item.changes[0].model_copy(update={"face_id": "face1"}),)
            }
        )
    elif fault == "missing_printing_face":
        with db.transaction():
            db.delete("printing_face", {"printing_id": "printing", "face_id": "face"})
        assert db.rows("face")[0].values["card_id"] == "card"
    else:
        item = item.model_copy(
            update={
                "printings": (
                    ErrataPrinting(
                        card_no="TEST-001Ⓢa",
                        scope="confirmed_applies",
                        decision_id="decision",
                    ),
                )
            }
        )
        if fault == "unconfirmed":
            with db.transaction():
                db.insert("source_record", item.source.values())
                db.insert(
                    "decision_source",
                    {
                        "decision_id": "decision",
                        "source_id": item.source.id,
                        "role": "errata_applicability",
                    },
                )
                db.update(
                    "decision",
                    {"id": "decision"},
                    {"state": "proposed"},
                )
    plan = plan_card_extras(db, (), errata=(item,))
    before = db.rows("source_record")
    with pytest.raises(ValueError, match=r"Errata|Confirmed"), db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    assert not db.rows("errata")
    assert db.rows("source_record") == before


def test_confirmed_errata_requires_pinned_decision(db: Database) -> None:
    item = notice()
    item = item.model_copy(
        update={
            "printings": (
                ErrataPrinting(
                    card_no="TEST-001Ⓢa",
                    scope="confirmed_applies",
                    decision_id="decision",
                ),
            )
        }
    )
    plan = plan_card_extras(db, (), errata=(item,))
    with db.transaction():
        populate_source = item.source.values()
        db.insert("source_record", populate_source)
        db.insert(
            "decision_source",
            {
                "decision_id": "decision",
                "source_id": item.source.id,
                "role": "errata_applicability",
            },
        )
        populate_card_extras(db, plan, build=context(plan))
    assert db.rows("errata_printing")[0].values["scope"] == "confirmed_applies"


def test_missing_confirmed_decision_is_invalid_input() -> None:
    with pytest.raises(ValidationError, match="requires a decision"):
        ErrataPrinting(card_no="synthetic", scope="confirmed_applies")


def test_stale_plan_and_wrong_configuration_fail_before_writes(db: Database) -> None:
    plan = plan_card_extras(db, (page(),))
    with pytest.raises(ValueError, match="adopted identities"), db.transaction():
        populate_card_extras(db, replace(plan, questions=()), build=context(plan))
    changed = replace(plan, pages=())
    with pytest.raises(ValueError, match="configuration mismatch"), db.transaction():
        populate_card_extras(db, plan, build=context(changed))
    assert not db.rows("qa")


def _omit_use(build: BuildContext, uses: tuple[SourceUse, ...]) -> InputRecord:
    return input_record(build, uses[1:])


def test_cardlist_complete_does_not_imply_qa_or_errata_coverage(db: Database) -> None:
    with db.transaction():
        db.insert(
            "source_coverage",
            {
                "kind": "cardlist",
                "region": "jp",
                "scope_key": "region:*",
                "from_date": "2026-01-01",
                "until_date": None,
                "as_of": "2026-10-01",
                "state": "complete",
                "source_id": "source",
            },
        )
    plan = plan_card_extras(db, (page(),))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    assert {row.values["kind"] for row in db.rows("source_coverage")} == {"cardlist"}
    assert plan.report()["source_windows"] == []


def test_unknown_source_is_staged_without_inventing_card(db: Database) -> None:
    plan = plan_card_extras(db, (page("UNKNOWN"),))
    assert not plan.questions
    with db.transaction():
        record = populate_card_extras(db, plan, build=context(plan))
    assert record.uses == plan.source_uses()
    assert not db.rows("qa")
    restrictions = require_card_extras_ready(db, (("jp", "jp:UNKNOWN"),))
    assert len(restrictions) == 1
    assert restrictions[0].card_id is None
    assert restrictions[0].face_ids == ()
    assert restrictions[0].reason == "source_printing_missing"
    with pytest.raises(ValueError, match="blocks automation"):
        require_card_extras_ready(db, (("jp", "jp:UNKNOWN"),), strict=True)


def test_plan_is_independent_of_page_order(db: Database) -> None:
    pages = (page(), page("TEST-002"))
    first = plan_card_extras(db, pages)
    second = plan_card_extras(db, reversed(pages))
    assert first == second
    assert first.configuration() == second.configuration()
    assert first.source_uses() == second.source_uses()


def test_duplicate_locators_and_wrong_region_are_rejected(db: Database) -> None:
    current = page()
    invalid = current.model_copy(update={"qa": (current.qa[0], current.qa[0])})
    with pytest.raises(ValidationError, match="Duplicate"):
        plan_card_extras(db, (invalid,))
    with pytest.raises(ValueError, match="identity mismatch"):
        plan_card_extras(db, (current.model_copy(update={"region": "en"}),))


def test_errata_same_day_changed_fragment_keeps_both_versions(db: Database) -> None:
    first = notice()
    second = first.model_copy(
        update={
            "source": first.source.model_copy(
                update={
                    "id": source(hour=1).id,
                    "fetched_at": source(hour=1).fetched_at,
                }
            ),
            "changes": (
                first.changes[0].model_copy(
                    update={"after_value": "Changed synthetic fragment"}
                ),
            ),
        }
    )
    plan = plan_card_extras(db, (), errata=(second, first))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    versions = sorted(
        (row.values for row in db.rows("errata_version")),
        key=lambda r: str(r["revision"]),
    )
    assert len(versions) == 2
    assert versions[0]["announced_on"] == versions[1]["announced_on"]
    assert versions[1]["supersedes_id"] == versions[0]["id"]


def test_adjacent_identical_notices_share_version_and_keep_sources(
    db: Database,
) -> None:
    first = notice()
    later = first.model_copy(
        update={
            "source": first.source.model_copy(
                update={
                    "id": source(hour=1).id,
                    "fetched_at": source(hour=1).fetched_at,
                }
            )
        }
    )
    plan = plan_card_extras(db, (), errata=(later, first))
    with db.transaction():
        record = populate_card_extras(db, plan, build=context(plan))
    assert len(db.rows("errata")) == 1
    assert len(db.rows("errata_version")) == 1
    assert db.rows("errata_version")[0].values["source_id"] == first.source.id
    assert len(db.rows("errata_change")) == 1
    assert len(db.rows("errata_printing")) == 1
    assert {use.source.id for use in record.uses} == {first.source.id, later.source.id}
    assert {row.values["id"] for row in db.rows("source_record")} >= {
        first.source.id,
        later.source.id,
    }


@pytest.mark.parametrize(
    "message", ['"invalid context"', '{"region":1}', '{"region":"jp","card_id":false}']
)
def test_pending_issue_requires_valid_identity_context(
    db: Database, message: str
) -> None:
    current = page().model_copy(
        update={"errata_urls": ("https://shadowverse-evolve.com/errata/synthetic/",)}
    )
    plan = plan_card_extras(db, (current,))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
        db.update(
            "build_issue",
            {"id": db.rows("build_issue")[0].values["id"]},
            {"message": message},
        )
    with pytest.raises(TypeError, match="Invalid card extras pending issue"):
        require_card_extras_ready(db, (("jp", "card"),))


def test_errata_url_must_match_pinned_source() -> None:
    with pytest.raises(ValidationError, match="differs"):
        ErrataPage(
            source=source(),
            region="jp",
            official_url="https://shadowverse-evolve.com/errata/synthetic/",
        )


def test_same_day_reversion_is_a_third_observed_version(db: Database) -> None:
    first = page()
    second = page(
        hour=1,
        question=first.qa[0].model_copy(update={"answer": "Changed synthetic answer"}),
    )
    third = page(hour=2, question=first.qa[0])
    plan = plan_card_extras(db, (third, first, second))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    versions = sorted(
        (row.values for row in db.rows("qa_version")),
        key=lambda row: str(row["revision"]),
    )
    assert len(versions) == 3
    assert versions[0]["answer_unit_id"] == versions[2]["answer_unit_id"]
    assert versions[2]["supersedes_id"] == versions[1]["id"]
    assert versions[2]["id"] != versions[0]["id"]


@pytest.mark.parametrize("fault", ["region", "media", "calendar"])
def test_errata_boundary_rejects_unofficial_or_impossible_input(fault: str) -> None:
    data = notice().model_dump(mode="json")
    if fault == "region":
        data["region"] = "en"
    elif fault == "media":
        data["source"]["kind"] = "third_party_page"
    else:
        data["effective_on"] = "2026-02-30"
    with pytest.raises(ValidationError, match=r"region/media|string_pattern_mismatch"):
        ErrataPage.model_validate_json(canonical(data))


def test_errata_reversion_does_not_discard_last_observation(db: Database) -> None:
    first = notice()
    second = first.model_copy(
        update={
            "source": first.source.model_copy(
                update={
                    "id": source(hour=1).id,
                    "fetched_at": source(hour=1).fetched_at,
                }
            ),
            "changes": (
                first.changes[0].model_copy(
                    update={"after_value": "Different synthetic fragment"}
                ),
            ),
        }
    )
    third = first.model_copy(
        update={
            "source": first.source.model_copy(
                update={
                    "id": source(hour=2).id,
                    "fetched_at": source(hour=2).fetched_at,
                }
            )
        }
    )
    plan = plan_card_extras(db, (), errata=(first, third, second))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    versions = sorted(
        (row.values for row in db.rows("errata_version")),
        key=lambda row: str(row["revision"]),
    )
    assert len(versions) == 3
    assert versions[2]["source_id"] == third.source.id
    assert versions[2]["supersedes_id"] == versions[1]["id"]


def test_en_errata_keeps_unknown_dates_and_exact_regional_scope(db: Database) -> None:
    url = "https://en.shadowverse-evolve.com/errata/synthetic/"
    item = ErrataPage(
        source=source(region="en").model_copy(
            update={"url": url, "kind": "official_pdf"}
        ),
        region="en",
        official_url=url,
        date_raw="Unspecified synthetic date",
        reason="Synthetic English reason",
        changes=(
            ErrataChange(
                card_no="TEST-001Ⓢa",
                face_id="face2",
                field="defense",
                before_value=1,
                after_value=2,
                locator="fragment:0",
            ),
        ),
        printings=(ErrataPrinting(card_no="TEST-001Ⓢa"),),
    )
    plan = plan_card_extras(db, (), errata=(item,))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    version = db.rows("errata_version")[0].values
    assert version["announced_on"] is None
    assert version["effective_on"] is None
    assert version["date_raw"] == item.date_raw
    reason = next(
        row.values
        for row in db.rows("text_unit")
        if row.values["id"] == version["reason_unit_id"]
    )
    assert reason["lang"] == "en"
    assert db.rows("errata_change")[0].values["face_id"] == "face2"
    assert db.rows("errata_printing")[0].values["printing_id"] == "printing2"


@pytest.mark.parametrize("field", ["context", "identity", "target"])
def test_pending_issue_context_is_checked_at_the_boundary(
    db: Database, field: str
) -> None:
    plan = plan_card_extras(
        db,
        (
            page().model_copy(
                update={
                    "errata_urls": ("https://shadowverse-evolve.com/errata/synthetic/",)
                }
            ),
        ),
    )
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
        issue = next(
            row.values
            for row in db.rows("build_issue")
            if row.values["category"] == "card_extras:errata_current_pending"
        )
        invalid: JsonValue = (
            None
            if field == "context"
            else {
                "region": 42 if field == "identity" else "jp",
                "card_id": "card",
                "target": [] if field == "target" else "synthetic",
            }
        )
        db.update(
            "build_issue", {"id": issue["id"]}, {"message": canonical(invalid).decode()}
        )
    with pytest.raises(
        TypeError, match=r"^Invalid card extras pending issue " + field + "$"
    ):
        require_card_extras_ready(db, (("jp", "card"),))
