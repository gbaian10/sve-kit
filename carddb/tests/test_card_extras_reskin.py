"""One shared sealed synthetic baseline; mutations use fresh small SQLite graphs."""

from datetime import date
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.card_extras import applicable_reskin_regions
from sve_carddb.registry.inputs import Mapping
from sve_carddb.registry.review import InitDecisions, Inputs
from sve_carddb.text_observations import populate_text_observations

from .test_registry import card
from .text_observation_fixtures import make_case

if TYPE_CHECKING:
    from collections.abc import Mapping as RowMapping

    from sve_carddb.build_db import CompiledSchema, Database, Value

    from .text_observation_fixtures import Case


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_build(("en", "related"))


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> Case:
    inputs = Inputs(
        jp={
            "BP02-071": card("BP02-071", "Synthetic JP"),
            "PR-001": card("PR-001", "Synthetic JP"),
        },
        en={
            "BP02-070EN": card("BP02-070EN", "Synthetic name", english=True),
            "GF01-001EN": card("GF01-001EN", "Synthetic reskin", english=True),
        },
        mapping=Mapping(
            targets={"BP02-070EN": "BP02-071", "GF01-001EN": None},
            original_art={"BP02-070EN"},
            reskins={"GF01-001EN": "BP02-071"},
        ),
        decisions=InitDecisions(),
        as_of=date(2026, 9, 28),
        jp_hash="sha256:" + "0" * 64,
    )
    for original in inputs.en.values():
        original.faces[0].sections = ["Synthetic auxiliary section"]
    return make_case(tmp_path_factory.mktemp("extras-reskin") / "authored", inputs)


@pytest.mark.parametrize(
    "fault",
    [
        "none",
        "current_removed",
        "current_semantics",
        "current_mutated",
        "current_section_changed",
        "current_stat_changed",
        "current_trait_removed",
        "source_changed",
        "new_printing",
        "relation_removed",
        "endpoint_changed",
    ],
)
def test_reskin_requires_both_current_endpoints(
    baseline: Case, schema: CompiledSchema, fault: str
) -> None:
    with create_database(schema) as db:
        with db.transaction():
            baseline.stage(db)
            populate_text_observations(
                db,
                baseline.plan,
                build=baseline.context(),
                vocabulary=baseline.vocabulary,
                published=(),
            )
        included = applicable_reskin_regions(
            db, baseline.plan, vocabulary=baseline.vocabulary
        )
        assert len(included) == 1
        identifier = next(iter(included))
        assert included[identifier] == ("en",)
        related = db.rows("card_related")[0].values
        current = next(
            row.values
            for row in db.rows("face_current")
            if row.values["region"] == "en"
        )
        if fault == "none":
            assert related["from_card_id"] != related["to_card_id"]
            assert related["dsl_id"] is None
            assert related["suggested_count"] is None
            return
        with db.transaction():
            if fault == "current_removed":
                db.delete(
                    "face_current", {"face_id": current["face_id"], "region": "en"}
                )
            elif fault == "current_semantics":
                revision = next(
                    row.values
                    for row in db.rows("face_revision")
                    if row.values["id"] == current["revision_id"]
                )
                db.insert(
                    "face_revision",
                    dict(revision)
                    | {
                        "id": "changed-revision",
                        "revision": 99,
                        "effect_unit_id": revision["name_unit_id"],
                    },
                )
                db.update(
                    "face_current",
                    {"face_id": current["face_id"], "region": "en"},
                    {"revision_id": "changed-revision"},
                )
            elif fault in {
                "current_mutated",
                "current_section_changed",
                "current_stat_changed",
                "current_trait_removed",
            }:
                _change_rules(db, current, fault)
            elif fault == "source_changed":
                printing = next(
                    row.values
                    for row in db.rows("printing")
                    if row.values["region"] == "en"
                )
                db.update(
                    "printing",
                    {"id": printing["id"]},
                    {"source_id": db.rows("source_record")[0].values["id"]},
                )
            elif fault == "new_printing":
                printing = next(
                    row.values
                    for row in db.rows("printing")
                    if row.values["region"] == "en"
                )
                db.insert(
                    "printing",
                    dict(printing) | {"id": "new-printing", "card_no": "NEW-001EN"},
                )
            elif fault == "relation_removed":
                db.delete("card_related", {"id": related["id"]})
            else:
                db.update(
                    "card_related",
                    {"id": related["id"]},
                    {
                        "from_card_id": related["to_card_id"],
                        "to_card_id": related["from_card_id"],
                    },
                )
        assert not applicable_reskin_regions(
            db, baseline.plan, vocabulary=baseline.vocabulary
        )


def _change_rules(db: Database, current: RowMapping[str, Value], fault: str) -> None:
    revision = next(
        row.values
        for row in db.rows("face_revision")
        if row.values["id"] == current["revision_id"]
    )
    if fault == "current_mutated":
        db.update(
            "face_revision",
            {"id": revision["id"]},
            {"effect_unit_id": revision["name_unit_id"]},
        )
    elif fault == "current_section_changed":
        db.update(
            "face_text_section",
            {"revision_id": current["revision_id"], "ordinal": 0},
            {"text_unit_id": revision["name_unit_id"]},
        )
    elif fault == "current_stat_changed":
        db.update("face_revision", {"id": current["revision_id"]}, {"cost": 3})
    else:
        db.delete(
            "face_trait", {"revision_id": current["revision_id"], "trait_code": "mage"}
        )
