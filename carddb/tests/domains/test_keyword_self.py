"""Self keyword inspection is regional and never borrows a quoted or token ability."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json
from sve_carddb.core.json import array
from sve_carddb.domains.keyword_self import classify
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner

from .test_mechanics import case, printing, project

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize(
    ("region", "text", "spelling", "present", "unknown"),
    [
        ("jp", "【守護】（守護の説明）", "守護", True, False),
        ("jp", "これは【守護】を持つ。", "守護", True, False),
        ("en", "While active, this card has Ward.", "Ward", True, False),
    ],
)
def test_declarations_keep_ambiguous_uses_unknown(
    region: str, text: str, spelling: str, *, present: bool, unknown: bool
) -> None:
    found, uncertain = classify(text, region, {"kw:ward": spelling})
    assert ("kw:ward" in found) is present
    assert ("kw:ward" in uncertain) is unknown


@pytest.mark.parametrize(
    ("region", "icon", "ability"),
    [
        ("jp", "{クイック}", "{Q}{起動}：合成。"),
        ("en", "{icon_quick|[quick]}", "{icon_q|[Q]}{icon_act|[Act]}: Synthetic."),
    ],
)
def test_only_standalone_quick_icon_declares_the_keyword(
    region: str, icon: str, ability: str
) -> None:
    spelling = {"kw:quick": "クイック" if region == "jp" else "Quick"}
    assert classify(icon, region, spelling) == ({"kw:quick"}, set())
    assert classify(ability, region, spelling) == (set(), set())
    assert classify(f'"{icon}"', region, spelling) == (set(), {"kw:quick"})


def test_all_faces_and_region_owned_english_are_inspected(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(tmp_path, monkeypatch, "", back="【守護】") as (db, inputs, cid):
        with db.transaction():
            printing(db, cid, "jp")
            printing(db, cid, "en")
            for row in db.rows("face_current"):
                if row.values["region"] != "jp":
                    continue
                revision = db.select(
                    "face_revision",
                    db.columns("face_revision"),
                    where={"id": row.values["revision_id"]},
                )[0].values
                identifier = "en-" + str(revision["id"])
                db.insert(
                    "face_revision",
                    dict(revision)
                    | {
                        "id": identifier,
                        "region": "en",
                        "effect_unit_id": TextInterner(db).intern(
                            LocalizedText(lang="en", text="Storm.")
                        ),
                    },
                )
                db.insert(
                    "face_current",
                    dict(row.values) | {"region": "en", "revision_id": identifier},
                )
            report = project(db, inputs)
        positives = {
            (row.values["scope"], row.values["keyword_id"])
            for row in db.rows("mechanic_projection")
        }
        assert ("shared", "kw:ward") in positives
        assert ("en_override", "kw:storm") in positives
        assert ("en_override", "kw:ward") not in positives
        coverage = {
            row.values["scope"]: row.values for row in db.rows("card_mechanic_coverage")
        }
        assert coverage["en_override"]["has_complete_keyword_ids"] == Json(
            [
                record.id
                for record in sorted(
                    inputs.registry.records, key=lambda record: record.id
                )
            ]
        )
        assert coverage["en_override"]["complete_all"] is False
        assert report["self"] is not None


def test_token_definition_does_not_label_its_owner(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        "Synthetic text.",
        sections=("『Token』【守護】",),
    ) as (db, inputs, cid):
        with db.transaction():
            printing(db, cid, "jp")
            project(db, inputs)
        assert not db.rows("mechanic_projection")
        covered = db.rows("card_mechanic_coverage")[0].values[
            "has_complete_keyword_ids"
        ]
        assert isinstance(covered, Json)
        assert "kw:ward" not in array(covered.value)
        assert "kw:storm" in array(covered.value)


def test_section_quick_declaration_does_not_prove_absence(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(tmp_path, monkeypatch, "", sections=("{クイック}",)) as (db, inputs, cid):
        with db.transaction():
            printing(db, cid, "jp")
            project(db, inputs)
        assert not db.rows("mechanic_projection")
        covered = db.rows("card_mechanic_coverage")[0].values[
            "has_complete_keyword_ids"
        ]
        assert isinstance(covered, Json)
        assert "kw:quick" not in array(covered.value)


@pytest.mark.parametrize(
    ("section", "state"),
    [
        ("【守護】\nSynthetic continuation.", "present"),
        ("Synthetic text.\n【守護】", "unknown"),
    ],
    ids=["own-continuation", "unclear-owner"],
)
def test_section_declaration_follows_the_section_owner(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, section: str, state: str
) -> None:
    with case(tmp_path, monkeypatch, "Synthetic text.", sections=(section,)) as (
        db,
        inputs,
        cid,
    ):
        with db.transaction():
            printing(db, cid, "jp")
            project(db, inputs)
        held = {str(row.values["keyword_id"]) for row in db.rows("mechanic_projection")}
        covered = db.rows("card_mechanic_coverage")[0].values[
            "has_complete_keyword_ids"
        ]
        assert isinstance(covered, Json)
        assert ("kw:ward" in held) is (state == "present")
        # An unclear owner must not read as a confirmed absence.
        assert "kw:ward" not in array(covered.value) or state == "present"
        assert "kw:storm" in array(covered.value)
