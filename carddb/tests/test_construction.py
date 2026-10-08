"""Construction evidence gates, cross-table invariants and dated unknown cases."""

import sqlite3
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.build_db import Database, Value, create_database
from sve_carddb.build_db.source_rows import source_values
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.construction import (
    DeckRoleOverride,
    load_construction,
    populate_construction,
    resolve_construction,
)
from sve_carddb.core.json import canonical
from sve_carddb.products.models import LocalizedText

from .build_db_fixtures import rows
from .card_extras_fixtures import seed
from .construction_fixtures import context, evidence, plan

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build_db.compiler import CompiledSchema

REFS = frozenset({"synthetic-construction-v1"})


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_build(("cr",))


@pytest.fixture(scope="module")
def baseline(schema: CompiledSchema) -> tuple[tuple[str, dict[str, Value]], ...]:
    with create_database(schema) as db:
        seed(db)
        with db.transaction():
            db.insert("rules_name", rows()["rules_name"])
            db.insert(
                "rules_name",
                rows()["rules_name"]
                | {"id": "rules_name2", "official_name": "Synthetic two"},
            )
            db.insert(
                "rules_name", rows()["rules_name"] | {"id": "en_name", "region": "en"}
            )
        return tuple(
            (table.name, dict(row.values))
            for table in schema.tables
            for row in db.rows(table.name)
        )


@pytest.fixture
def db(
    schema: CompiledSchema, baseline: tuple[tuple[str, dict[str, Value]], ...]
) -> Iterator[Database]:
    with create_database(schema) as database:
        with database.transaction():
            for table, values in baseline:
                database.insert(table, values)
        yield database


@pytest.fixture(scope="module")
def ready_db(
    schema: CompiledSchema, baseline: tuple[tuple[str, dict[str, Value]], ...]
) -> Iterator[Database]:
    with create_database(schema) as database:
        with database.transaction():
            for table, values in baseline:
                database.insert(table, values)
            staging = plan()
            populate_construction(database, staging, build=context(staging))
        yield database


class TestReadOnlyConstruction:
    def test_inputs_ready_still_do_not_evaluate_legality(
        self, ready_db: Database
    ) -> None:
        resolved = resolve_construction(
            ready_db,
            "profile",
            on_date="2026-10-01",
            as_of="2026-10-02",
            supported_refs=REFS,
        )
        assert resolved.inputs_state == "ready"
        assert resolved.reasons == ()
        assert resolved.revision_id == "profile_revision"
        assert resolved.restriction_ids == ("choice", "limit")
        assert resolved.legality == "unknown"

    @pytest.mark.parametrize("day", ["2026-08-31", "2026-10-03"])
    def test_outside_coverage_unknown(self, ready_db: Database, day: str) -> None:
        resolved = resolve_construction(
            ready_db, "profile", on_date=day, as_of="2026-10-04", supported_refs=REFS
        )
        assert resolved.inputs_state == "unknown"
        assert "coverage_unknown" in resolved.reasons
        assert resolved.legality == "unknown"

    def test_unknown_algorithm_not_known(self, ready_db: Database) -> None:
        resolved = resolve_construction(
            ready_db,
            "profile",
            on_date="2026-10-01",
            as_of="2026-10-02",
            supported_refs=frozenset(),
        )
        assert resolved.inputs_state == "unknown"
        assert "construction_rules_unknown" in resolved.reasons

    def test_unknown_profile_not_known(self, ready_db: Database) -> None:
        resolved = resolve_construction(
            ready_db,
            "other",
            on_date="2026-10-01",
            as_of="2026-10-02",
            supported_refs=REFS,
        )
        assert resolved.inputs_state == "unknown"
        assert "profile_missing" in resolved.reasons

    def test_source_and_clause_entities(self, ready_db: Database) -> None:
        staging = plan()
        assert (
            ready_db.rows("cr_version")[0].values["source_url"]
            == staging.cr_versions[0].evidence.source.url
        )
        assert ready_db.rows("cr_clause")[0].values["cr_version_id"] == "cr"
        assert (
            ready_db.rows("rules_profile_revision")[0].values["cr_version_id"] == "cr"
        )
        for table in (
            "rules_profile_revision",
            "restriction",
            "restriction_coverage",
            "cr_version",
        ):
            assert all(
                row.values["source_id"] == evidence().source.id
                for row in ready_db.rows(table)
            )
        assert any(use.locator == "synthetic:clause" for use in staging.source_uses())
        members = ready_db.rows("restriction_member")
        assert len(members) == 3
        assert {
            row.values["choice_option"]
            for row in members
            if row.values["restriction_id"] == "choice"
        } == {0, 1}


def test_strict_json_roundtrip_and_duplicate_keys() -> None:
    staging = plan()
    assert load_construction(staging.model_dump_json().encode()) == staging
    with pytest.raises(ValueError, match="Duplicate"):
        load_construction(b'{"profiles":[],"profiles":[]}')
    with pytest.raises(ValidationError):
        load_construction(b'{"unknown":true}')


@pytest.mark.parametrize(
    "update",
    [
        {"max_copies": None},
        {"max_selected_groups": 1},
        {"max_copies": True},
        {"max_copies": -1},
        {"effective_until": "2026-09-01"},
        {"effective_from": "2026-02-30"},
        {"members": []},
    ],
)
def test_invalid_restriction_boundary(update: dict[str, object]) -> None:
    staging = plan().model_dump(mode="json")
    staging["restrictions"][0].update(update)
    with pytest.raises(ValidationError):
        load_construction(canonical(staging))


def test_choice_group_cannot_also_have_copy_limit() -> None:
    staging = plan().model_dump(mode="json")
    staging["restrictions"][1]["max_copies"] = 0
    with pytest.raises(ValidationError, match="exclusively"):
        load_construction(canonical(staging))


@pytest.mark.parametrize("kind", ["image", "third_party_page", "third_party_audio"])
def test_nonofficial_evidence_rejected(kind: str) -> None:
    staging = plan().model_dump(mode="json")
    staging["revisions"][0]["evidence"]["source"]["kind"] = kind
    with pytest.raises(ValidationError, match="official frozen"):
        load_construction(canonical(staging))


def test_cr_clause_wrong_region_language() -> None:
    staging = plan().model_dump(mode="json")
    staging["cr_versions"][0]["clauses"][0]["text"]["lang"] = "en"
    with pytest.raises(ValidationError, match="regional language"):
        load_construction(canonical(staging))


def test_context_must_pin_exact_staging(db: Database) -> None:
    staging = plan()
    changed = staging.model_copy(update={"coverage": ()})
    with pytest.raises(ValueError, match="configuration"), db.transaction():
        populate_construction(db, changed, build=context(staging))
    assert not db.rows("rules_profile")


def test_source_metadata_conflict_rolls_back(db: Database) -> None:
    staging = plan()
    with db.transaction():
        db.insert(
            "source_record",
            source_values(evidence().source) | {"sha256": "sha256:" + "b" * 64},
        )
    with pytest.raises(ValueError, match="Conflicting raw source"), db.transaction():
        populate_construction(db, staging, build=context(staging))
    assert not db.rows("rules_profile")


def test_idempotent_and_conflicting_import(db: Database) -> None:
    staging = plan()
    with db.transaction():
        populate_construction(db, staging, build=context(staging))
        populate_construction(db, staging, build=context(staging))
    assert len(db.rows("restriction_member")) == 3
    changed = staging.model_copy(
        update={
            "profiles": (
                staging.profiles[0].model_copy(
                    update={"name": LocalizedText(lang="ja", text="Changed synthetic")}
                ),
            )
        }
    )
    with pytest.raises(ValueError, match="Conflicting"), db.transaction():
        populate_construction(db, changed, build=context(changed))


def test_reimport_cannot_silently_union_changed_members(db: Database) -> None:
    staging = plan()
    with db.transaction():
        populate_construction(db, staging, build=context(staging))
    changed = staging.model_copy(
        update={
            "restrictions": (
                staging.restrictions[1].model_copy(
                    update={"members": staging.restrictions[1].members[:1]}
                ),
            )
        }
    )
    with pytest.raises(ValueError, match="membership"), db.transaction():
        populate_construction(db, changed, build=context(changed))
    assert len(db.rows("restriction_member")) == 3


@pytest.mark.parametrize("cr_id", ["missing", None])
def test_nonempty_cr_fk_requires_entity(db: Database, cr_id: str | None) -> None:
    staging = plan()
    changed = staging.model_copy(
        update={
            "cr_versions": (),
            "revisions": (
                staging.revisions[0].model_copy(update={"cr_version_id": cr_id}),
            ),
        }
    )
    if cr_id is None:
        with db.transaction():
            populate_construction(db, changed, build=context(changed))
        assert db.rows("rules_profile_revision")[0].values["cr_version_id"] is None
    else:
        with pytest.raises((ValueError, sqlite3.IntegrityError)), db.transaction():
            populate_construction(db, changed, build=context(changed))
        assert not db.rows("rules_profile")


def test_referenced_cr_requires_clause(db: Database) -> None:
    staging = plan()
    changed = staging.model_copy(
        update={
            "cr_versions": (staging.cr_versions[0].model_copy(update={"clauses": ()}),)
        }
    )
    with pytest.raises(ValueError, match="concrete clauses"), db.transaction():
        populate_construction(db, changed, build=context(changed))


def test_cr_profile_region_mismatch(db: Database) -> None:
    staging = plan()
    changed = staging.model_copy(
        update={
            "cr_versions": (
                staging.cr_versions[0].model_copy(
                    update={
                        "region": "en",
                        "clauses": (
                            staging.cr_versions[0]
                            .clauses[0]
                            .model_copy(
                                update={
                                    "text": LocalizedText(
                                        lang="en", text="Synthetic English clause"
                                    )
                                }
                            ),
                        ),
                    }
                ),
            )
        }
    )
    with (
        pytest.raises(sqlite3.IntegrityError, match="profile_cr_region"),
        db.transaction(),
    ):
        populate_construction(db, changed, build=context(changed))


def test_restriction_member_region_mismatch(db: Database) -> None:
    staging = plan()
    restriction = staging.restrictions[0].model_copy(
        update={
            "members": (
                staging.restrictions[0]
                .members[0]
                .model_copy(update={"rules_name_id": "en_name"}),
            )
        }
    )
    changed = staging.model_copy(update={"restrictions": (restriction,)})
    with (
        pytest.raises(sqlite3.IntegrityError, match="restriction_member_region"),
        db.transaction(),
    ):
        populate_construction(db, changed, build=context(changed))


@pytest.mark.parametrize(
    ("until", "second_from", "valid"),
    [
        (None, "2026-10-01", False),
        ("2026-10-02", "2026-10-01", False),
        ("2026-10-02", "2026-10-02", True),
    ],
)
def test_profile_intervals_half_open(
    db: Database, until: str | None, second_from: str, valid: bool
) -> None:
    staging = plan()
    first = staging.revisions[0].model_copy(update={"effective_until": until})
    second = first.model_copy(
        update={
            "id": "revision2",
            "effective_from": second_from,
            "effective_until": None,
        }
    )
    changed = staging.model_copy(update={"revisions": (first, second)})
    if valid:
        with db.transaction():
            populate_construction(db, changed, build=context(changed))
        assert (
            resolve_construction(
                db,
                "profile",
                on_date=second_from,
                as_of=second_from,
                supported_refs=REFS,
            ).revision_id
            == "revision2"
        )
    else:
        with (
            pytest.raises(sqlite3.IntegrityError, match="profile_revision_overlap"),
            db.transaction(),
        ):
            populate_construction(db, changed, build=context(changed))


@pytest.mark.parametrize("state", ["announced", "withdrawn"])
def test_unconfirmed_or_withdrawn_not_active_confirmed(
    db: Database, state: str
) -> None:
    staging = plan()
    changed = staging.model_copy(
        update={
            "restrictions": (
                staging.restrictions[0].model_copy(update={"state": state}),
            )
        }
    )
    with db.transaction():
        populate_construction(db, changed, build=context(changed))
    resolved = resolve_construction(
        db, "profile", on_date="2026-10-01", as_of="2026-10-02", supported_refs=REFS
    )
    assert resolved.restriction_ids == ()
    if state == "announced":
        assert resolved.inputs_state == "unknown"
        assert "restriction_unconfirmed" in resolved.reasons
    else:
        assert resolved.inputs_state == "ready"


@pytest.mark.parametrize(
    "state", ["proposed", "model_reviewed", "sampled", "rejected", "disputed"]
)
def test_db_unconfirmed_decision_not_confirmed(db: Database, state: str) -> None:
    staging = plan()
    with db.transaction():
        populate_construction(db, staging, build=context(staging))
    with pytest.raises(sqlite3.IntegrityError, match="confirmed_decision"):
        _unconfirmed_material(db, state)


def _unconfirmed_material(db: Database, state: str) -> None:
    with db.transaction():
        db.update("decision", {"id": "decision"}, {"state": state})
        db.update("restriction", {"id": "limit"}, {"decision_id": "decision"})


@pytest.mark.parametrize("table", ["restriction", "deck_role_override"])
@pytest.mark.parametrize("state", ["proposed", "confirmed"])
def test_import_cannot_substitute_identity_decision_for_rule_adoption(
    db: Database, table: str, state: str
) -> None:
    staging = plan()
    with db.transaction():
        db.update("decision", {"id": "decision"}, {"state": state})
        db.insert("decision_source", rows()["decision_source"])
    changed = staging.model_copy(
        update={
            "restrictions": (
                staging.restrictions[0].model_copy(update={"decision_id": "decision"}),
            ),
        }
        if table == "restriction"
        else {
            "overrides": (
                DeckRoleOverride(
                    card_id="card", region="jp", role="extra", decision_id="decision"
                ),
            ),
        }
    )
    with (
        pytest.raises(
            ValueError,
            match="confirmed decision"
            if state == "proposed"
            else "authored adoption contract",
        ),
        db.transaction(),
    ):
        populate_construction(db, changed, build=context(changed))
    assert not db.rows("rules_profile")


def test_override_wrong_card_region_rejected(db: Database) -> None:
    with (
        pytest.raises(sqlite3.IntegrityError, match="deck_role_regional_card"),
        db.transaction(),
    ):
        db.insert("deck_role_override", rows()["deck_role_override"] | {"region": "en"})


@pytest.mark.parametrize("field", ["construction_rules_ref", "default_copy_limit"])
def test_null_construction_inputs_unknown(db: Database, field: str) -> None:
    staging = plan()
    changed = staging.model_copy(
        update={"revisions": (staging.revisions[0].model_copy(update={field: None}),)}
    )
    with db.transaction():
        populate_construction(db, changed, build=context(changed))
    resolved = resolve_construction(
        db, "profile", on_date="2026-10-01", as_of="2026-10-02", supported_refs=REFS
    )
    assert resolved.inputs_state == "unknown"
    assert (
        "construction_rules_unknown"
        if field == "construction_rules_ref"
        else "copy_limit_unknown"
    ) in resolved.reasons


@pytest.mark.parametrize("coverage_state", ["partial", "missing"])
def test_unknown_coverage_not_complete(db: Database, coverage_state: str) -> None:
    staging = plan()
    changed = staging.model_copy(
        update={
            "coverage": ()
            if coverage_state == "missing"
            else (staging.coverage[0].model_copy(update={"state": "partial"}),)
        }
    )
    with db.transaction():
        populate_construction(db, changed, build=context(changed))
    resolved = resolve_construction(
        db, "profile", on_date="2026-10-01", as_of="2026-10-02", supported_refs=REFS
    )
    assert resolved.inputs_state == "unknown"
    assert "coverage_unknown" in resolved.reasons


def test_open_coverage_not_future_proof(db: Database) -> None:
    staging = plan()
    changed = staging.model_copy(
        update={
            "coverage": (staging.coverage[0].model_copy(update={"until_date": None}),)
        }
    )
    with db.transaction():
        populate_construction(db, changed, build=context(changed))
    resolved = resolve_construction(
        db, "profile", on_date="2026-10-03", as_of="2026-10-02", supported_refs=REFS
    )
    assert resolved.inputs_state == "unknown"
    assert "after_as_of" in resolved.reasons


def test_coverage_overlap_requires_reconciliation(db: Database) -> None:
    staging = plan()
    changed = staging.model_copy(
        update={
            "coverage": (
                *staging.coverage,
                staging.coverage[0].model_copy(
                    update={"from_date": "2026-10-01", "state": "partial"}
                ),
            )
        }
    )
    with pytest.raises(ValueError, match="Overlapping coverage"), db.transaction():
        populate_construction(db, changed, build=context(changed))
    with db.transaction():
        populate_construction(db, staging, build=context(staging))
        db.insert(
            "restriction_coverage",
            {
                "profile_id": "profile",
                "from_date": "2026-10-01",
                "until_date": None,
                "state": "partial",
                "source_id": evidence().source.id,
            },
        )
    resolved = resolve_construction(
        db, "profile", on_date="2026-10-01", as_of="2026-10-02", supported_refs=REFS
    )
    assert resolved.inputs_state == "unknown"
    assert "coverage_unknown" in resolved.reasons


def test_confirmed_override_database_shape(db: Database) -> None:
    with db.transaction():
        db.insert("deck_role_override", rows()["deck_role_override"])
    assert db.rows("deck_role_override")[0].values["region"] == "jp"


def test_cr_fk_independent_of_clause_gate(db: Database) -> None:
    staging = plan()
    with db.transaction():
        populate_construction(db, staging, build=context(staging))
    with pytest.raises(sqlite3.IntegrityError, match="Foreign key"), db.transaction():
        db.update(
            "rules_profile_revision",
            {"id": "profile_revision"},
            {"cr_version_id": "absent"},
        )


def test_confirmed_restriction_without_members_not_ready(db: Database) -> None:
    staging = plan()
    with db.transaction():
        populate_construction(db, staging, build=context(staging))
    with db.transaction():
        db.delete(
            "restriction_member",
            {
                "restriction_id": "limit",
                "rules_name_id": "rules_name",
                "deck_scope": "all",
            },
        )
    assert db.rows("restriction_member")
    resolved = resolve_construction(
        db, "profile", on_date="2026-10-01", as_of="2026-10-02", supported_refs=REFS
    )
    assert resolved.inputs_state == "unknown"
    assert resolved.reasons == ("restriction_members_missing",)
    assert resolved.legality == "unknown"


def test_missing_revision_not_current(db: Database) -> None:
    staging = plan().model_copy(update={"revisions": ()})
    with db.transaction():
        populate_construction(db, staging, build=context(staging))
    resolved = resolve_construction(
        db, "profile", on_date="2026-10-01", as_of="2026-10-02", supported_refs=REFS
    )
    assert resolved.inputs_state == "unknown"
    assert "revision_unknown" in resolved.reasons


@pytest.mark.parametrize("day", ["2026-02-30", "2026-10-01T00:00:00Z"])
def test_query_dates_are_calendar_dates(ready_db: Database, day: str) -> None:
    with pytest.raises(ValidationError):
        resolve_construction(
            ready_db, "profile", on_date=day, as_of="2026-10-02", supported_refs=REFS
        )


@pytest.mark.parametrize(
    ("catalog_state", "available"), [("official", True), ("unlisted", False)]
)
def test_decklog_defaults_remain_independent(
    db: Database, catalog_state: str, available: bool
) -> None:
    staging = plan()
    with db.transaction():
        db.update(
            "printing",
            {"id": "printing"},
            {"catalog_state": catalog_state, "decklog_available": available},
        )
        populate_construction(db, staging, build=context(staging))
    printing = next(
        row.values for row in db.rows("printing") if row.values["id"] == "printing"
    )
    assert printing["decklog_available"] is available
    assert printing["decklog_verification"] == "unverified"
    assert printing["decklog_checked_on"] is None
    with pytest.raises(sqlite3.IntegrityError), db.transaction():
        db.update("printing", {"id": "printing"}, {"decklog_checked_on": "2026-10-01"})
    with pytest.raises(sqlite3.IntegrityError), db.transaction():
        db.update("printing", {"id": "printing"}, {"decklog_available": not available})
