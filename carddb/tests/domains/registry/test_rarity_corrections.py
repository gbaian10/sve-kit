"""Rarity corrections replace a contradicted official label before normalization."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.domains.registry.rarity import populate as populate_rarities
from sve_carddb.domains.registry.rarity_corrections import (
    FILE,
    load_rarity_corrections,
    populate_rarity_corrections,
)
from tests.support.schema_helpers import compile_t0

from ...support.build_db_fixtures import seed

if TYPE_CHECKING:
    from pathlib import Path

EVIDENCE = (
    "  evidence:\n"
    "    - kind: card_image\n"
    f"      sha256: sha256:{'a' * 64}\n"
    "      image_src: /cardlist/PR/pr-659.png\n"
    "      region: {evidence_region}\n"
    "      locator: Bottom-left card number line\n"
)


def record(
    *,
    printing: str = "printing",
    expected: str = "PR",
    corrected: str = "-",
    region: str = "jp",
    evidence_region: str = "jp",
) -> str:
    return (
        f"- printing_id: {printing}\n"
        f"  region: {region}\n"
        "  card_no: TEST-001Ⓢa\n"
        f'  expected_raw_value: "{expected}"\n'
        f'  corrected_value: "{corrected}"\n'
        "  reason: The printed card shows no rarity mark.\n"
        + EVIDENCE.format(evidence_region=evidence_region)
    )


def authored(tmp_path: Path, records: str) -> Path:
    path = tmp_path / FILE
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        "format: 1\nkind: printing_rarity_corrections\nrecords:\n" + records
    )
    return tmp_path


@pytest.mark.parametrize(
    ("staged", "code", "diagnostics"),
    [
        ("PR", None, []),
        ("-", None, [{"reason": "correction_upstream_fixed", "rarity_raw": "-"}]),
    ],
)
def test_corrects_the_raw_label_or_reports_an_upstream_fix(
    tmp_path: Path, staged: str, code: str | None, diagnostics: list[dict[str, str]]
) -> None:
    records = load_rarity_corrections(authored(tmp_path, record()))
    with create_database(compile_t0()) as db:
        seed(db)
        with db.transaction():
            db.update("printing", {"id": "printing"}, {"rarity_raw": staged})
            reported = populate_rarity_corrections(db, records)
            populate_rarities(db)
        printing = db.rows("printing")[0].values
    assert (printing["rarity_raw"], printing["rarity_code"]) == ("-", code)
    assert printing["premium"] is None
    assert [
        {"reason": item["reason"], "rarity_raw": item["rarity_raw"]}
        for item in reported
    ] == diagnostics


def test_rejects_a_label_that_matches_neither_side(tmp_path: Path) -> None:
    records = load_rarity_corrections(authored(tmp_path, record()))
    with create_database(compile_t0()) as db:
        seed(db)
        with db.transaction():
            db.update("printing", {"id": "printing"}, {"rarity_raw": "SR"})
            with pytest.raises(ValueError, match="conflict"):
                populate_rarity_corrections(db, records)


@pytest.mark.parametrize(
    ("row", "match"),
    [
        (record(printing="missing"), "Unknown rarity correction"),
        (
            record(expected="Promo", region="en", evidence_region="en"),
            "identity mismatch",
        ),
    ],
    ids=["unknown", "mismatch"],
)
def test_rejects_unknown_or_mismatched_printings(
    tmp_path: Path, row: str, match: str
) -> None:
    records = load_rarity_corrections(authored(tmp_path, row))
    with create_database(compile_t0()) as db:
        seed(db)
        with db.transaction(), pytest.raises(ValueError, match=match):
            populate_rarity_corrections(db, records)


@pytest.mark.parametrize(
    "row",
    [
        record(corrected="PR"),
        record(corrected="Rare"),
        record(evidence_region="en"),
        record().replace(EVIDENCE.format(evidence_region="jp"), "  evidence: []\n"),
    ],
    ids=["unchanged", "unknown-label", "evidence-region", "no-evidence"],
)
def test_rejects_invalid_records(tmp_path: Path, row: str) -> None:
    with pytest.raises(ValueError, match="Invalid rarity correction"):
        load_rarity_corrections(authored(tmp_path, row))


def test_rejects_duplicate_printings(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="Duplicate"):
        load_rarity_corrections(authored(tmp_path, record() + record()))
