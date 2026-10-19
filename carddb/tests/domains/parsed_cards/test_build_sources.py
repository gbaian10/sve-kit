"""Build inputs must be pinned and never fall back to omitted physical pages."""

import json
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical
from sve_carddb.domains.parsed_cards.generate import generate
from sve_carddb.domains.parsed_cards.sources import ParsedBatches
from sve_carddb.domains.parsed_cards.storage import load, write
from sve_carddb.ingest.archive.source_archive import ArchiveError, seal_batch
from sve_carddb.workflows.offline import Inputs, RegionalInput, source_batches

from ...ingest.test_source_archive import _put, _resource, _store
from .test_inputs import REVISION, _batch

if TYPE_CHECKING:
    from pathlib import Path


def test_unreviewed_sve_batch_fails_before_opening_raw(tmp_path: Path) -> None:
    first, number = _batch(tmp_path / "reviewed", "en")
    extra, _ = _batch(tmp_path / "unreviewed", "en", changed=True)
    catalog = generate(
        [("en", first)], {("en", number): "SYN"}, producer_revision=REVISION
    )
    for entry in extra.inventory.entries:
        (extra.root / entry.blob.path).unlink()
    with pytest.raises(ValueError, match="absent from reviewed"):
        ParsedBatches(catalog).batch(extra.root, extra.store_id, extra.batch_id)


@pytest.fixture
def recipe(tmp_path: Path) -> Inputs:
    en, en_number = _batch(tmp_path / "en", "en")
    jp, jp_number = _batch(tmp_path / "jp", "jp")
    catalog = generate(
        [("en", en), ("jp", jp)],
        {("en", en_number): "SYN", ("jp", jp_number): "SYN"},
        producer_revision=REVISION,
    )
    write(tmp_path / "repo/card-data", catalog)
    en.root.rename(tmp_path / "unmounted-en")
    jp.root.rename(tmp_path / "unmounted-jp")
    return Inputs(
        repo=tmp_path / "repo",
        archive=tmp_path / "absent",
        store_id=en.store_id,
        sources=(
            RegionalInput(
                region="en",
                card_batch=en.batch_id,
                image_batch=en.batch_id,
                parser_version="synthetic-en",
            ),
            RegionalInput(
                region="jp",
                card_batch=jp.batch_id,
                image_batch=jp.batch_id,
                parser_version="synthetic-jp",
            ),
        ),
        revision=REVISION,
        as_of="2026-10-01",
        data_version="preview-20261001T000000Z-0001",
        published_at="2026-10-01T00:00:00Z",
        feedback_url="https://example.invalid/feedback",
        grammar_version="synthetic-v1",
        normalizer_version="synthetic-v1",
    )


def test_default_build_resolves_original_batches_without_archive(
    recipe: Inputs,
) -> None:
    readers = source_batches(recipe)
    for pin in recipe.sources:
        sources = readers.batch(recipe.archive, recipe.store_id, pin.card_batch)
        assert all(
            sources.descriptor(v).provider == pin.region for v in sources.version_ids
        )


def test_build_rejects_wrong_regional_batch(recipe: Inputs) -> None:
    en, jp = recipe.sources
    recipe = recipe.model_copy(
        update={"sources": (en.model_copy(update={"card_batch": jp.card_batch}), jp)}
    )
    with pytest.raises(ValueError, match="lack the requested regional card batch"):
        source_batches(recipe)


def test_build_rejects_missing_reviewed_inputs(recipe: Inputs) -> None:
    (recipe.repo / "card-data/index.json").unlink()
    with pytest.raises(FileNotFoundError, match=r"index\.json"):
        source_batches(recipe)


def test_build_defaults_to_parsed_with_explicit_archive_opt_in(recipe: Inputs) -> None:
    value = recipe.model_dump(mode="json")
    assert isinstance(
        source_batches(Inputs.model_validate_json(canonical(value))), ParsedBatches
    )
    value["source_mode"] = "archive"
    archived = Inputs.model_validate_json(canonical(value))
    assert not isinstance(source_batches(archived), ParsedBatches)


def test_independent_image_batch_still_checks_archive(
    recipe: Inputs, tmp_path: Path
) -> None:
    store = _store(tmp_path / "image")
    raw = b"synthetic bitmap bytes"
    resource = _resource("https://example.invalid/image.png", "raw/image.png", raw)
    _put(store, resource, raw)
    sealed = seal_batch(store)
    readers = source_batches(recipe)
    frozen = readers.configured({store.store_id: store.root}, sealed.batch_id)
    version = next(iter(frozen.version_ids))
    assert frozen.read(version, parser_version="synthetic")[1] == raw
    assert readers.batch(store.root, store.store_id, sealed.batch_id) is frozen
    (store.root / frozen.entries[version].blob.path).write_bytes(b"tampered")
    with pytest.raises(ArchiveError, match="archive file hash or size mismatch"):
        source_batches(recipe).configured({store.store_id: store.root}, sealed.batch_id)


@pytest.mark.parametrize("error", ["id", "hash"])
def test_unreviewed_batch_identity_is_checked(
    recipe: Inputs, error: str, tmp_path: Path
) -> None:
    readers = source_batches(recipe)
    batch_id = "sha256:" + "f" * 64
    if error == "id":
        batch_id = "invalid"
        message = "Invalid configured source batch ID"
    else:
        path = tmp_path / "batches" / batch_id[7:] / "inventory.json"
        path.parent.mkdir(parents=True)
        path.write_bytes(b"{}")
        message = "Unparsed inventory hash mismatch"
    with pytest.raises(ValueError, match=message):
        readers.batch(tmp_path, "synthetic", batch_id)


def test_parsed_batch_rejects_a_foreign_version(recipe: Inputs) -> None:
    readers = source_batches(recipe)
    en, jp = (
        readers.batch(recipe.archive, recipe.store_id, p.card_batch)
        for p in recipe.sources
    )
    foreign = next(iter(jp.version_ids))
    for action in (
        lambda: en.descriptor(foreign),
        lambda: en.metadata(foreign, parser_version="synthetic"),
    ):
        with pytest.raises(ArchiveError, match="absent from pinned batch"):
            action()


def test_build_trusts_admitted_data_while_validate_rejects_changed_pins(
    recipe: Inputs,
) -> None:
    path = recipe.repo / "card-data/index.json"
    index = json.loads(path.read_bytes())
    index["files"][0]["sha256"] = "sha256:" + "0" * 64
    path.write_text(json.dumps(index, ensure_ascii=False, indent=2) + "\n")
    assert isinstance(source_batches(recipe), ParsedBatches)
    with pytest.raises(ValueError, match="Parsed pair size/hash mismatch"):
        load(path.parent)
