"""Complete historical groups, one executing-host F1 bundle and bounded publication."""

import resource
import time
from typing import TYPE_CHECKING

from sve_carddb.build_bundle import publish_bundle
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.build_inputs import (
    InputRecord,
    SourceUse,
    insert_raw_sources,
    uses_sorted,
)
from sve_carddb.snapshot.values import canonical, digest, parse
from sve_carddb.template_semantics.audit import ExpectedPlan, host_context
from sve_carddb.template_semantics.budget import Budget, Monitor
from sve_carddb.template_semantics.registry import DENIED
from sve_carddb.template_translations.replay_models import FlavorReplayInputs

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.build_db import Database
    from sve_carddb.template_translations.replay_models import InventoryV2
    from sve_carddb.template_translations.sources import TemplateSources


def replay(  # ruff: ignore[too-many-locals,too-many-statements] -- one session binds groups, budgets and the single F1 publication
    inventories: tuple[InventoryV2, ...],
    sources: TemplateSources,
    host: str,
    destination: Path,
    *,
    budget: Budget = Budget(),
) -> dict[str, JsonValue]:
    """Actual uses cannot define expected uses; no inventory or historical group is skipped."""
    started = time.monotonic()
    monitor = Monitor(budget)
    grouped: dict[str, list[InventoryV2]] = {}
    for inventory in inventories:
        grouped.setdefault(inventory.group_key(), []).append(inventory)
    if not grouped:
        raise ValueError("Replay requires at least one complete inventory group")
    plans = ExpectedPlan(sources.repository, sources.stores)
    expected: list[SourceUse] = []
    actual: list[SourceUse] = []
    reports: list[JsonValue] = []
    identifiers: set[str] = set()
    with monitor.watchdog():
        effects = flavors = 0.0
        for group in grouped.values():
            inventory = group[0]
            context = inventory.replay_context
            if isinstance(context.inputs, FlavorReplayInputs):
                batch = context.inputs.source_batch
                flavors += (
                    len(plans.batch(batch.store_id, batch.batch_id).inventory.current)
                    / 7369
                )
            else:
                pin = next(
                    p
                    for p in inventory.recipes
                    if p.id == "template-parameters-jp-candidate-v1"
                )
                from sve_carddb.snapshot.values import object_value  # ruff: ignore[import-outside-top-level] -- only the effect branch has this closed input

                batch_data = object_value(pin.config["source_batch"])
                effects += (
                    len(
                        plans.batch(
                            str(batch_data["store_id"]), str(batch_data["batch_id"])
                        ).inventory.current
                    )
                    / 7369
                )
        monitor.estimate(effects, flavors)
        retained = 0
        for identity, group in sorted(grouped.items()):
            inventory = group[0]
            before = time.monotonic()
            independent = sources.expected_v2(
                inventory.recipes, inventory.replay_context
            )
            expected.extend(independent)
            result = sources.reconstruct_v2(inventory.recipes, inventory.replay_context)
            actual.extend(result.source_uses)
            declared = {entry.id: entry for shard in group for entry in shard.entries}
            count = sum(len(shard.entries) for shard in group)
            if len(declared) != count or identifiers.intersection(declared):
                raise ValueError("Replay inventory entry IDs must be globally unique")
            identifiers.update(declared)
            if declared != {m.entry.id: m.entry for m in result.entries}:
                raise ValueError("Replay inventory group must cover its complete batch")
            assert result.semantic_output is not None
            retained += sum(len(raw) for _name, raw in result.semantic_output.streams)
            retained += sum(
                len(m.normalized.encode()) + len(m.field_text.encode())
                for m in result.entries
            )
            monitor.estimate(effects, flavors, retained_bytes=retained)
            reports.append(
                {
                    "group_key": identity,
                    "replay_context": inventory.replay_context.model_dump(mode="json"),
                    "recipes": [p.model_dump(mode="json") for p in inventory.recipes],
                    "provenance": parse(result.provenance),
                    "entry_count": len(result.entries),
                    "shard_count": len(group),
                    "elapsed_ms": int((time.monotonic() - before) * 1000),
                }
            )
        expected_uses, actual_uses = uses_sorted(expected), uses_sorted(actual)
        if actual_uses != expected_uses:
            raise ValueError("Replay independent source use closure mismatch")
        config: JsonValue = {
            "template_replay_format": 1,
            "semantic_denials": [list(d) for d in DENIED],
            "groups": reports,
            "budget": budget.model_dump(mode="json"),
        }
        build = host_context(sources.repository, host, config)
        report: dict[str, JsonValue] = {
            "format": 1,
            "groups": len(grouped),
            "entries": len(identifiers),
            "input_hash": digest(
                canonical([i.model_dump(mode="json") for i in inventories])
            ),
            "replay_count": len(grouped),
            "shards": len(inventories),
            "retained_output_bytes": retained,
            "optimization_required": monitor.optimization_required,
            "elapsed_ms": int((time.monotonic() - started) * 1000),
            "peak_rss_bytes": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * 1024,
        }

        def populate(db: Database) -> InputRecord:
            insert_raw_sources(db, (use.source for use in actual_uses))
            return InputRecord(context=build, uses=actual_uses)

        publish_bundle(
            compile_t0(),
            destination,
            build,
            expected_uses,
            populate,
            report,
            stores=sources.stores,
        )
    return report
