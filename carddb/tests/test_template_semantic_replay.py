"""A complete synthetic effect batch independently generates and consumes its v2 baseline."""

from typing import TYPE_CHECKING

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import canonical, object_value, parse
from sve_carddb.template_semantics.generation import generate
from sve_carddb.template_translations.loader import load_templates
from sve_carddb.template_translations.replay_models import EffectInputs, InventoryV2
from sve_carddb.template_translations.sources import TemplateSources

from .adoption_fixtures import commit
from .template_intake_fixtures import (
    INVENTORY,
    intake_case,
    policy_git,
    recognition_term_source,
    write,
)

if TYPE_CHECKING:
    from pathlib import Path

    from .template_intake_fixtures import Case

__all__ = ("intake_case", "policy_git", "recognition_term_source")


def test_complete_synthetic_effect_v2_uses_the_same_roles_and_raw_bytes(
    intake_case: Case, tmp_path: Path
) -> None:
    root = intake_case.fork(tmp_path / "effect-v2")
    producer = commit(root)
    sources = TemplateSources(
        PinnedRepository(root),
        intake_case.sources().stores,
        main_revision=producer,
        legacy_bytes=intake_case.source.legacy,
        proposals=intake_case.source.proposals,
    )
    config = {
        k: v
        for k, v in intake_case.source.recipe.config.items()
        if k in {"recognition_policy", "source_batch", "references"}
    }
    pins, context, generated = generate(
        sources, producer, EffectInputs(kind="effect"), effect_config=config
    )
    assert generated.semantic_output is not None
    inventory = InventoryV2(
        template_source_format=2,
        kind="template_source_inventory",
        recipes=pins,
        replay_context=context,
        entries=tuple(
            m.entry for m in sorted(generated.entries, key=lambda m: m.entry.id)
        ),
    )
    reread = InventoryV2.model_validate_json(
        canonical(inventory.model_dump(mode="json"))
    )
    independent = TemplateSources(
        PinnedRepository(root),
        sources.stores,
        main_revision=producer,
        legacy_bytes=sources.legacy_bytes,
        proposals=sources.proposals,
    )
    actual = independent.reconstruct_v2(reread.recipes, reread.replay_context)
    assert actual.semantic_output == generated.semantic_output
    assert sorted(
        (m.entry.id, m.roles, m.pending, m.normalized) for m in actual.entries
    ) == sorted(
        (m.entry.id, m.roles, m.pending, m.normalized)
        for m in intake_case.replay.entries
    )
    assert (
        object_value(object_value(parse(actual.provenance))["actual_outputs"])["root"]
        == context.expected_outputs.root
    )


def test_complete_v2_inventory_can_load_all_adopted_synthetic_definitions(
    intake_case: Case, tmp_path: Path
) -> None:
    import copy  # ruff: ignore[import-outside-top-level] -- immutable module fixture is copied before mutation

    root = intake_case.fork(tmp_path / "formal-v2")
    producer = commit(root)
    sources = TemplateSources(
        PinnedRepository(root),
        intake_case.sources().stores,
        main_revision=producer,
        legacy_bytes=intake_case.source.legacy,
        proposals=intake_case.source.proposals,
    )
    config = {
        k: v
        for k, v in intake_case.source.recipe.config.items()
        if k in {"recognition_policy", "source_batch", "references"}
    }
    pins, context, replay = generate(
        sources, producer, EffectInputs(kind="effect"), effect_config=config
    )
    inventory = InventoryV2(
        template_source_format=2,
        kind="template_source_inventory",
        recipes=pins,
        replay_context=context,
        entries=tuple(
            m.entry for m in sorted(replay.entries, key=lambda m: m.entry.id)
        ),
    )
    files = copy.deepcopy(intake_case.files)
    files[INVENTORY] = inventory.model_dump(mode="json")
    adoption = write(root, files)
    fresh = TemplateSources(
        PinnedRepository(root),
        sources.stores,
        main_revision=adoption,
        legacy_bytes=sources.legacy_bytes,
        proposals=sources.proposals,
    )
    actual = load_templates(PinnedRepository(root), adoption, fresh)
    assert actual.frequencies == intake_case.verified.frequencies
    assert actual.records() == intake_case.verified.records()
