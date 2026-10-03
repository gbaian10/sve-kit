"""Small complete, distinct historical groups exercise actual aggregate F1 cost."""

import json
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_models import Batch
from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.manifest import Kind
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.template_semantics.generation import generate
from sve_carddb.template_semantics.session import replay
from sve_carddb.template_translations.replay_models import (
    FlavorReplayInputs,
    InventoryV2,
)
from sve_carddb.template_translations.sources import TemplateSources

from .adoption_fixtures import commit
from .template_flavor_fixtures import flavor_case
from .template_intake_fixtures import intake_case, policy_git, recognition_term_source
from .test_registry_preview_archive import RAW
from .test_source_archive import _put, _resource, _store
from .test_template_semantic_guards import fresh, generated

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from .template_flavor_fixtures import Case

__all__ = (
    "flavor_case",
    "generated",
    "intake_case",
    "policy_git",
    "recognition_term_source",
)


@pytest.mark.parametrize("count", [1, 2, 4, 8])
def test_distinct_complete_groups_retain_all_outputs_and_audit_all_uses(  # ruff: ignore[too-many-locals] -- each complete synthetic group has independently sealed inputs
    flavor_case: Case,
    tmp_path: Path,
    count: int,
    record_property: Callable[[str, object], None],
) -> None:
    root = flavor_case.fork(tmp_path / "growth")
    producer = commit(root)
    stores = {}
    inventories = []
    for group in range(count):
        name = f"synthetic-group-{group}"
        store = replace(_store(tmp_path / name), store_id=name)
        raw = RAW.replace(
            b'<div class="speech"></div>',
            f'<div class="speech">Synthetic paragraph {group}</div>'.encode(),
        )
        number = f"SYN-GROUP-{group:03}"
        _put(
            store,
            _resource(card_url(number), f"raw/{number}.html", raw, Kind.CARD),
            raw,
        )
        batch = seal_batch(store)
        stores[name] = store.root
        sources = TemplateSources(
            PinnedRepository(root), stores, main_revision=producer, legacy_bytes=b""
        )
        inputs = FlavorReplayInputs(
            kind="flavor",
            source_batch=Batch(store_id=name, batch_id=batch.batch_id),
            identity_basis=None,
            identity_batches=(),
        )
        pins, context, result = generate(sources, producer, inputs)
        inventories.append(
            InventoryV2(
                template_source_format=2,
                kind="template_source_inventory",
                recipes=pins,
                replay_context=context,
                entries=tuple(m.entry for m in result.entries),
            )
        )
    consumer = TemplateSources(
        PinnedRepository(root), stores, main_revision=producer, legacy_bytes=b""
    )
    report = replay(tuple(inventories), consumer, producer, tmp_path / "complete-f1")
    assert report["groups"] == report["replay_count"] == count
    assert report["entries"] == count
    assert len(consumer.replay_groups) == count
    record_property("growth_metrics", json.dumps(report, sort_keys=True))


def test_distinct_effect_and_flavor_producers_share_one_later_host_f1(  # ruff: ignore[too-many-locals] -- independent historical producers and the actual host are validated in one complete bundle
    generated: tuple[TemplateSources, InventoryV2], tmp_path: Path
) -> None:
    import shutil  # ruff: ignore[import-outside-top-level] -- clone the shared immutable fixture before creating later producer commits

    from sve_carddb.build_inputs import InputRecord  # ruff: ignore[import-outside-top-level] -- validate the published wire evidence independently

    old_sources, effect = generated
    root = tmp_path / "distinct-producers"
    shutil.copytree(old_sources.repository.root, root)
    note = root / "engineering-note.txt"
    note.write_text("Second producer; frozen calculation files are unchanged.\n")
    second = commit(root)
    store = replace(_store(tmp_path / "flavor-store"), store_id="flavor-only")
    raw = RAW.replace(
        b'<div class="speech"></div>', b'<div class="speech">Synthetic flavor</div>'
    )
    _put(
        store,
        _resource(card_url("SYN-PRODUCER"), "raw/card.html", raw, Kind.CARD),
        raw,
    )
    batch = seal_batch(store)
    stores = {**old_sources.stores, store.store_id: store.root}
    producer_sources = TemplateSources(
        PinnedRepository(root), stores, main_revision=second, legacy_bytes=b""
    )
    pins, context, result = generate(
        producer_sources,
        second,
        FlavorReplayInputs(
            kind="flavor",
            source_batch=Batch(store_id=store.store_id, batch_id=batch.batch_id),
            identity_basis=None,
            identity_batches=(),
        ),
    )
    flavor = InventoryV2(
        template_source_format=2,
        kind="template_source_inventory",
        recipes=pins,
        replay_context=context,
        entries=tuple(m.entry for m in result.entries),
    )
    note.write_text("Third host; historical producer pins remain unchanged.\n")
    host = commit(root)
    sources = TemplateSources(
        PinnedRepository(root),
        stores,
        main_revision=host,
        legacy_bytes=old_sources.legacy_bytes,
        proposals=old_sources.proposals,
    )
    destination = tmp_path / "one-f1"
    report = replay((effect, flavor), sources, host, destination)
    assert report["groups"] == report["replay_count"] == 2
    evidence = InputRecord.model_validate_json(
        (destination / "inputs.json").read_bytes()
    )
    assert evidence.context.program_revision == host
    producers = {
        pin.code_revision for inventory in (effect, flavor) for pin in inventory.recipes
    }
    assert producers == {old_sources.main_revision, second}
    assert host not in producers
    for inventory in (effect, flavor):
        actual = fresh(sources).reconstruct_v2(
            inventory.recipes, inventory.replay_context
        )
        assert actual.semantic_output is not None
        assert (
            actual.semantic_output.manifest == inventory.replay_context.expected_outputs
        )
