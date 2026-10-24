"""Load complete immutable human adoption history; never authorize caller staging."""

import re
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- Only immutable local Git blobs are read.
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import ConfigDict, TypeAdapter

from sve_carddb.core.authored import check_path, read, require_directory
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.provenance import Revision
from sve_carddb.domains.construction.adoption_models import (
    Index,
    ProfileRecord,
    Record,
    Shard,
    basis,
    ordered,
)

if TYPE_CHECKING:
    from pydantic import JsonValue

ENTRY = "construction-adoptions"
_PATH = re.compile(
    r"construction-adoptions/(profiles|restrictions|coverage)/(jp|en)/standard/([0-9]{3,})\.yaml\Z"
)
MODEL_IDENTITIES = frozenset(
    {("anthropic", "Claude Sonnet", "5.5"), ("openai", "Codex", "gpt-6.1-sol")}
)


def committed(repository: Path, revision: str, name: str) -> bytes:
    """Read an immutable blob without allowing revisions or paths to become options."""
    TypeAdapter(Revision, config=ConfigDict(regex_engine="python-re")).validate_python(
        revision
    )
    if (
        Path(name).is_absolute()
        or ".." in Path(name).parts
        or Path(name).as_posix() != name
    ):
        raise ValueError("Construction dependency path must be canonical and relative")
    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- The revision is validated and no shell is used.
        [
            shutil.which("git") or "/usr/bin/git",
            "-C",
            str(repository),
            "show",
            revision + ":" + name,
        ],
        check=True,
        capture_output=True,
    )
    return result.stdout


@dataclass(frozen=True)
class LoadedShard:
    name: str
    exact: bytes
    encoded: bytes
    shard: Shard


@dataclass(frozen=True)
class Snapshot:
    revision: str
    index_exact: bytes
    index_content: bytes
    shards: tuple[LoadedShard, ...]
    current: tuple[Record, ...]

    def configuration(self) -> dict[str, JsonValue]:
        """Pin complete adoption bytes, staging and verified report inventories."""
        return {
            "construction_adoptions": {
                "authored_revision": self.revision,
                "index_sha256": digest(self.index_exact),
                "index_hash": digest(self.index_content),
                "shards": [
                    {
                        "path": item.name,
                        "sha256": digest(item.exact),
                        "hash": digest(item.encoded),
                    }
                    for item in self.shards
                ],
            }
        }


def load(root: Path, repository: Path, revision: str) -> Snapshot:
    """Validate the complete indexed history before selecting effective records."""
    directory = root / ENTRY
    require_directory(root, directory)
    exact, encoded = read(directory / "index.yaml", root=root)
    index = Index.model_validate_json(encoded)
    if committed(repository, revision, "authored/" + ENTRY + "/index.yaml") != exact:
        raise ValueError("Construction index differs from immutable authored revision")
    paths: set[str] = set()
    for path in directory.rglob("*"):
        check_path(root, path)
        if not path.is_dir():
            paths.add(path.relative_to(root).as_posix())
    if paths != {ENTRY + "/index.yaml", *index.includes}:
        raise ValueError(
            "Construction index must cover every file, without missing or orphan shards"
        )
    loaded: list[LoadedShard] = []
    sequences: dict[tuple[str, str], list[int]] = {}
    for name, expected in sorted(index.includes.items()):
        match = _PATH.fullmatch(name)
        if match is None:
            raise ValueError("Unsupported or unsafe construction shard path")
        area, region, sequence = match.groups()
        number = int(sequence)
        if sequence != f"{number:03d}":
            raise ValueError("Construction shard sequence must be canonical")
        sequences.setdefault((area, region), []).append(number)
        raw, content = read(root / name, root=root)
        if (
            digest(content) != expected
            or committed(repository, revision, "authored/" + name) != raw
        ):
            raise ValueError("Construction shard hash or immutable bytes mismatch")
        shard = Shard.model_validate_json(content)
        _envelope(shard, area, region)
        loaded.append(LoadedShard(name, raw, content, shard))
    if any(values != list(range(1, len(values) + 1)) for values in sequences.values()):
        raise ValueError("Construction shard sequences must start at 001 without gaps")
    return Snapshot(revision, exact, encoded, tuple(loaded), _current(tuple(loaded)))


def _current(loaded: tuple[LoadedShard, ...]) -> tuple[Record, ...]:
    chains: dict[bytes, tuple[Record, str]] = {}
    keys: set[str] = set()
    for item in loaded:
        decision = item.shard.decisions[0]
        for record in item.shard.records:
            if record.record_key in keys:
                raise ValueError("Duplicate construction adoption record")
            keys.add(record.record_key)
            subject = canonical(
                [record.kind, record.data.subject.model_dump(mode="json")]
            )
            previous = chains.get(subject)
            if previous is not None and previous[0].filing_key != record.filing_key:
                raise ValueError("Construction subject cannot move between regions")
            expected_no = 1 if previous is None else previous[0].data.adoption_no + 1
            if record.data.adoption_no != expected_no:
                raise ValueError("Construction adoption history must be contiguous")
            predecessor = record.data.predecessor
            if previous is None:
                if predecessor is not None:
                    raise ValueError("Initial adoption cannot have a predecessor")
            elif predecessor is None or predecessor.model_dump(mode="json") != {
                "record_key": previous[0].record_key,
                "record_hash": digest(canonical(previous[0].model_dump(mode="json"))),
                "decision_id": previous[1],
            }:
                raise ValueError(
                    "Construction predecessor does not name the exact prior adoption"
                )
            chains[subject] = record, decision.id
    return tuple(
        sorted(
            (record for record, _ in chains.values() if record.data.value is not None),
            key=lambda record: record.record_key,
        )
    )


def _envelope(  # ruff: ignore[complex-structure, too-many-branches] -- One envelope binds the complete decision, records and reviews.
    shard: Shard, area: str, region: str
) -> None:
    kinds = {
        "profiles": "construction_profile",
        "restrictions": "construction_restriction",
        "coverage": "construction_coverage",
    }
    ordered(shard.review_context.source_batches)
    decision = shard.decisions[0]
    expected_members = tuple(
        sorted(
            (record.record_key, digest(canonical(record.model_dump(mode="json"))))
            for record in shard.records
        )
    )
    membership_hash = digest(canonical([list(pair) for pair in expected_members]))
    if (
        decision.members != expected_members  # ruff: ignore[too-many-boolean-expressions] -- Bind every field to the same complete decision.
        or decision.membership_hash != membership_hash
        or decision.id != "d:" + membership_hash[7:]
        or shard.default_decision_id != decision.id
        or decision.sample_ids != tuple(key for key, _ in expected_members)
        or decision.category != kinds[area]
    ):
        raise ValueError(
            "Human confirmed decision must cover exactly the full category batch"
        )
    context_hash = digest(canonical(shard.review_context.model_dump(mode="json")))
    batches = {item.batch_id for item in shard.review_context.source_batches}
    for record in shard.records:
        if record.kind != kinds[area] or record.filing_key != region + "-standard":
            raise ValueError(
                "Construction record kind or region differs from its shard"
            )
        key = canonical(
            [
                record.kind,
                record.data.subject.model_dump(mode="json"),
                record.data.adoption_no,
            ]
        ).decode()
        if record.record_key != key or record.data.review_context_hash != context_hash:
            raise ValueError("Construction record key or review context mismatch")
        ordered(record.evidence)
        ordered(record.data.dependencies)
        if sum(item.primary for item in record.evidence) != 1:
            raise ValueError(
                "Construction adoption requires exactly one primary evidence"
            )
        if len(
            {canonical(item.use.model_dump(mode="json")) for item in record.evidence}
        ) != len(record.evidence):
            raise ValueError("Construction source uses must be unique within a record")
        role = {
            "profiles": "profile",
            "restrictions": "restriction",
            "coverage": "coverage",
        }[area]
        if next(item.role for item in record.evidence if item.primary) != role:
            raise ValueError(
                "Primary construction evidence must directly support its category"
            )
        if any(
            item.use.source.archive.batch_id not in batches
            or item.use.usage != record.kind
            or item.role not in {role, "reconciliation"}
            for item in record.evidence
        ):
            raise ValueError(
                "Construction evidence kind, role or source batch mismatch"
            )
        if any(item.region != region for item in record.data.dependencies):
            raise ValueError("Construction dependencies cannot cross regions")
        current_basis = basis(record)
        if record.review.basis_hash != current_basis:
            raise ValueError("Construction review must cover the final basis")
        reviews = record.review.model_reviews
        if tuple(
            (review.vendor, review.model, review.version) for review in reviews
        ) != tuple(
            sorted((review.vendor, review.model, review.version) for review in reviews)
        ) or {review.vendor for review in reviews} != {"anthropic", "openai"}:
            raise ValueError(
                "Construction review requires two distinct vendors in canonical order"
            )
        for review in reviews:
            if (
                review.vendor,
                review.model,
                review.version,
            ) not in MODEL_IDENTITIES or review.basis_hash != current_basis:
                raise ValueError(
                    "Construction review tool identity or final basis mismatch"
                )
        events = (
            *record.review.maintainer_samples,
            *(
                (record.review.maintainer_resolution,)
                if record.review.maintainer_resolution is not None
                else ()
            ),
        )
        if not events or any(
            event.basis_hash != current_basis or event.outcome != "adopted"
            for event in events
        ):
            raise ValueError(
                "Human confirmed adoption needs a genuine adopted event for its final basis"
            )
        if any(
            token in event.reviewed_by.lower()
            for event in events
            for token in ("bot", "claude", "codex", "gpt", "model", "模型")
        ):
            raise ValueError("A model cannot sign a maintainer event")
        if (
            any(review.outcome == "disputed" for review in reviews)
            and record.review.maintainer_resolution is None
        ):
            raise ValueError("Disputed adoption requires maintainer resolution")
        if (
            isinstance(record, ProfileRecord)
            and record.data.value is not None
            and (
                record.data.value.region != region
                or record.data.value.name.lang != ("ja" if region == "jp" else "en")
                or not record.data.value.name.text.strip()
            )
        ):
            raise ValueError("Profile region or language differs from shard")
