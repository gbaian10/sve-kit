"""Reconstruct the current full source closure once per build, without old producer replay."""

from collections import Counter
from copy import deepcopy
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.template_parameter_rules.current import resolve
from sve_carddb.template_parameter_rules.models import LEGACY_IDS as LEGACY_RULES
from sve_carddb.template_parameters.inventory import build
from sve_carddb.template_parameters.models import Range
from sve_carddb.template_sources.flavor import partition
from sve_carddb.template_sources.inventory import coverage, scan_current
from sve_carddb.template_sources.pins import PARSER
from sve_carddb.template_translations.flavor_models import (
    FlavorCandidate,
    FlavorEntry,
    FlavorSpan,
)
from sve_carddb.template_translations.sources import Reconstructed, _members
from sve_carddb.translations.sources import pointer

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.catalog.adoption_models import Batch
    from sve_carddb.template_parameter_rules.current import Rules
    from sve_carddb.template_parameters.references import References
    from sve_carddb.template_translations.current_owners import Owners


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
        *,
        owners: Owners | None = None,
    ) -> None:
        self.stores = dict(stores)
        self.references = deepcopy(references)
        self.rules = deepcopy(rules)
        self.owners = owners
        self._cache: dict[tuple[str, str], Generated] = {}
        self.generated_batches = 0

    def generate(self, batches: tuple[Batch, ...]) -> Generated:
        """Within one build, each sealed batch is parsed and classified only once."""
        entries: list[Reconstructed] = []
        reports: list[JsonValue] = []
        for batch in batches:
            key = batch.store_id, batch.batch_id
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
        store = self.stores.get(batch.store_id)
        if store is None:
            raise ValueError("Current template source store is unavailable")
        frozen = FrozenSources(store, batch.store_id, batch.batch_id)
        scan = scan_current(frozen)
        candidates = build(
            frozen,
            scan,
            self.references,
            enabled_rules=tuple(
                key for key in self.rules.enabled() if key not in LEGACY_RULES
            ),
        )
        resolved, remaining = resolve(self.rules, candidates)
        solved = {
            (str(row["inventory_id"]), str(row["slot"])): row
            for row in (object_value(parse(raw)) for raw in resolved)
        }
        pending: dict[str, set[str]] = {}
        for raw in remaining:
            row = object_value(parse(raw))
            pending.setdefault(str(row["inventory_id"]), set()).update(
                str(i) for i in array(row["issues"])
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
            tuple(scan.entries), tuple(candidates.entries), texts, solved, pending
        )
        flavor, flavor_fields = self._flavor(frozen, scan.documents)
        return Generated(
            tuple(sorted((*effect, *flavor), key=lambda member: member.entry.id)),
            canonical(
                {
                    "source_batch": batch.model_dump(mode="json"),
                    "effect_coverage": coverage(scan),
                    "flavor_states": dict(
                        Counter(str(object_value(f)["state"]) for f in flavor_fields)
                    ),
                    "flavor_fields": flavor_fields,
                    "remaining_slots": [parse(raw) for raw in remaining],
                }
            ),
        )

    def _flavor(
        self, frozen: FrozenSources, documents: dict[str, JsonValue]
    ) -> tuple[list[Reconstructed], list[JsonValue]]:
        members = []
        proofs: list[JsonValue] = []
        for current in frozen.inventory.current:
            source, _, _ = frozen.read(current.source_version_id, parser_version=PARSER)
            document = documents.get(source.id)
            if document is None:
                raise ValueError("Current flavor source cannot be projected")
            for index, face in enumerate(array(object_value(document)["faces"])):
                locator = f"/faces/{index}/flavor"
                value = object_value(face).get("flavor")
                if value is not None and not isinstance(value, str):
                    raise ValueError("Flavor field must be exact text or unknown")
                result = partition(value)
                proofs.append(
                    {
                        "source_version_id": source.id,
                        "locator": locator,
                        "state": result.state,
                        "text_hash": None if value is None else digest(value.encode()),
                    }
                )
                part = result.part
                if part is None:
                    continue
                ref = SourceRef(
                    store_id=frozen.store_id,
                    batch_id=frozen.batch_id,
                    source_version_id=source.id,
                    parser=PARSER,
                    locator=locator,
                    text_hash=part.normalized_hash,
                )
                span = FlavorSpan(
                    role="flavor",
                    segments=(Range(start=0, end=len(part.normalized)),),
                    anchor=None,
                )
                identifier = (
                    "inv:"
                    + digest(
                        canonical(
                            [
                                ref.model_dump(mode="json"),
                                0,
                                "flavor",
                                [[0, len(part.normalized)]],
                            ]
                        )
                    )[7:]
                )
                entry = FlavorEntry(
                    id=identifier,
                    level="sentence",
                    source_ref=ref,
                    line_ordinal=0,
                    role="flavor",
                    normalizer_id="flavor-exact-v1",
                    normalized_hash=part.normalized_hash,
                    legacy_fingerprint=None,
                )
                owner = (
                    None
                    if self.owners is None
                    else self.owners.resolve(ref, source, part.normalized)
                )
                members.append(
                    Reconstructed(
                        entry,
                        FlavorCandidate(span),
                        part.normalized,
                        part.normalized,
                        (),
                        (),
                        () if owner else ("missing_flavor_identity_owner",),
                        owner,
                    )
                )
        return members, proofs
