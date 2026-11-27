"""Native ability headers are distinct from quoted abilities and body references."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json
from sve_carddb.domains.ability_types import english_author_headers, parse
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner

from .test_mechanics import case, printing, project

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize(
    ("region", "text", "expected"),
    [
        (
            "en",
            "{icon_act|[act]} Fuse a synthetic card: Place a fusion counter.",
            {"act", "fusion"},
        ),
        ("en", "On Evolve/Strike - Synthetic.", {"on_evolve", "on_attack"}),
        ("en", "{icon_q|[q]}{icon_act|[act]} Synthetic.", {"act"}),
        ("en", "On Super-Evolve - Synthetic.", {"on_super_evolve"}),
        (
            "jp",
            "{ファンファーレ}それは「\n【攻撃時】Synthetic.\n」を持つ。",
            {"fanfare"},
        ),
    ],
)
def test_native_headers(region: str, text: str, expected: set[str]) -> None:
    assert parse(text, region) == (expected, True)


def test_unclosed_quote_does_not_claim_complete() -> None:
    assert parse('Give another follower "\nOn Attack - Synthetic.', "en") == (
        set(),
        False,
    )


def test_english_author_headers_include_the_quick_activation_modifier() -> None:
    assert english_author_headers("{icon_q|[q]}{icon_act|[act]} Synthetic.") == {
        1: {"act", "quick"},
    }


def test_regions_faces_and_token_sections(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        "{ファンファーレ}Synthetic.",
        back="{起動}Synthetic.",
        sections=("【超進化時】Token.",),
    ) as (db, inputs, cid):
        with db.transaction():
            printing(db, cid, "jp")
            printing(db, cid, "en")
            revision = db.select(
                "face_revision",
                db.columns("face_revision"),
                where={"id": "link-revision"},
            )[0].values
            db.insert(
                "face_revision",
                dict(revision)
                | {
                    "id": "english-revision",
                    "region": "en",
                    "effect_unit_id": TextInterner(db).intern(
                        LocalizedText(
                            lang="en", text="{icon_lastword|[lastwords]} Synthetic."
                        )
                    ),
                },
            )
            db.insert(
                "face_current",
                {
                    "face_id": revision["face_id"],
                    "region": "en",
                    "revision_id": "english-revision",
                    "basis": "latest_observed_no_errata",
                },
            )
            project(db, inputs)
        coverage = {
            row.values["scope"]: row.values for row in db.rows("card_mechanic_coverage")
        }
        assert coverage["shared"]["ability_types"] == Json(["act", "fanfare"])
        assert coverage["shared"]["ability_types_complete"] is True
        assert coverage["en_override"]["ability_types"] == Json(["last_words"])
        assert coverage["en_override"]["ability_types_complete"] is False
