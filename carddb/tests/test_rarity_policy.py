"""The approved 27 labels are policy facts, independent of processing adoption."""

import pytest

from sve_carddb.build import CompiledSchema, create_database
from sve_carddb.build.t0 import compile_t0
from sve_carddb.routes.defaults import GeneralEvidence, select_defaults
from sve_carddb.routes.rarity_policy import APPROVED_GENERAL_RARITIES, RarityWhitelist

from .routes_fixtures import base, printing


def test_rarity_rejects_unsupported_region() -> None:
    with pytest.raises(ValueError, match=r"^Unsupported rarity region$"):
        APPROVED_GENERAL_RARITIES.classify("other", "BR")


def test_database_rejects_nontext_rarity_before_classification(
    schema: CompiledSchema,
) -> None:
    with create_database(schema) as db:
        base(db)
        with (
            db.transaction(),
            pytest.raises(TypeError, match=r"^Expected text for rarity_raw$"),
        ):
            printing(db, "p:synthetic-malformed", "SYN-99", changes={"rarity_raw": 42})


@pytest.mark.parametrize(
    ("region", "raw", "expected"),
    [
        *[("jp", raw, True) for raw in ("BR", "SR", "GR", "LG")],
        *[("en", raw, True) for raw in ("Bronze", "Silver", "Gold", "Legendary")],
        *[
            ("jp", raw, False)
            for raw in (
                "BR・プレミアム",
                "SR・プレミアム",
                "GR・プレミアム",
                "プレミアム",
                "SL",
                "SP",
                "SSP",
                "UR",
                "PR",
                "-",
            )
        ],
        *[
            ("en", raw, False)
            for raw in (
                "Bronze / Premium",
                "Silver / Premium",
                "Gold / Premium",
                "Premium",
                "Super Legendary",
                "Special",
                "Super Special",
                "Ultimate",
                "Promo",
                "-",
            )
        ],
        ("jp", "unknown・プレミアム", False),
        ("en", "Unknown / Premium", False),
        ("jp", "unknown", None),
        ("en", None, None),
    ],
)
def test_approved_raw_labels_and_unknowns(
    region: str, raw: str | None, expected: bool | None
) -> None:
    assert APPROVED_GENERAL_RARITIES.classify(region, raw) is expected


@pytest.mark.parametrize(
    ("raw", "method"),
    [
        ("BR", "candidate_general"),
        ("PR", "fallback"),
        ("SL", "fallback"),
        ("-", "fallback"),
        ("BR・プレミアム", "fallback"),
        ("unknown", "fallback"),
    ],
)
def test_selector_uses_whitelist_without_fabricating_frame_or_stamp(
    raw: str, method: str, schema: CompiledSchema
) -> None:
    with create_database(schema) as db:
        base(db)
        with db.transaction():
            printing(db, "a", "A", changes={"rarity_raw": raw})
        result = select_defaults(db, rarity_whitelist=APPROVED_GENERAL_RARITIES)[0]
        assert result.method == method
        assert result.printing_id == "a"
        with pytest.raises(ValueError, match="unadopted processing"):
            select_defaults(
                db,
                rarity_whitelist=APPROVED_GENERAL_RARITIES,
                general_evidence={"a": GeneralEvidence(True, True, False)},
            )


def test_caller_cannot_extend_approved_rarity_policy() -> None:
    with pytest.raises(ValueError, match="user-approved"):
        RarityWhitelist(
            jp=("BR", "GR", "LG", "PR", "SR"), en=APPROVED_GENERAL_RARITIES.en
        )
    assert APPROVED_GENERAL_RARITIES.configuration()["general_rarity_policy_hash"]


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_t0()
