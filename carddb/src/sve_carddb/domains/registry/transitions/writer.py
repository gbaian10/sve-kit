"""Plan explicit human mapping-review decisions and append one immutable transition."""

import io
import os
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

from pydantic import Field, model_validator
from ruamel.yaml import YAML

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import RecordData, Text
from sve_carddb.core.provenance import BuildContext, Revision
from sve_carddb.domains.registry.preview.review_inputs import ReviewInputPin
from sve_carddb.domains.registry.records import ArtData, CardId, MappingReviewData
from sve_carddb.domains.registry.storage import Entry, RegistryFiles, read_base_files
from sve_carddb.domains.registry.transitions.files import MAX_BYTES, LoadedShard
from sve_carddb.domains.registry.transitions.history import git
from sve_carddb.domains.registry.transitions.loader import load_transitions
from sve_carddb.domains.registry.transitions.models import (
    After,
    Batch,
    RegistryBasis,
    ReviewContext,
    Shard,
    Transition,
    Update,
)
from sve_carddb.domains.registry.transitions.renewal import (
    load_renewed_registry,
    reference,
    tail,
)
from sve_carddb.domains.registry.transitions.review_sources import (
    Artifact,
    Configuration,
    RegionalSource,
    ReviewSources,
    printings,
)


class Recipe(RecordData):
    repo: Path
    archive: Path
    store_id: Text
    sources: tuple[RegionalSource, ...]
    registry_review_inputs: ReviewInputPin
    program_revision: Revision

    @model_validator(mode="after")
    def _paths(self) -> Recipe:
        if not self.repo.is_absolute() or not self.archive.is_absolute():
            raise ValueError("Registry renewal runtime paths must be absolute")
        if tuple(s.region for s in self.sources) != ("en", "jp"):
            raise ValueError("Registry renewal sources must be EN then JP")
        return self

    def evidence(self) -> ReviewSources:
        """Keep runtime filesystem roots outside the authored context."""
        return ReviewSources(
            self.repo,
            self.archive,
            self.store_id,
            self.registry_review_inputs.root,
        )


class Decisions(RecordData):
    card_ids: tuple[CardId, ...] = Field(min_length=1)
    conclusion: Literal["confirmed_none"]
    reason: Text

    @model_validator(mode="after")
    def _cards(self) -> Decisions:
        if (
            self.card_ids != tuple(sorted(set(self.card_ids)))
            or not self.reason.strip()
        ):
            raise ValueError(
                "Renewal decisions require sorted unique cards and a reason"
            )
        return self


class ArtDecisions(RecordData):
    arts: tuple[ArtData, ...] = Field(min_length=1)
    reason: Text

    @model_validator(mode="after")
    def _arts(self) -> ArtDecisions:
        ids = tuple(art.id for art in self.arts)
        if ids != tuple(sorted(set(ids))) or not self.reason.strip():
            raise ValueError("Art decisions require sorted unique art IDs and a reason")
        return self


@dataclass(frozen=True)
class Plan:
    shard: LoadedShard | None
    reviewed_records: int


def _wire(record: Transition) -> LoadedShard:
    model = Shard(format=1, kind="identity_transition_shard", records=(record,))
    value = model.model_dump(mode="json", exclude_computed_fields=True)
    output = io.StringIO()
    yaml = YAML()
    yaml.default_flow_style = False
    yaml.indent(mapping=2, sequence=4, offset=2)
    yaml.dump(value, output)
    exact = output.getvalue().encode()
    if len(exact) >= MAX_BYTES:
        raise ValueError(
            "Renewal shard is too large; explicitly review smaller card groups"
        )
    content = canonical(value)
    return LoadedShard(
        f"identity-transitions/{record.sequence:03}.yaml",
        digest(content),
        content,
        exact,
    )


def _basis(
    recipe: Recipe, sources: ReviewSources
) -> tuple[RegistryFiles, RegistryBasis]:
    root = recipe.repo / "authored"
    files = read_base_files(root)
    revision = git(recipe.repo, "rev-parse", "HEAD").decode().strip()
    basis = RegistryBasis(
        authored_revision=revision,
        index_path="ids/index.yaml",
        index_hash=digest(canonical(files.index().model_dump(mode="json"))),
    )
    committed = sources.history.registry(basis)
    if files != committed:
        raise ValueError("Renewal registry basis must match committed exact Git bytes")
    changed = git(
        recipe.repo,
        "diff",
        recipe.program_revision,
        "--",
        "carddb/src",
        "carddb/uv.lock",
        "carddb/pyproject.toml",
    )
    if changed:
        raise ValueError(
            "Renewal program and dependencies must match their committed revision"
        )
    if git(
        recipe.repo,
        "status",
        "--porcelain",
        "--untracked-files=all",
        "--",
        "carddb/src",
        "carddb/uv.lock",
        "carddb/pyproject.toml",
    ):
        raise ValueError("Renewal program worktree must be committed and clean")
    if (root / "ids/index.yaml").read_bytes() != git(
        recipe.repo, "show", revision + ":authored/ids/index.yaml"
    ):
        raise ValueError("Renewal index must match committed exact Git bytes")
    return files, basis


def _latest(
    files: RegistryFiles, shards: tuple[LoadedShard, ...]
) -> dict[str, tuple[Entry, str | None]]:
    latest: dict[str, tuple[Entry, str | None]] = {
        entry.record_key: (entry, None)
        for shard in files.shards
        for entry in shard.envelope().records
    }
    for shard in shards:
        record = shard.envelope().records[0]
        for update in record.updates:
            if update.after is None:
                raise ValueError("Only complete confirmed-none renewals are supported")
            latest[update.target_key] = (
                Entry.model_validate_json(
                    canonical(update.after.model_dump(mode="json", round_trip=True))
                ),
                record.record_key,
            )
    return latest


def _art_updates(
    files: RegistryFiles,
    shards: tuple[LoadedShard, ...],
    decisions: ArtDecisions,
) -> tuple[Update, ...]:
    latest = _latest(files, shards)
    updates = []
    for art in decisions.arts:
        key = "art:" + art.id
        if key not in latest:
            raise ValueError("Art extension decision names an unregistered art")
        old, producer = latest[key]
        if old.data != art.model_dump(mode="json"):
            updates.append(
                Update(
                    target_key=key,
                    before=reference(old, producer),
                    allocation_anchor=None,
                    after=After(
                        kind="art", owner=old.owner, data=art.model_dump(mode="json")
                    ),
                )
            )
    return tuple(updates)


def _review_updates(
    files: RegistryFiles,
    shards: tuple[LoadedShard, ...],
    decisions: Decisions,
    artifact: Artifact,
) -> tuple[tuple[Update, ...], tuple[MappingReviewData, ...]]:
    latest = _latest(files, shards)
    registered = printings(files)
    updates: list[Update] = []
    reviews: list[MappingReviewData] = []
    for card_id in decisions.card_ids:
        key = "region_mapping_review:" + card_id
        if key not in latest:
            raise ValueError("Renewal decision names an unregistered absence review")
        old, producer = latest[key]
        data = MappingReviewData.model_validate_json(canonical(old.data))
        after = data.model_copy(
            update={
                "as_of": artifact.as_of,
                "coverage_scope": artifact.coverage_scope,
                "coverage_hash": artifact.coverage_hash,
                "observations": tuple(
                    sorted(
                        (p.observation for p in registered if p.card_id == card_id),
                        key=lambda obs: obs.card_no,
                    )
                ),
            }
        )
        reviews.append(after)
        if after != data:
            updates.append(
                Update(
                    target_key=key,
                    before=reference(old, producer),
                    after=After(
                        kind="region_mapping_review",
                        owner=old.owner,
                        data=after.model_dump(mode="json", round_trip=True),
                    ),
                    allocation_anchor=None,
                )
            )
    return tuple(updates), tuple(reviews)


def plan(recipe: Recipe, decisions: Decisions | ArtDecisions) -> Plan:
    """Validate the full candidate before writing; absence is never inferred by this tool."""
    root = recipe.repo / "authored"
    sources = recipe.evidence()
    files, basis = _basis(recipe, sources)
    existing = load_transitions(root)
    if isinstance(decisions, Decisions):
        updates, reviewed = _review_updates(
            files,
            existing.shards,
            decisions,
            Artifact.from_pin(recipe.registry_review_inputs),
        )
        config = Configuration(
            registry=basis,
            review_input=Artifact.from_pin(recipe.registry_review_inputs),
            sources=recipe.sources,
            dependencies=(sources.history.dependency(recipe.program_revision),),
        )
        selected = len(reviewed)
        verified = sources.verify(
            config,
            files,
            tuple(
                MappingReviewData.model_validate_json(canonical(update.after.data))
                for update in updates
                if update.after is not None
            )
            if updates
            else reviewed,
        )
    else:
        updates = _art_updates(files, existing.shards, decisions)
        config = Configuration(
            profile="registry-art-extension-v1",
            registry=basis,
            review_input=None,
            sources=recipe.sources,
            dependencies=(sources.history.dependency(recipe.program_revision),),
        )
        selected = len(decisions.arts)
        verified = sources.verify_art(
            config,
            files,
            tuple(
                ArtData.model_validate_json(canonical(update.after.data))
                for update in updates
                if update.after is not None
            )
            if updates
            else decisions.arts,
        )
    if not updates:
        load_renewed_registry(root, sources)
        return Plan(None, selected)
    record = Transition(
        kind="identity_transition",
        action="apply",
        reverts=None,
        sequence=len(existing.shards) + 1,
        previous=None
        if not existing.shards
        else tail(existing.shards[-1].envelope().records[0]),
        registry_basis=basis,
        review_context=ReviewContext(
            context=BuildContext.from_inputs(
                recipe.program_revision, config.model_dump(mode="json")
            ),
            source_batches=tuple(
                Batch(batch_id=batch)
                for batch in sorted(
                    {s.card_batch for s in recipe.sources}
                    | {
                        s.image_batch
                        for s in recipe.sources
                        if s.image_batch is not None
                    }
                )
            ),
        ),
        updates=tuple(sorted(updates, key=lambda update: update.target_key)),
        repairs=(),
        routes=(),
        evidence=verified.evidence,
        reason=decisions.reason,
    )
    candidate = _wire(record)
    load_renewed_registry(root, sources, candidate=candidate)
    return Plan(candidate, selected)


def write(root: Path, planned: Plan) -> None:
    """Install only a fully validated shard under the caller's global registry lock."""
    if planned.shard is None:
        return
    destination = root / planned.shard.path
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        raise ValueError("Renewal transition already exists; refusing to overwrite")
    with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as stream:
        temporary = Path(stream.name)
        stream.write(planned.shard.exact_content)
        stream.flush()
        os.fsync(stream.fileno())
    try:
        temporary.replace(destination)
        directory = os.open(destination.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        temporary.unlink(missing_ok=True)
