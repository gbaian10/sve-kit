"""Native ability headers are distinct from quoted abilities and body references."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json
from sve_carddb.domains import mechanics
from sve_carddb.domains.ability_types import parse
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner

from .test_keyword_self import printing
from .test_mechanics import case

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize(
    ("region", "text", "expected"),
    [
        ("jp", "{クイック}\nSynthetic.", {"quick"}),
        (
            "jp",
            "{ファンファーレ}Synthetic.\n【攻撃時】Other.",
            {"fanfare", "on_attack"},
        ),
        ("jp", "《起動》Synthetic.", {"act"}),
        ("jp", "{UB}{起動}Synthetic.", {"act"}),
        ("jp", "{アドバンス起動}{コスト4}：Synthetic.", {"advanced_act"}),
        ("jp", "【超進化時】Synthetic.", {"on_super_evolve"}),
        ("jp", "【進化時】Synthetic.", {"on_evolve"}),
        ("jp", "{起動}【融合】Synthetic：Other.", {"act", "fusion"}),
        ("en", "{icon_adv|[adv]} Synthetic.", {"advanced_act"}),
        (
            "en",
            "{icon_quick|[quick]}\n{icon_fanfare|[fanfare]} Synthetic.",
            {"quick", "fanfare"},
        ),
        ("en", "{icon_lastword|[lastwords]} Synthetic.", {"last_words"}),
        ("en", "On Super-Evolve - Synthetic.", {"on_super_evolve"}),
        ("en", "Strike: Draw a card, then discard a card.", {"on_attack"}),
        (
            "en",
            "On Evolve - Synthetic.\nOn Attack - Other.",
            {"on_evolve", "on_attack"},
        ),
        (
            "en",
            "{icon_act|[act]} Fuse a synthetic card: Place a fusion counter.",
            {"act", "fusion"},
        ),
        ("en", "Whenever you fuse a card, synthetic.", set()),
        ("en", 'Give another follower "\nOn Attack - Synthetic.\n".', set()),
        (
            "jp",
            "{ファンファーレ}それは「\n【攻撃時】Synthetic.\n」を持つ。",
            {"fanfare"},
        ),
        (
            "en",
            "{icon_act|[act]} Synthetic.\n{icon_fanfare|[fanfare]} Fuse a card: Other.",
            {"act", "fanfare"},
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


def test_regions_faces_and_token_sections(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        "{ファンファーレ}Synthetic.",
        back="{起動}Synthetic.",
        sections=("【超進化時】Token.",),
        first_batch="synthetic",
    ) as (db, inputs, settings, cid):
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
            mechanics.apply(db, inputs, settings)
        coverage = {
            row.values["scope"]: row.values for row in db.rows("card_mechanic_coverage")
        }
        assert coverage["shared"]["ability_types"] == Json(["act", "fanfare"])
        assert coverage["shared"]["ability_types_complete"] is True
        assert coverage["en_override"]["ability_types"] == Json(["last_words"])
        assert coverage["en_override"]["ability_types_complete"] is False
