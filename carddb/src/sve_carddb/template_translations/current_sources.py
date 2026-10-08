"""Reconstruct the current full source closure once per build, without old producer replay."""

from copy import deepcopy
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.snapshot.values import canonical, digest, parse
from sve_carddb.template_parameters.inventory import build
from sve_carddb.template_sources.inventory import coverage, scan_current
from sve_carddb.template_translations.members import Reconstructed, _members
from sve_carddb.translations.sources import pointer

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.catalog.adoption_models import Batch
    from sve_carddb.template_parameter_rules.current import Rules
    from sve_carddb.template_parameters.references import References


@dataclass(frozen=True)
class Generated:
    entries: tuple[Reconstructed, ...]
    report: bytes


class Sources:
    def __init__(
        self,
        stores: dict[str, Path],
        references: References,
        rules: Rules,
    ) -> None:
        self.stores = dict(stores)
        self.references = deepcopy(references)
        self.rules = deepcopy(rules)
        self._cache: dict[str, Generated] = {}
        self.generated_batches = 0

    def generate(self, batches: tuple[Batch, ...]) -> Generated:
        """Within one build, each sealed batch is parsed and classified only once."""
        entries: list[Reconstructed] = []
        reports: list[JsonValue] = []
        for batch in batches:
            key = batch.batch_id
            if key not in self._cache:
                self._cache[key] = self._batch(batch)
                self.generated_batches += 1
            generated = self._cache[key]
            entries.extend(generated.entries)
            reports.append(parse(generated.report))
        if len({member.entry.id for member in entries}) != len(entries):
            raise ValueError(
                "Current source batches contain duplicate inventory entries"
            )
        return Generated(tuple(deepcopy(entries)), canonical(reports))

    def _batch(self, batch: Batch) -> Generated:
        frozen = FrozenSources.configured(self.stores, batch.batch_id)
        scan = scan_current(frozen)
        candidates = build(
            frozen,
            scan,
            self.references,
            enabled_rules=self.rules.enabled(),
        )
        texts = {}
        for item in scan.entries:
            ref = item.source_ref
            value = pointer(scan.documents[ref.source_version_id], ref.locator)
            if not isinstance(value, str) or digest(value.encode()) != ref.text_hash:
                raise ValueError(
                    "Current template field differs from its exact source hash"
                )
            texts[ref.source_version_id, ref.locator] = value
        effect = _members(
            tuple(scan.entries),
            tuple(candidates.entries),
            texts,
        )
        doubtful = {rule.rule_id for rule in self.rules.rules if rule.low_confidence}
        # A name kept in its source spelling or a doubtful rule makes the whole field uncertain.
        effect = tuple(
            replace(
                member,
                low_confidence=any(
                    hint.rule_id in doubtful
                    or (hint.target is not None and "card_name" in hint.target)
                    for hint in member.hints
                ),
            )
            for member in effect
        )
        return Generated(
            tuple(sorted(effect, key=lambda member: member.entry.id)),
            canonical(
                {
                    "source_batch": batch.model_dump(mode="json"),
                    "effect_coverage": coverage(scan),
                    "remaining_slots": [
                        {"inventory_id": c.inventory_id, "issues": list(c.issues)}
                        for c in candidates.entries
                        if c.issues
                    ],
                }
            ),
        )
