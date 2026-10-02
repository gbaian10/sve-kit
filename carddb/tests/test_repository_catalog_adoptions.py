"""The repository's real adoption envelopes must pass the structural loader."""

from pathlib import Path

from sve_carddb.catalog.adoption_loader import load_adoptions


def test_repository_catalog_adoptions_load_without_private_sources() -> None:
    root = Path(__file__).resolve().parents[2] / "authored"
    snapshot = load_adoptions(root, entry="catalog-adoptions")
    shards = {shard.path: shard for shard in snapshot.shards}
    for path in (
        "catalog-adoptions/languages/shared/001.yaml",
        "catalog-adoptions/vocabulary/shared/001.yaml",
    ):
        assert path in shards
        assert shards[path].envelope().records
    assert snapshot.effective()
