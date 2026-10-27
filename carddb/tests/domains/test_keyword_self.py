"""Self keyword inspection is regional and never borrows a quoted or token ability."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Database, Json
from sve_carddb.core.json import array
from sve_carddb.domains import mechanics
from sve_carddb.domains.keyword_self import classify
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner

from .test_mechanics import case

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize(
    ("region", "text", "spelling", "present", "unknown"),
    [
        ("jp", "【守護】（守護の説明）", "守護", True, False),
        ("en", "Ward. (Synthetic reminder.)", "Ward", True, False),
        ("en", "Rush. Ward.", "Ward", True, False),
        ("jp", "これは【守護】を持つ。", "守護", True, False),
        ("jp", "それは【守護】を持つ。", "守護", False, True),
        ("jp", "これは【守護】を持つフォロワーを攻撃できる。", "守護", False, True),
        ("en", "While active, this has Ward.", "Ward", True, False),
        (
            "en",
            "Give this follower Ward until the end of the turn.",
            "Ward",
            True,
            False,
        ),
        ("en", "Give another follower Ward.", "Ward", False, True),
        ("jp", "それは「\n【守護】\n」を持つ。", "守護", False, True),
        ("en", 'Give another follower "\nWard.\n".', "Ward", False, True),
        ("jp", "「A」を持つ「B\n【守護】\n」", "守護", False, True),
        ("en", "While active, this card has Ward.", "Ward", True, False),
        ("en", "Synthetic Warden.", "Ward", False, False),
        ("en", "", "Ward", False, False),
    ],
)
def test_declarations_keep_ambiguous_uses_unknown(
    region: str, text: str, spelling: str, *, present: bool, unknown: bool
) -> None:
    found, uncertain = classify(text, region, {"kw:ward": spelling})
    assert ("kw:ward" in found) is present
    assert ("kw:ward" in uncertain) is unknown


def printing(db: Database, cid: str, region: str) -> None:
    card = db.select("card", db.columns("card"), where={"id": cid})[0].values
    db.insert(
        "printing",
        {
            "id": region + "-printing",
            "card_id": cid,
            "region": region,
            "card_no": region + "-001",
            "card_no_state": "official",
            "catalog_state": "official",
            "decklog_available": True,
            "decklog_verification": "unverified",
            "variant_key": "default",
            "home_set_id": card["home_set_id"],
            "rarity_raw": "-",
            "source_id": "source",
        },
    )


def test_all_faces_and_region_owned_english_are_inspected(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(tmp_path, monkeypatch, "", back="【守護】", first_batch="synthetic") as (
        db,
        inputs,
        settings,
        cid,
    ):
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
            report = mechanics.apply(db, inputs, settings)
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
        first_batch="synthetic",
    ) as (db, inputs, settings, cid):
        with db.transaction():
            printing(db, cid, "jp")
            mechanics.apply(db, inputs, settings)
        assert not db.rows("mechanic_projection")
        covered = db.rows("card_mechanic_coverage")[0].values[
            "has_complete_keyword_ids"
        ]
        assert isinstance(covered, Json)
        assert "kw:ward" not in array(covered.value)
        assert "kw:storm" in array(covered.value)
