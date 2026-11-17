"""Printed mechanisms retain native ownership, regional coverage and numeric thresholds."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json
from sve_carddb.domains import mechanics
from sve_carddb.domains.keyword_self import CurrentFields
from sve_carddb.domains.printed_mechanisms import apply, parse

from .test_keyword_self import printing
from .test_mechanics import case

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize(
    ("region", "text", "expected"),
    [
        ("jp", "【土の秘術_2】Synthetic.", {"earth_rite"}),
        ("en", "Earth Rite (2): Synthetic.", {"earth_rite"}),
        (
            "en",
            "If Overflow is active for you, synthetic. If Sanguine is active for you, other.",
            {"overflow", "sanguine"},
        ),
        ("jp", "それは「\n【ツインドライブ】\n」を持つ。", set()),
        ("en", "Pay {icon_carrot|[feed]} costs using an Evolution Point.", set()),
    ],
)
def test_native_mechanisms(region: str, text: str, expected: set[str]) -> None:
    assert parse(text, region).present == expected


def test_thresholds_and_unclosed_quotes() -> None:
    parsed = parse('SC (5): Synthetic. Spellchain (10): Other.\nGrant "', "en")
    assert parsed.thresholds == {"spellchain": {5, 10}}
    assert not parsed.balanced


def test_current_faces_and_supplement_owners(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(
        tmp_path, monkeypatch, "【スペルチェイン_5】Synthetic.", first_batch="synthetic"
    ) as (db, inputs, settings, cid):
        with db.transaction():
            printing(db, cid, "jp")
            printing(db, cid, "en")
            mechanics.apply(db, inputs, settings)
            apply(
                db,
                {
                    (cid, "jp"): CurrentFields(
                        ["【スペルチェイン_5】Synthetic."],
                        [
                            "『Synthetic』{ウィッチ}【スタック】",
                            "これは【覚醒】状態でないなら、Synthetic.",
                        ],
                    ),
                    (cid, "en"): CurrentFields(
                        ["Synthetic."],
                        [
                            "Ward. Twin Drive.\n{icon_fanfare|[fanfare]} Synthetic.",
                            "Unclassified supplement: Stack.",
                        ],
                        True,
                    ),
                },
            )
        rows = {
            row.values["scope"]: row.values for row in db.rows("card_mechanic_coverage")
        }
        assert rows["shared"]["printed_mechanisms"] == Json(
            [
                {"code": "overflow", "thresholds": []},
                {"code": "spellchain", "thresholds": [5]},
            ]
        )
        assert rows["shared"]["checked_mechanisms"] != Json([])
        # A missing English current face never borrows Japanese negative coverage.
        assert rows["en_override"]["printed_mechanisms"] == Json(
            [{"code": "twin_drive", "thresholds": []}]
        )
        assert rows["en_override"]["checked_mechanisms"] == Json([])


def test_missing_faces_keep_positive_continuations_without_claiming_absence(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(tmp_path, monkeypatch, "Synthetic.", first_batch="synthetic") as (
        db,
        inputs,
        settings,
        cid,
    ):
        with db.transaction():
            printing(db, cid, "jp")
            mechanics.apply(db, inputs, settings)
            apply(
                db,
                {
                    (cid, "jp"): CurrentFields(
                        [], ["【ツインドライブ】", "Unclassified 【スタック】"], True
                    )
                },
            )
        row = db.rows("card_mechanic_coverage")[0].values
        assert row["printed_mechanisms"] == Json(
            [{"code": "twin_drive", "thresholds": []}]
        )
        assert row["checked_mechanisms"] == Json([])
