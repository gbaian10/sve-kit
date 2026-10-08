"""Repository vocabulary and language values load without any private sources."""

from pathlib import Path

from sve_carddb.catalog.adoption_loader import load_adoptions


def test_repository_catalog_adoptions_load_without_private_sources() -> None:
    root = Path(__file__).resolve().parents[2] / "authored"
    snapshot = load_adoptions(root, entry="catalog-adoptions")
    records = snapshot.current_records()
    assert sum(r.kind == "language_adoption" for r in records) == 3
    assert sum(r.kind == "vocabulary_adoption" for r in records) == 149
    assert {r.data.subject.code for r in records if r.kind == "language_adoption"} == {
        "ja",
        "en",
        "zh-Hant",
    }
