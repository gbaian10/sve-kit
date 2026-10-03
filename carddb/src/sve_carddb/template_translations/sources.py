"""Replay formal inventories from sealed pages and the approved recognition recipe."""

from copy import deepcopy
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.template_parameter_rules.loader import load_config
from sve_carddb.template_parameter_rules.models import LEGACY_IDS
from sve_carddb.template_parameter_rules.replay import (
    _historical_positions,
    _recipe,
    _references,
    _resolve,
    _vocabulary,
    numeric_identity,
)
from sve_carddb.template_parameters.analysis import (
    SAFE_INTEGER,
    VERSION_PARAMETERS,
    prepared,
)
from sve_carddb.template_parameters.inventory import build
from sve_carddb.template_parameters.verification import verify_values
from sve_carddb.template_sources.checkpoint import compare, parse_legacy
from sve_carddb.template_sources.inventory import coverage, entry, scan_batch
from sve_carddb.template_sources.normalizer import VERSION, partition
from sve_carddb.template_sources.pins import PARSER
from sve_carddb.translations.sources import pointer, project

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.catalog.adoption_sources import PinnedRepository
    from sve_carddb.template_parameter_rules.replay import ProposalInputs
    from sve_carddb.template_parameters.models import Candidate, Hint, Schema
    from sve_carddb.template_sources.models import Entry, Recipe

ORDINALS = {"choice_ordinal", "card_ordinal", "repetition_ordinal", "turn_ordinal"}


@dataclass(frozen=True)
class Reconstructed:
    entry: Entry
    candidate: Candidate
    normalized: str
    field_text: str
    hints: tuple[Hint, ...]
    roles: tuple[str, ...]
    pending: tuple[str, ...]

    def verify_schema(self, schema: Schema) -> None:
        """Renaming or merging repeated slots cannot erase a source position or its role."""
        if self.pending:
            raise ValueError(
                "Template definition source has unresolved parameter roles"
            )
        hints = []
        used = set()
        for slot in schema.slots:
            roles = set()
            for occurrence in slot.occurrences:
                found = [
                    i
                    for i, hint in enumerate(self.hints)
                    if hint.occurrence == occurrence
                ]
                if len(found) != 1 or found[0] in used:
                    raise ValueError(
                        "Template schema must cover each source position exactly once"
                    )
                index = found[0]
                used.add(index)
                hint = self.hints[index]
                roles.add(self.roles[index])
                lower = 1 if self.roles[index] in ORDINALS else 0
                if slot.type == "uint" and (
                    slot.min != lower or slot.max != SAFE_INTEGER
                ):
                    raise ValueError(
                        "Template numeric bounds differ from the recognized role"
                    )
                hints.append(hint.model_copy(update={"name": slot.name}))
            if len(roles) != 1:
                raise ValueError(
                    "Repeated template slot cannot combine distinct semantic roles"
                )
        if used != set(range(len(self.hints))):
            raise ValueError(
                "Template schema must cover each source position exactly once"
            )
        verify_values(self.field_text, self.normalized, schema, tuple(hints))

    def role_signature(self) -> bytes:
        """Keep semantic role differences separate from type/coordinate equality."""
        return canonical(
            [
                [hint.occurrence.model_dump(mode="json"), role]
                for hint, role in zip(self.hints, self.roles, strict=True)
            ]
        )


@dataclass(frozen=True)
class SourceReplay:
    recipe: bytes
    entries: tuple[Reconstructed, ...]
    source_coverage: bytes
    checkpoint: bytes


class TemplateSources:
    def __init__(
        self,
        repository: PinnedRepository,
        stores: dict[str, Path],
        *,
        main_revision: str,
        legacy_bytes: bytes,
        proposals: ProposalInputs | None = None,
    ) -> None:
        self.repository = repository
        self.stores = dict(stores)
        self.main_revision = main_revision
        self.legacy_bytes = legacy_bytes
        self.proposals = proposals
        self._cache: dict[bytes, SourceReplay] = {}

    def reconstruct(self, pins: tuple[Recipe, ...]) -> SourceReplay:
        """The caller supplies pins, never normalized text, hints or resolution claims."""
        recipes = {r.id: r for r in pins}
        if len(recipes) != len(pins) or set(recipes) != {
            VERSION,
            PARSER,
            VERSION_PARAMETERS,
        }:
            raise ValueError(
                "Formal template inventory requires its three complete recipe pins"
            )
        recipe = recipes[VERSION_PARAMETERS]
        key = canonical([r.model_dump(mode="json") for r in pins])
        if key not in self._cache:
            source_pins = _recipe(self.repository, recipe)
            if tuple(recipes[r.id] for r in source_pins) != source_pins:
                raise ValueError(
                    "Formal template source recipes differ from the parameter recipe"
                )
            if set(recipe.config) != {
                "recognition_policy",
                "source_recipes",
                "source_batch",
                "references",
                "legacy_file_hash",
            } or recipe.config["legacy_file_hash"] != digest(self.legacy_bytes):
                raise ValueError(
                    "Formal template recipe requires its exact legacy input and closed config"
                )
            self._cache[key] = self._replay(recipe, source_pins)
        return deepcopy(self._cache[key])

    def _replay(self, recipe: Recipe, pins: tuple[Recipe, ...]) -> SourceReplay:  # ruff: ignore[too-many-locals] -- a complete frozen batch shares one policy, source scan and reference closure
        loaded = load_config(
            recipe.config, self.repository, main_revision=self.main_revision
        )
        batch = object_value(recipe.config["source_batch"])
        if set(batch) != {"store_id", "batch_id"} or not all(
            isinstance(v, str) for v in batch.values()
        ):
            raise ValueError(
                "Formal template recipe requires one exact frozen source batch"
            )
        store, identifier = str(batch["store_id"]), str(batch["batch_id"])
        if loaded is not None and (
            loaded.policy.scope.source_batches[0].store_id,
            loaded.policy.scope.source_batches[0].batch_id,
        ) != (store, identifier):
            raise ValueError("Formal template batch differs from its recognition scope")
        if store not in self.stores:
            raise ValueError("Formal template frozen source store is unavailable")
        sources = FrozenSources(self.stores[store], store, identifier)
        scan = scan_batch(sources, repository=self.repository.root, pins=pins)
        legacy = parse_legacy(self.legacy_bytes)
        checkpoint = compare(scan, legacy)
        if (
            any(
                object_value(checkpoint[k])["complete"] is not True
                for k in ("fingerprints", "legacy_member_coverage")
            )
            or object_value(checkpoint["fingerprints"])["additional_ids"]
            or object_value(checkpoint["legacy_member_coverage"])["additional_members"]
        ):
            raise ValueError(
                "Formal template replay must reproduce all legacy fingerprints and uses"
            )
        refs = _references(self.repository, recipe, self.stores)
        _vocabulary(refs, recipe, self.proposals)
        enabled = (
            ()
            if loaded is None
            else tuple(
                sorted(
                    r.rule_id
                    for r in loaded.policy.rules
                    if r.rule_id not in LEGACY_IDS
                )
            )
        )
        candidates = build(sources, scan, refs, enabled_rules=enabled)
        if loaded is not None and loaded.historical_revision is not None:
            current = tuple(
                sorted(
                    numeric_identity(c, h, h.numeric_rule)
                    for c in candidates.entries
                    for h in c.slots
                    if h.numeric_rule is not None
                )
            )
            if _historical_positions(sources, scan, candidates) != current:
                raise ValueError(
                    "Formal template recognition changed an original numeric position"
                )
        resolved, remaining = _resolve(loaded, candidates)
        solved = {
            (str(r["inventory_id"]), str(r["slot"])): r
            for r in (object_value(parse(b)) for b in resolved)
        }
        pending: dict[str, set[str]] = {}
        for raw in remaining:
            row = object_value(parse(raw))
            pending.setdefault(str(row["inventory_id"]), set()).update(
                str(i) for i in array(row["issues"])
            )
        fields = _fields(sources, tuple(scan.entries))
        result = _members(
            tuple(scan.entries), tuple(candidates.entries), fields, solved, pending
        )
        return SourceReplay(
            canonical(recipe.model_dump(mode="json")),
            tuple(result),
            canonical(coverage(scan)),
            canonical(checkpoint),
        )

    def evidence(self, ref: SourceRef) -> None:
        """Template evidence must locate a complete field in the verified frozen closure."""
        if not any(
            ref == member.entry.source_ref
            for replay in self._cache.values()
            for member in replay.entries
        ):
            raise ValueError(
                "Template evidence is absent from its verified frozen field closure"
            )

    @staticmethod
    def normalized(item: Entry, text: str) -> str:
        """Rebuild one exact source part including the fixed layout parameter recipe."""
        section = (
            int(item.source_ref.locator.rsplit("/", 1)[-1])
            if "/sections/" in item.source_ref.locator
            else None
        )
        for part in partition(text, section=section):
            if entry(item.source_ref, part, VERSION).id == item.id:
                return prepared(text, part)[0].normalized
        raise ValueError(
            "Formal template inventory entry is absent from its frozen field"
        )


def _fields(
    sources: FrozenSources, entries: tuple[Entry, ...]
) -> dict[tuple[str, str], str]:
    fields: dict[tuple[str, str], str] = {}
    for item in entries:
        ref = item.source_ref
        key = ref.source_version_id, ref.locator
        if key not in fields:
            source, raw, _ = sources.read(ref.source_version_id, parser_version=PARSER)
            _, document = project(raw, source.url, "jp")
            value = pointer(document, ref.locator)
            if not isinstance(value, str) or digest(value.encode()) != ref.text_hash:
                raise ValueError(
                    "Formal template field must match its exact source hash"
                )
            fields[key] = value
    return fields


def _members(
    entries: tuple[Entry, ...],
    candidates: tuple[Candidate, ...],
    fields: dict[tuple[str, str], str],
    solved: dict[tuple[str, str], dict[str, JsonValue]],
    pending: dict[str, set[str]],
) -> tuple[Reconstructed, ...]:
    by_id = {e.id: e for e in entries}
    result = []
    for candidate in candidates:
        item = by_id[candidate.inventory_id]
        normalized = TemplateSources.normalized(
            item, fields[item.source_ref.source_version_id, item.source_ref.locator]
        )
        hints, roles = [], []
        for hint in candidate.slots:
            solution = solved.get((item.id, hint.name))
            hints.append(
                hint
                if solution is None
                else hint.model_copy(
                    update={
                        "issues": tuple(
                            str(i) for i in array(solution["remaining_issues"])
                        )
                    }
                )
            )
            roles.append(
                hint.semantic_role
                if solution is None
                else str(solution["recognized_role"])
            )
        # Defining reminder boundaries remains a human decision, not recognition consent.
        causes = pending.get(item.id, set()) - {
            "legacy_parenthesis_classification_requires_review"
        }
        result.append(
            Reconstructed(
                item,
                candidate,
                normalized,
                fields[item.source_ref.source_version_id, item.source_ref.locator],
                tuple(hints),
                tuple(roles),
                tuple(sorted(causes)),
            )
        )
    return tuple(result)
