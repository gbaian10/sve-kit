"""Synthetic drift, environment, independent F1 and hard budget counterexamples."""

import copy
import re
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_bundle import verify_bundle
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.template_semantics import candidates
from sve_carddb.template_semantics.audit import host_context
from sve_carddb.template_semantics.budget import (
    Budget,
    Monitor,
    ReplayBudgetExceededError,
)
from sve_carddb.template_semantics.generation import generate
from sve_carddb.template_semantics.session import replay
from sve_carddb.template_translations.replay_models import (
    EffectInputs,
    FlavorReplayInputs,
    InventoryV2,
    ReplayContext,
)
from sve_carddb.template_translations.sources import TemplateSources

from .adoption_fixtures import commit
from .template_flavor_fixtures import flavor_case
from .template_intake_fixtures import intake_case, policy_git, recognition_term_source

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.template_sources.models import Recipe
    from sve_carddb.template_translations.sources import SourceReplay

    from .template_flavor_fixtures import Case as FlavorCase
    from .template_intake_fixtures import Case

__all__ = ("flavor_case", "intake_case", "policy_git", "recognition_term_source")


def fresh(sources: TemplateSources) -> TemplateSources:
    """A new consumer never borrows the producer's cached replay or output manifest."""
    return TemplateSources(
        sources.repository,
        sources.stores,
        main_revision=sources.main_revision,
        legacy_bytes=sources.legacy_bytes,
        proposals=sources.proposals,
    )


@pytest.fixture(scope="module")
def generated(
    intake_case: Case, tmp_path_factory: pytest.TempPathFactory
) -> tuple[TemplateSources, InventoryV2]:
    root = intake_case.fork(tmp_path_factory.mktemp("semantic-guards") / "guard-repo")
    producer = commit(root)
    from sve_carddb.catalog.adoption_sources import PinnedRepository  # ruff: ignore[import-outside-top-level] -- fixture boundary

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
    pins, context, result = generate(
        sources, producer, EffectInputs(kind="effect"), effect_config=config
    )
    return sources, InventoryV2(
        template_source_format=2,
        kind="template_source_inventory",
        recipes=pins,
        replay_context=context,
        entries=tuple(
            m.entry for m in sorted(result.entries, key=lambda m: m.entry.id)
        ),
    )


@pytest.mark.parametrize(
    "stream", ["entries", "fields", "members", "coverage", "checkpoint", "source_uses"]
)
def test_every_complete_stream_is_an_independent_immutable_drift_gate(
    generated: tuple[TemplateSources, InventoryV2], stream: str
) -> None:
    sources, inventory = generated
    raw = inventory.replay_context.model_dump(mode="json")
    expected = copy.deepcopy(raw["expected_outputs"])
    expected["streams"][stream]["hash"] = "sha256:" + "f" * 64
    expected["root"] = digest(
        canonical({k: v for k, v in expected.items() if k != "root"})
    )
    raw["expected_outputs"] = expected
    context = ReplayContext.model_validate_json(canonical(raw))
    detail = canonical(
        {
            "stream": stream,
            "expected": expected["streams"][stream],
            "actual": inventory.replay_context.expected_outputs.streams.model_dump(
                mode="json"
            )[stream],
            "environment_differences": {},
        }
    ).decode()
    with pytest.raises(
        ValueError, match="^" + re.escape("replay_output_mismatch: " + detail) + "$"
    ):
        fresh(sources).reconstruct_v2(inventory.recipes, context)


def test_environment_difference_is_recorded_and_continues_with_identical_outputs(
    generated: tuple[TemplateSources, InventoryV2],
) -> None:
    sources, inventory = generated
    raw = inventory.replay_context.model_dump(mode="json")
    raw["environment"]["python"]["version"] = "3.14.999"
    context = ReplayContext.model_validate_json(canonical(raw))
    result = fresh(sources).reconstruct_v2(inventory.recipes, context)
    assert result.semantic_output is not None
    assert result.semantic_output.manifest == context.expected_outputs
    delta = object_value(parse(result.provenance))["environment_differences"]
    assert (
        object_value(object_value(delta)["python"])["producer"]
        == raw["environment"]["python"]
    )


def test_private_candidate_requires_separate_hash_and_reloads_written_baseline(
    generated: tuple[TemplateSources, InventoryV2], tmp_path: Path
) -> None:
    sources, inventory = generated
    result = fresh(sources).reconstruct_v2(inventory.recipes, inventory.replay_context)
    target = tmp_path / "private-candidate"
    checksum = candidates.write(
        target,
        inventory.recipes,
        inventory.replay_context,
        result,
        inputs=(sources.repository.root, *sources.stores.values()),
    )
    loaded = candidates.read(target, checksum)
    assert (
        tuple(entry for shard in loaded for entry in shard.entries) == inventory.entries
    )
    independent = fresh(sources).reconstruct_v2(
        loaded[0].recipes, loaded[0].replay_context
    )
    assert independent.semantic_output == result.semantic_output
    with pytest.raises(
        ValueError, match=r"^Replay candidate index exact hash mismatch$"
    ):
        candidates.read(target, "sha256:" + "f" * 64)
    (target / "001.yaml").write_bytes(b"{}")
    with pytest.raises(
        ValueError, match=r"^Replay candidate shard exact hash mismatch$"
    ):
        candidates.read(target, checksum)


def test_host_f1_uses_actual_program_and_independent_expected_plan(
    generated: tuple[TemplateSources, InventoryV2], tmp_path: Path
) -> None:
    sources, inventory = generated
    destination = tmp_path / "f1"
    report = replay((inventory,), fresh(sources), sources.main_revision, destination)
    assert report["groups"] == report["replay_count"] == 1
    assert {p.name for p in destination.iterdir()} == {
        "build.sqlite",
        "inputs.json",
        "report.json",
        "seal.json",
    }
    from sve_carddb.build_inputs import InputRecord  # ruff: ignore[import-outside-top-level] -- wire readback
    from sve_carddb.template_semantics.audit import ExpectedPlan  # ruff: ignore[import-outside-top-level] -- independent verification

    record = InputRecord.model_validate_json((destination / "inputs.json").read_bytes())
    context = host_context(
        sources.repository,
        sources.main_revision,
        parse(record.context.configuration.encode()),
    )
    expected = ExpectedPlan(sources.repository, sources.stores).expected(
        inventory.recipes, inventory.replay_context, sources.main_revision
    )
    assert (
        verify_bundle(
            compile_t0(), destination, context, expected, stores=sources.stores
        )
        == record
    )


def test_actual_use_omission_does_not_shrink_the_expected_plan(
    generated: tuple[TemplateSources, InventoryV2],
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    sources, inventory = generated
    original = TemplateSources.reconstruct_v2
    monkeypatch.setattr(
        TemplateSources,
        "reconstruct_v2",
        lambda self, pins, context: replace(
            original(self, pins, context), source_uses=()
        ),
    )
    with pytest.raises(
        ValueError, match=r"^Replay independent source use closure mismatch$"
    ):
        replay(
            (inventory,), fresh(sources), sources.main_revision, tmp_path / "refused"
        )
    assert not (tmp_path / "refused").exists()


def test_flavor_exact_v2_replays_with_its_own_complete_owner_basis(
    flavor_case: FlavorCase, tmp_path: Path
) -> None:
    from sve_carddb.catalog.adoption_sources import PinnedRepository  # ruff: ignore[import-outside-top-level] -- fixture boundary
    from sve_carddb.template_translations.flavor_models import FlavorInputs  # ruff: ignore[import-outside-top-level] -- generation interface

    root = flavor_case.fork(tmp_path / "flavor-v2")
    producer = commit(root)
    sources = TemplateSources(
        PinnedRepository(root),
        {"test-store": flavor_case.store},
        main_revision=producer,
        legacy_bytes=b"",
    )
    assert isinstance(flavor_case.sources.flavor, FlavorInputs)
    inputs = FlavorReplayInputs(
        kind="flavor",
        source_batch=flavor_case.sources.flavor.source_batch,
        identity_basis=flavor_case.basis,
        identity_batches=flavor_case.sources.flavor.identity_batches,
    )
    assert isinstance(flavor_case.sources.flavor, FlavorInputs)
    pins, context, generated = generate(sources, producer, inputs)
    actual = fresh(sources).reconstruct_v2(pins, context)
    assert actual.semantic_output == generated.semantic_output
    assert [(m.normalized, m.owner, m.pending) for m in actual.entries] == [
        (m.normalized, m.owner, m.pending) for m in flavor_case.replay.entries
    ]


def test_hard_budget_refuses_before_publication_and_watchdog_restores_timer(
    tmp_path: Path,
) -> None:
    import signal  # ruff: ignore[import-outside-top-level] -- process state is observed only in this test

    previous = signal.getsignal(signal.SIGALRM)
    with pytest.raises(ReplayBudgetExceededError, match=r"^replay_budget_exceeded$"):
        with Monitor(Budget(wall_seconds=0)).watchdog():
            (tmp_path / "forbidden").touch()
    assert not (tmp_path / "forbidden").exists()
    assert signal.getsignal(signal.SIGALRM) == previous
    with pytest.raises(ReplayBudgetExceededError, match=r"^replay_budget_exceeded$"):
        Monitor(Budget(rss_bytes=1)).estimate(0, 0)
    with pytest.raises(ValueError, match=r"^replay_budget_exceeded$"):
        Monitor(Budget()).estimate(8, 0)


@pytest.mark.parametrize("shards", [1, 10, 100])
def test_more_shards_of_one_background_do_not_add_replay_work(
    generated: tuple[TemplateSources, InventoryV2],
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
    shards: int,
) -> None:
    from sve_carddb.template_translations import semantic_replay  # ruff: ignore[import-outside-top-level] -- measure the actual full replay boundary

    sources, inventory = generated
    calls = []
    original = semantic_replay.reconstruct

    def counted(
        source: TemplateSources, pins: tuple[Recipe, ...], context: ReplayContext
    ) -> SourceReplay:
        calls.append(1)
        return original(source, pins, context)

    monkeypatch.setattr(semantic_replay, "reconstruct", counted)
    split = tuple(
        inventory.model_copy(update={"entries": inventory.entries if n == 0 else ()})
        for n in range(shards)
    )
    report = replay(
        split, fresh(sources), sources.main_revision, tmp_path / "grouped-f1"
    )
    assert len(calls) == report["replay_count"] == 1
    assert report["entries"] == len(inventory.entries)
    assert report["shards"] == shards
