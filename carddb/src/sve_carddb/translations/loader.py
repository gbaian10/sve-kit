"""Validate the complete adopted glossary history before projecting terminal choices."""

import re
from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError

from sve_carddb.catalog.adoption_loader import ordered
from sve_carddb.registry.records import RecordData
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, parse
from sve_carddb.translations.models import (
    AssignmentRecord,
    ChoiceRecord,
    ConceptRecord,
    Decision,
    EmphasisRecord,
    Index,
    Record,
    Shard,
    TermRecord,
    VocabularyRecord,
)

if TYPE_CHECKING:
    from pathlib import Path


def _model[T: RecordData](model: type[T], value: JsonValue) -> T:
    try:
        return model.model_validate_json(canonical(value))
    except ValidationError as error:
        location = ".".join(
            str(part) for part in error.errors(include_input=False)[0]["loc"]
        )
        raise ValueError(
            "Invalid translation authored fields at " + (location or "root")
        ) from None


def record_hash(record: Record) -> str:
    """Hash the full adopted payload including its evidence."""
    return digest(canonical(record.model_dump(mode="json")))


def key(record: Record) -> str:
    """Use explicit primary keys rather than names, translated text or file order."""
    if isinstance(record, TermRecord):
        fields: list[JsonValue] = [record.kind, record.data.id]
    elif isinstance(record, EmphasisRecord):
        fields = [record.kind, record.data.term_id]
    elif isinstance(record, ChoiceRecord):
        fields = [record.kind, record.data.term_id, record.data.lang]
    elif isinstance(record, AssignmentRecord):
        fields = [
            record.kind,
            record.data.owner.model_dump(mode="json"),
            record.data.field,
            record.data.ordinal,
        ]
    elif isinstance(record, ConceptRecord):
        fields = [record.kind, record.data.subject.model_dump(mode="json")]
    else:
        fields = [
            record.kind,
            record.data.vocabulary_kind,
            record.data.vocabulary_code,
            record.data.lang,
        ]
    if not isinstance(record, TermRecord):
        fields.append(record.data.adoption_no)
    return canonical(fields).decode()


@dataclass(frozen=True)
class Emphasis:
    bold: bool | None
    record_hash: str | None
    decision_id: str | None


@dataclass(frozen=True)
class Snapshot:
    index: bytes
    shards: tuple[tuple[str, bytes, bytes], ...]
    closure: tuple[tuple[str, bytes, bytes], ...] = ()

    def envelopes(self) -> tuple[Shard, ...]:
        """Return detached models; callers cannot mutate the approved byte snapshot."""
        return tuple(
            Shard.model_validate_json(content) for _, _, content in self.shards
        )

    def records(self) -> tuple[tuple[Record, str], ...]:
        """Retain historical records together with exact batch decisions."""
        return tuple(
            (r, s.default_decision_id) for s in self.envelopes() for r in s.records
        )

    def effective(self) -> tuple[tuple[Record, str], ...]:
        """Choose terminal adopted revisions after complete chain validation."""
        latest: dict[str, tuple[Record, str]] = {}
        for record, decision in self.records():
            subject = (
                record.record_key
                if isinstance(record, TermRecord)
                else canonical(array(parse(record.record_key.encode()))[:-1]).decode()
            )
            previous = latest.get(subject)
            if previous is None or (
                not isinstance(record, TermRecord)
                and record.data.adoption_no > revision(previous[0])
            ):
                latest[subject] = record, decision
        return tuple(latest[name] for name in sorted(latest))

    def emphasis(self, term_id: str) -> Emphasis:
        """Separate missing presentation metadata from false or missing translation."""
        terms = {r.data.id: r for r, _ in self.effective() if isinstance(r, TermRecord)}
        if term_id not in terms:
            raise ValueError("Emphasis references an unadopted concept")
        if terms[term_id].data.category != "rule_term":
            return Emphasis(True, None, None)
        for record, decision in self.effective():
            if isinstance(record, EmphasisRecord) and record.data.term_id == term_id:
                return Emphasis(record.data.value, record_hash(record), decision)
        return Emphasis(None, None, None)

    def review_counts(self) -> dict[str, int]:
        """Count delegated checks separately from actual human sample declarations."""
        human = delegated = 0
        for shard in self.envelopes():
            if (
                isinstance(shard.records[0], (AssignmentRecord, ConceptRecord))
                or shard.records[0].data.adoption_review.mode == "human"
            ):
                human += len(shard.decisions[0].sample_ids)
            else:
                delegated += len(shard.records)
        return {"human_sampled_rows": human, "delegated_glossary_rows": delegated}

    def pins(self) -> dict[str, JsonValue]:
        """Distinguish exact YAML inputs from canonical membership hashes."""
        return {
            "index_hash": digest(self.index),
            "shards": [
                {
                    "path": p,
                    "exact_hash": digest(exact),
                    "canonical_hash": digest(content),
                }
                for p, exact, content in (self.closure or self.shards)
            ],
        }


def revision(record: Record) -> int:
    """Immutable concepts have no adoption revision."""
    return 0 if isinstance(record, TermRecord) else record.data.adoption_no


def validate_snapshot(snapshot: Snapshot) -> None:
    """Validate glossary and name overrides in a separately verified full closure."""
    for path, _, content in snapshot.shards:
        match = re.fullmatch(
            r"translations/(glossary|overrides)/([A-Za-z0-9_-]+)/[0-9]{3,}\.yaml", path
        )
        if match is None:
            raise ValueError("Glossary snapshot contains an unsupported shard path")
        shard = _model(Shard, parse(content))
        is_override = isinstance(shard.records[0], (AssignmentRecord, ConceptRecord))
        if is_override != (match[1] == "overrides"):
            raise ValueError("Translation record is in the wrong authored area")
        _envelope(shard, match[2])
    _history(snapshot)


def _safe(path: Path) -> None:
    if any(part.is_symlink() for part in (path, *path.parents)) or not path.is_file():
        raise ValueError("Missing or symlink translation input")


def load_glossary(root: Path) -> Snapshot:  # ruff: ignore[complex-structure] -- every filesystem closure guard precedes projection
    """Verify the full closure and project only glossary and name override records."""
    path = root / "translations/index.yaml"
    _safe(path)
    index = _model(Index, read_yaml(path))
    if set(index.includes) & set(index.inventories):
        raise ValueError("Template shard and inventory paths must be disjoint")
    indexed = {**index.includes, **index.inventories}
    present = set()
    for file in path.parent.rglob("*"):
        if file.is_symlink():
            raise ValueError("Symlink translation input")
        if not file.is_dir() and file != path:
            present.add(file.relative_to(root).as_posix())
    if present != set(indexed):
        raise ValueError("Translation indexed file closure differs from disk")
    sequences: dict[str, list[int]] = defaultdict(list)
    shards = []
    closure = []
    for name, checksum in sorted(indexed.items()):
        match = re.fullmatch(
            r"translations/(glossary|overrides|templates)/([A-Za-z0-9_-]+)/([0-9]{3,})\.yaml",
            name,
        )
        inventory = re.fullmatch(
            r"translations/template-sources/([0-9]{3,})\.yaml", name
        )
        if (name in index.includes and match is None) or (
            name in index.inventories and inventory is None
        ):
            raise ValueError("Unsafe or unsupported translation include")
        sequences[name.rsplit("/", 1)[0]].append(int(name.rsplit("/", 1)[1][:-5]))
        file = root / name
        _safe(file)
        content = canonical(read_yaml(file))
        if digest(content) != checksum:
            raise ValueError("Translation shard hash mismatch")
        closure.append((name, file.read_bytes(), content))
        if inventory is not None or (match is not None and match[1] == "templates"):
            _template_input(name, content)
            continue
        assert match is not None
        shards.append((name, file.read_bytes(), content))
    for numbers in sequences.values():
        if sorted(numbers) != list(range(1, len(numbers) + 1)):
            raise ValueError("Translation shard sequence gap")
    snapshot = Snapshot(path.read_bytes(), tuple(shards), tuple(closure))
    validate_snapshot(snapshot)
    return snapshot


def _template_input(path: str, content: bytes) -> None:
    """Foreign envelopes are checked, but source replay belongs to load_templates."""
    from sve_carddb.template_translations.loader import _inventory, _shard, envelope  # ruff: ignore[import-outside-top-level] -- the two area validators share a closure without a module import cycle

    if path.startswith("translations/template-sources/"):
        data = _inventory(content)
        from sve_carddb.template_semantics.registry import foreign  # ruff: ignore[import-outside-top-level] -- finite semantic support is independent of the source loader
        from sve_carddb.template_translations.replay_models import InventoryV2  # ruff: ignore[import-outside-top-level] -- foreign validation must not recursively import source replay

        if isinstance(data, InventoryV2):
            foreign(data.replay_context)
    else:
        envelope(_shard(content), path.split("/")[2])


def _envelope(shard: Shard, filing: str) -> None:  # ruff: ignore[complex-structure] -- exact membership and separate human/delegated gates are independently checked
    decision = shard.decisions[0]
    keys = [r.record_key for r in shard.records]
    if keys != sorted(set(keys)):
        raise ValueError("Translation members must be sorted and unique")
    members = tuple((r.record_key, record_hash(r)) for r in shard.records)
    checksum = digest(canonical([[k, h] for k, h in members]))
    if (
        decision.members != members
        or decision.membership_hash != checksum
        or decision.id != "d:" + checksum.removeprefix("sha256:")
        or shard.default_decision_id != decision.id
    ):
        raise ValueError("Translation decision exact membership mismatch")
    if (
        not decision.sample_ids
        or tuple(sorted(set(decision.sample_ids))) != decision.sample_ids
        or not set(decision.sample_ids) <= set(keys)
    ):
        raise ValueError("Translation decision requires actual checked members")
    if decision.state == "confirmed" and decision.sample_ids != tuple(keys):
        raise ValueError("Confirmed translation must check every member")
    modes = {
        "human"
        if isinstance(record, (AssignmentRecord, ConceptRecord))
        else record.data.adoption_review.mode
        for record in shard.records
    }
    if len(modes) != 1:
        raise ValueError("Glossary review modes must be uniform within a shard")
    for record in shard.records:
        if (
            record.kind != decision.category
            or record.filing_key != filing
            or record.record_key != key(record)
        ):
            raise ValueError("Translation record kind/key/filing mismatch")
        if isinstance(record, (AssignmentRecord, ConceptRecord)):
            if decision.reviewed_by != "gbaian10":
                raise ValueError("Name override requires the maintainer human reviewer")
            if (
                isinstance(record, ConceptRecord)
                and decision.policy_id != "card-name-concept-v1"
            ):
                raise ValueError("Name concept decision policy mismatch")
        else:
            _delegated(record, decision)
        ordered(record.evidence)
        if (
            isinstance(record, TermRecord)
            and record.data.id != "term:" + record.data.concept_key
        ):
            raise ValueError("Permanent term ID differs from concept key")


def _delegated(
    record: TermRecord | ChoiceRecord | VocabularyRecord | EmphasisRecord,
    decision: Decision,
) -> None:
    review = record.data.adoption_review
    if review.delegation is not None:
        receipt = review.delegation
        if decision.state != "confirmed":
            raise ValueError("Delegated glossary requires confirmed full checks")
        if (
            decision.reviewed_by,
            decision.reviewed_at,
            decision.reviewed_precision,
        ) != (receipt.decided_by, receipt.decided_at, receipt.decided_precision):
            raise ValueError("Delegated glossary decision differs from receipt event")
        if "維護者委託；協調者決定；不是維護者親自核可" not in decision.note:
            raise ValueError("Delegated glossary note must identify delegated approval")
        if record.record_key not in receipt.scope:
            raise ValueError("Delegation scope excludes current glossary record")


def _history(snapshot: Snapshot) -> None:  # ruff: ignore[complex-structure,too-many-branches] -- immutable allocations and revision chains share one complete inventory
    seen: dict[str, tuple[Record, str]] = {}
    concepts = set()
    chains: dict[
        str,
        list[
            tuple[
                ChoiceRecord
                | VocabularyRecord
                | EmphasisRecord
                | AssignmentRecord
                | ConceptRecord,
                str,
            ]
        ],
    ] = defaultdict(list)
    for record, decision in snapshot.records():
        if record.record_key in seen:
            raise ValueError("Duplicate immutable translation record")
        seen[record.record_key] = record, decision
        if isinstance(record, TermRecord):
            if record.data.concept_key in concepts:
                raise ValueError("Concept key already allocated")
            concepts.add(record.data.concept_key)
        else:
            subject = canonical(array(parse(record.record_key.encode()))[:-1]).decode()
            chains[subject].append((record, decision))
    for record, _ in snapshot.records():
        receipt = (
            None
            if isinstance(record, (AssignmentRecord, ConceptRecord))
            else record.data.adoption_review.delegation
        )
        if receipt is not None:
            for member in receipt.scope:
                scoped = seen.get(member)
                if scoped is None:
                    raise ValueError(
                        "Delegation scope references absent glossary record"
                    )
                if (
                    isinstance(scoped[0], (AssignmentRecord, ConceptRecord))
                    or scoped[0].data.adoption_review.delegation != receipt
                ):
                    raise ValueError("Delegation scope member has a different receipt")
    for chain in chains.values():
        chain.sort(key=lambda item: item[0].data.adoption_no)
        previous = None
        for number, (record, decision) in enumerate(chain, 1):
            if record.data.adoption_no != number:
                raise ValueError("Translation adoption sequence gap or fork")
            predecessor = record.data.predecessor
            if previous is None:
                if predecessor is not None:
                    raise ValueError("Initial choice must have no predecessor")
            elif predecessor is None or (
                predecessor.record_key,
                predecessor.record_hash,
                predecessor.decision_id,
            ) != (previous[0].record_key, record_hash(previous[0]), previous[1]):
                raise ValueError("Translation predecessor mismatch")
            previous = record, decision
    terms = {r.data.id for r, _ in snapshot.records() if isinstance(r, TermRecord)}
    if any(
        isinstance(r, ChoiceRecord) and r.data.term_id not in terms
        for r, _ in snapshot.records()
    ):
        raise ValueError("Glossary choice references an unadopted concept")

    categories = {
        r.data.id: r.data.category
        for r, _ in snapshot.records()
        if isinstance(r, TermRecord)
    }
    for record, _ in snapshot.records():
        if isinstance(record, EmphasisRecord):
            if record.data.term_id not in categories:
                raise ValueError("Emphasis references an unadopted concept")
            if categories[record.data.term_id] != "rule_term":
                raise ValueError("Only rule terms accept emphasis choices")

    keys = {
        r.data.concept_key
        for r, _ in snapshot.records()
        if isinstance(r, TermRecord) and r.data.category == "card_name"
    }
    for record, _ in snapshot.records():
        if (
            isinstance(record, ConceptRecord)
            and record.data.term_id is not None
            and categories.get(record.data.term_id) != "card_name"
        ):
            raise ValueError("Name concept requires an adopted card-name term")
        if isinstance(record, AssignmentRecord) and record.data.concept_key not in keys:
            raise ValueError(
                "Name assignment requires an adopted card-name concept key"
            )
