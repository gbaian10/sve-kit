"""Prepared inputs may be shared only when every consumer gets isolated state."""

from types import MappingProxyType
from typing import TYPE_CHECKING

import pytest

from sve_carddb.domains.parsed_cards.sources import ParsedBatches
from sve_carddb.domains.physical_sources import page

from ..support.author_current_fixtures import CurrentSourceInputs
from ..support.prepared_values import freeze, restore, shared_preparation

if TYPE_CHECKING:
    from pathlib import Path


def test_restored_graphs_keep_aliases_without_sharing_nested_mutations() -> None:
    values = ["original"]
    content = freeze({"first": values, "second": MappingProxyType({"values": values})})
    first = restore(content, dict)
    second = restore(content, dict)
    first["first"].append("changed")
    assert first["second"]["values"] == ["original", "changed"]
    assert second["first"] == second["second"]["values"] == ["original"]
    with pytest.raises(TypeError):
        first["second"]["other"] = []


def test_failed_preparation_can_be_retried_without_a_partial_cached_result(
    tmp_path: Path,
) -> None:
    cache = tmp_path / "prepared.pickle"

    def fail() -> bytes:
        raise ValueError("Preparation failed")

    with pytest.raises(ValueError, match="Preparation failed"):
        shared_preparation(cache, fail)
    assert not cache.exists()
    content = shared_preparation(cache, lambda: freeze({"selection": ["first"]}))
    first = restore(content, dict)
    first["selection"].append("changed")
    second = restore(shared_preparation(cache, fail), dict)
    assert second == {"selection": ["first"]}


def test_interrupted_write_does_not_publish_a_partial_preparation(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    cache = tmp_path / "prepared.pickle"
    write = type(cache).write_bytes

    def interrupted(path: Path, data: bytes) -> int:
        write(path, data[:1])
        raise OSError("Interrupted cache write")

    with monkeypatch.context() as patch:
        patch.setattr(type(cache), "write_bytes", interrupted)
        with pytest.raises(OSError, match="Interrupted cache write"):
            shared_preparation(cache, lambda: freeze({"selection": ["first"]}))
    assert not cache.exists()
    content = shared_preparation(cache, lambda: freeze({"selection": ["first"]}))
    assert restore(content, dict) == {"selection": ["first"]}


def test_fixed_catalog_reuse_keeps_physical_page_caches_private(
    author_source_preparation: bytes, tmp_path: Path
) -> None:
    for parser in ("first-v1", "second-v2"):
        inputs = restore(author_source_preparation, CurrentSourceInputs)
        batches = ParsedBatches(inputs.catalog)
        frozen = batches.batch(tmp_path / "sources", "public-fixture", inputs.batch_id)
        physical = page(
            frozen,
            frozen.current_versions[0].source_version_id,
            region="jp",
            parser_version=parser,
        )
        assert physical.text.get().source.parser_version == parser
