"""Formal main ancestry, independent uses and immutable in-memory replay evidence."""

import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_semantics import registry
from sve_carddb.template_translations import loader, semantic_replay
from sve_carddb.template_translations.files import Files
from sve_carddb.template_translations.sources import TemplateSources

from .adoption_fixtures import commit, git
from .test_template_semantic_guards import (
    fresh,
    generated,
    intake_case,
    policy_git,
    recognition_term_source,
)

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.template_translations.replay_models import InventoryV2

__all__ = ("generated", "intake_case", "policy_git", "recognition_term_source")


@pytest.mark.parametrize("position", [0, 1, 2])
@pytest.mark.parametrize("kind", ["binding", "recipe"])
@pytest.mark.parametrize("history", ["sidebranch", "after_main", "missing"])
def test_every_producer_pin_requires_main_ancestry(
    generated: tuple[TemplateSources, InventoryV2],
    tmp_path: Path,
    kind: str,
    history: str,
    position: int,
) -> None:
    original, inventory = generated
    root = tmp_path / "history"
    shutil.copytree(original.repository.root, root)
    main = original.main_revision
    if history == "sidebranch":
        (root / "main-note").write_text("Synthetic main continuation\n")
        main = commit(root)
        git(root, "checkout", "--detach", original.main_revision)
    (root / "producer-note").write_text("Synthetic unmerged producer\n")
    unmerged = commit(root)
    if history == "missing":
        unmerged = "a" * 40
    pins, context = inventory.recipes, inventory.replay_context
    if kind == "binding":
        context = context.model_copy(
            update={
                "semantic_bindings": tuple(
                    b.model_copy(update={"revision": unmerged}) if i == position else b
                    for i, b in enumerate(context.semantic_bindings)
                )
            }
        )
    else:
        pins = tuple(
            p.model_copy(update={"code_revision": unmerged}) if i == position else p
            for i, p in enumerate(pins)
        )
    consumer = TemplateSources(
        PinnedRepository(root),
        original.stores,
        main_revision=main,
        legacy_bytes=original.legacy_bytes,
        proposals=original.proposals,
    )
    expected = (
        r"^Recognition immutable Git history is unavailable$"
        if history == "missing"
        else r"^Recognition matcher commit must be reachable from pinned main$"
    )
    with pytest.raises(ValueError, match=expected):
        consumer.reconstruct_v2(pins, context)
    assert consumer.replay_groups == {}
    with pytest.raises(ValueError, match=expected):
        semantic_replay.reconstruct(consumer, pins, context)


def test_cache_hit_cannot_bypass_a_changed_pinned_main(
    generated: tuple[TemplateSources, InventoryV2],
) -> None:
    original, inventory = generated
    consumer = fresh(original)
    consumer.reconstruct_v2(inventory.recipes, inventory.replay_context)
    assert consumer.replay_groups
    consumer.main_revision = "a" * 40
    # An already computed group cannot replace availability of its main history.
    with pytest.raises(
        ValueError, match=r"^Recognition immutable Git history is unavailable$"
    ):
        consumer.reconstruct_v2(inventory.recipes, inventory.replay_context)


def test_effect_source_recipes_cannot_diverge_from_otherwise_valid_source_pins(
    generated: tuple[TemplateSources, InventoryV2],
) -> None:
    sources, inventory = generated
    pins = []
    for pin in inventory.recipes:
        altered = pin
        if pin.id == semantic_replay.PARAMETERS:
            config = {**pin.config, "source_recipes": []}
            altered = pin.model_copy(
                update={"config": config, "config_hash": digest(canonical(config))}
            )
        pins.append(altered)
    checked = registry.verify(
        sources.repository, inventory.replay_context.semantic_bindings, flavor=False
    )
    registry.verify_recipes(sources.repository, tuple(pins), checked)
    with pytest.raises(
        ValueError,
        match=r"^Formal template source recipes differ from the parameter recipe$",
    ):
        semantic_replay.effect(sources, tuple(pins), checked)


def test_reconstruct_rejects_actual_use_omission_at_the_independent_gate(
    generated: tuple[TemplateSources, InventoryV2],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    sources, inventory = generated
    original = semantic_replay.reconstruct
    monkeypatch.setattr(
        semantic_replay,
        "reconstruct",
        lambda consumer, pins, context: replace(
            original(consumer, pins, context), source_uses=()
        ),
    )
    with pytest.raises(
        ValueError, match=r"^Replay independent source use closure mismatch$"
    ):
        fresh(sources).reconstruct_v2(inventory.recipes, inventory.replay_context)


def test_immutable_git_inventory_cannot_be_replaced_in_memory_with_its_replay(
    generated: tuple[TemplateSources, InventoryV2],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    sources, inventory = generated
    root = tmp_path / "immutable-inventory"
    shutil.copytree(sources.repository.root, root)
    name = "translations/template-sources/001.yaml"
    target = root / "authored" / name
    target.parent.mkdir(parents=True, exist_ok=True)
    raw = canonical(inventory.model_dump(mode="json"))
    target.write_bytes(raw)
    adoption = commit(root)
    consumer = TemplateSources(
        PinnedRepository(root),
        sources.stores,
        main_revision=adoption,
        legacy_bytes=sources.legacy_bytes,
        proposals=sources.proposals,
    )
    actual = consumer.reconstruct_v2(inventory.recipes, inventory.replay_context)
    assert len(actual.entries) > 1
    removed = actual.entries[0].entry.id
    altered = inventory.model_copy(
        update={"entries": tuple(e for e in inventory.entries if e.id != removed)}
    )
    replay = replace(
        actual, entries=tuple(e for e in actual.entries if e.entry.id != removed)
    )
    monkeypatch.setattr(TemplateSources, "reconstruct_v2", lambda *_: replay)
    fake_raw = canonical(altered.model_dump(mode="json"))
    files = Files(adoption, b"{}", ((name, fake_raw, fake_raw),))
    # Both the declared inventory and its matching computed result are corrupted;
    # only the separate Git reread retains the original immutable entry.
    with pytest.raises(
        ValueError, match=r"^Template immutable inventory differs from in-memory input$"
    ):
        loader._inventories(files, consumer)
