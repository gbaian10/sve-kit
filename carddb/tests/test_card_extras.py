"""Independent synthetic counterexamples for first-release supplemental semantics."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import input_record
from sve_carddb.card_extras import (
    ErrataChange,
    ErrataPage,
    ErrataPrinting,
    QAEntry,
    RelatedLink,
    importer,
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


def test_errata_reference_blocks_only_affected_scope(db: Database) -> None:
    current = page().model_copy(
        update={"errata_urls": ("https://shadowverse-evolve.com/errata/synthetic/",)}
    )
    plan = plan_card_extras(db, (current,))
    with db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    assert plan.report()["source_windows"] == []
    require_card_extras_ready(db, (("en", "card"), ("jp", "card1")))
    with pytest.raises(ValueError, match="blocks release"):
        require_card_extras_ready(db, (("jp", "card"),))


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
    with pytest.raises(ValueError, match="blocks release"):
        require_card_extras_ready(db, (("jp", "card"),))


@pytest.mark.parametrize(
    "fault", ["unknown_printing", "wrong_face", "unconfirmed", "unpinned"]
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
                db.update(
                    "decision",
                    {"id": "decision"},
                    {"state": "proposed", "reviewed_by": None, "reviewed_at": None},
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


def test_partial_input_closure_cannot_be_hidden_by_composer(
    db: Database, monkeypatch: pytest.MonkeyPatch
) -> None:
    plan = plan_card_extras(db, (page(),))
    monkeypatch.setattr(importer, "input_record", _omit_use)
    with pytest.raises(ValueError, match="use closure"), db.transaction():
        populate_card_extras(db, plan, build=context(plan))
    assert not db.rows("qa")


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
    with pytest.raises(ValueError, match="blocks release"):
        require_card_extras_ready(db, (("jp", "jp:UNKNOWN"),))


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
