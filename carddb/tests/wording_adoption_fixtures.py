"""Synthetic immutable Git, frozen HTML and exact approved-rule receipts."""

import shutil
import sys
import unicodedata
from dataclasses import dataclass, field, replace
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import BuildContext
from sve_carddb.extract.compare_jp import legacy_projection
from sve_carddb.extract.official_jp import extract_card
from sve_carddb.manifest import Kind
from sve_carddb.registry.records import PrintingData
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import canonical, digest, object_value
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.wording_adoptions.importer import (
    AdoptionInputs,
    adoption_configuration,
    adoption_dependencies,
)
from sve_carddb.wording_adoptions.loader import AdoptionSnapshot, load_adoptions
from sve_carddb.wording_adoptions.models import AdoptionRecord, ReviewContext
from sve_carddb.wording_adoptions.policy import Policy, equivalent_matches
from sve_carddb.wording_adoptions.reconstruction import (
    ReconstructedScope,
    Reconstructor,
    scope_evidence,
)
from sve_carddb.wording_adoptions.replay import ReplayedAdoption, replay_adoptions

from .adoption_fixtures import git as git  # ruff: ignore[useless-import-alias] -- share the isolated synthetic Git boundary
from .test_effect_presence import page
from .test_registry import make_inputs
from .test_source_archive import _put, _resource, _store
from .text_observation_fixtures import Case, make_case

if TYPE_CHECKING:
    from sve_carddb.text_observations import Vocabulary


def commit(root: Path) -> str:
    git(root, "add", ".")
    git(
        root,
        "-c",
        "user.name=Synthetic Reviewer",
        "-c",
        "user.email=synthetic@example.invalid",
        "commit",
        "-m",
        "Synthetic immutable wording inputs",
    )
    return git(root, "rev-parse", "HEAD")


@dataclass(frozen=True)
class AdoptionCase:
    base: Case
    root: Path
    store: Path
    face_id: str
    review: ReviewContext
    newer_review: ReviewContext
    snapshot: AdoptionSnapshot
    replayed: tuple[ReplayedAdoption, ...]
    scope: ReconstructedScope
    vocabulary: Vocabulary
    inputs_cache: dict[bool, AdoptionInputs] = field(
        default_factory=dict, compare=False
    )

    def inputs(self, *, newer: bool = False) -> AdoptionInputs:
        if newer in self.inputs_cache:
            return self.inputs_cache[newer].copy()
        review = self.newer_review if newer else self.review
        reconstruction = Reconstructor(self.root, {"wording-store": self.store})
        scope = reconstruction.scope(review, self.face_id, "jp")
        dependencies = adoption_dependencies(self.snapshot, self.replayed)
        dependencies.update(
            {
                name: raw
                for name, raw in scope.dependencies.items()
                if name.startswith("carddb/")
            }
        )
        key = digest(canonical(review.model_dump(mode="json"))).removeprefix("sha256:")
        dependencies.update(
            {
                f"wording-reviews/{key}/{name}": raw
                for name, raw in scope.dependencies.items()
            }
        )
        configuration = adoption_configuration(
            self.snapshot, self.replayed, self.vocabulary, (), regions=("jp",)
        )
        configuration["wording_current_review"] = review.model_dump(mode="json")
        build = BuildContext.from_inputs(
            review.context.program_revision, dependencies, configuration
        )
        result = AdoptionInputs(
            repository_root=self.root,
            authored_root=self.root / "authored",
            authored_revision=self.snapshot.authored_revision,
            registry=self.base.identity.snapshot,
            stores={"wording-store": self.store},
            build=build,
            vocabulary=self.vocabulary,
            published=(),
            current_review=review,
            regions=("jp",),
        )

        self.inputs_cache[newer] = result
        return result.copy()


def make_adoption_case(root: Path, *, human: bool = False) -> AdoptionCase:  # ruff: ignore[too-many-locals, too-many-statements] -- one shared module fixture binds separate immutable registry, program, batch and receipt inputs
    repository = root / "repository"
    repository.mkdir()
    inputs = make_inputs()
    for card in inputs.en.values():
        for face in card.faces:
            face.info["Class"] = "-"
            face.stats = {"cost": "1", "power": "1", "hp": "1"}
    for number in inputs.jp:
        raw = (
            page(
                "jp",
                '<div class="detail">'
                + ("Synthetic Alpha" if human else "Synthetic&#13;\nparagraph")
                + "</div>",
            )
            .replace(b"SYN-01", number.encode())
            .replace(b"Synthetic type", "フォロワー".encode())
        )
        inputs.jp[number] = legacy_projection(extract_card(raw, number=number))
    base = make_case(repository / "authored", inputs, regions=("jp",))
    face_id = base.plan.groups[0].face_id
    printings = tuple(
        r.data
        for r in base.identity.snapshot.records.values()
        if isinstance(r.data, PrintingData)
        and r.data.region == "jp"
        and any(m.face_id == face_id for m in r.data.source_face_map)
    )
    assert len(printings) == 2
    runtime = Path(__file__).resolve().parents[2]
    shutil.copytree(
        runtime / "carddb/src",
        repository / "carddb/src",
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc"),
    )
    for name in (
        "carddb/uv.lock",
        "carddb/pyproject.toml",
        "authored/wording-rules/145-v1.policy.yaml",
        "authored/wording-rules/145-v1.approval.yaml",
    ):
        target = repository / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(runtime / name, target)
    git(repository, "init")
    initial_revision = commit(repository)
    files = {
        name: (repository / name).read_bytes()
        for name in ("carddb/uv.lock", "carddb/pyproject.toml")
    }
    files.update(
        {
            p.relative_to(repository).as_posix(): p.read_bytes()
            for p in (repository / "carddb/src/sve_carddb").rglob("*.py")
        }
    )
    files.update(
        {"authored/ids/index.yaml": (base.root / "ids/index.yaml").read_bytes()}
    )
    files.update(
        {
            "authored/" + s.path: (base.root / s.path).read_bytes()
            for s in base.identity.snapshot.files.shards
        }
    )
    configuration: dict[str, JsonValue] = {
        "recipe": "wording-review-v1",
        "registry": {
            "authored_revision": initial_revision,
            "index_path": "authored/ids/index.yaml",
            "index_hash": digest(base.identity.snapshot.files.index_content),
        },
        "products": None,
        "product_identity": "disabled",
        "corrections": "registry-active-v1",
        "errata": "disabled",
        "errata_as_of": "2026-10-02",
        "regions": ["jp"],
        "parser": "text-observations-v1",
        "projection": "effect-presence-v1-then-source-correction-v1",
        "python_version": sys.version.split()[0],
        "unicode_version": unicodedata.unidata_version,
    }
    context = BuildContext.from_inputs(initial_revision, files, configuration)
    store = replace(_store(root / "historical"), store_id="wording-store")
    batches = []
    for generation in (0, 1, 2):
        for i, printing in enumerate(printings):
            text = (
                "Synthetic Alpha"
                if human and (generation == 0 or i == 0)
                else "Synthetic Beta"
                if human
                else "Synthetic&#13;\nparagraph"
                if generation == 0 or i == 0
                else "Synthetic\nparagraph"
            )
            raw = (
                page("jp", '<div class="detail">' + text + "</div>")
                .replace(b"SYN-01", printing.card_no.encode())
                .replace(b"Synthetic type", "フォロワー".encode())
            )
            if generation == 2 and i == 1:
                raw = raw.replace(
                    b"</body>", b"<!-- synthetic new source version --></body>"
                )
            _put(
                store,
                _resource(card_url(printing.card_no), f"raw/{i}.html", raw, Kind.CARD),
                raw,
            )
        batches.append(seal_batch(store).batch_id)
    reviews = tuple(
        ReviewContext.model_validate_json(
            canonical(
                {
                    "context": context.model_dump(mode="json"),
                    "source_batches": [{"batch_id": b}],
                }
            )
        )
        for b in batches
    )
    reconstruction = Reconstructor(repository, {"wording-store": store.root})
    before = reconstruction.scope(reviews[0], face_id, "jp")
    scope = reconstruction.scope(reviews[1], face_id, "jp")
    assert len(before.observations) == 2
    assert len(scope.observations) == 3
    old_selected = min(
        before.contents,
        key=lambda k: (
            before.contents[k].card.source.id,
            before.contents[k].printing_id,
        ),
    )
    levels = (
        tuple(
            tuple(
                o.observation_key
                for o in scope.observations
                if ("Alpha" in (scope.contents[o.observation_key].content.effect or ""))
                == old
            )
            for old in (True, False)
        )
        if human
        else (tuple(o.observation_key for o in scope.observations),)
    )
    selected = levels[-1][0]

    policy_raw = read_yaml(repository / "authored/wording-rules/145-v1.policy.yaml")
    policy = Policy.model_validate_json(canonical(policy_raw))
    matches = (
        ()
        if human
        else equivalent_matches(
            policy,
            {k: i.content for k, i in scope.contents.items()},
            selected,
            region="jp",
            previous=before.contents[old_selected].content,
        )
    )
    order_answer: dict[str, JsonValue] = {
        "reviewed_by": "Synthetic order reviewer",
        "reviewed_at": "2026-10-02T00:00:00Z",
        "reviewed_precision": "day",
        "before_observation_keys": list(levels[0]),
        "after_observation_keys": list(levels[-1]),
        "note": "Synthetic explicit adoption order; not an official date.",
    }
    evidence = tuple(
        sorted(
            {*scope_evidence(before), *scope_evidence(scope)},
            key=lambda e: canonical(e.model_dump(mode="json")),
        )
    )
    record = AdoptionRecord.model_validate_json(
        canonical(
            {
                "record_key": canonical(
                    ["wording_adoption", face_id, "jp", 1]
                ).decode(),
                "kind": "wording_adoption",
                "filing_key": "jp",
                "data": {
                    "face_id": face_id,
                    "region": "jp",
                    "adoption_no": 1,
                    "review_context": reviews[1].model_dump(mode="json"),
                    "observations": [
                        o.model_dump(mode="json") for o in scope.observations
                    ],
                    "observations_hash": digest(
                        canonical(
                            [o.model_dump(mode="json") for o in scope.observations]
                        )
                    ),
                    "checked_observation_keys": [
                        o.observation_key for o in scope.observations
                    ],
                    "previous": {
                        "kind": "mechanical",
                        "review_context": reviews[0].model_dump(mode="json"),
                        "observations": [
                            o.model_dump(mode="json") for o in before.observations
                        ],
                        "observations_hash": digest(
                            canonical(
                                [o.model_dump(mode="json") for o in before.observations]
                            )
                        ),
                        "selected_observation_key": old_selected,
                    },
                    "equivalence": "equivalent",
                    "review": {
                        "mode": "approved_rules",
                        "rule_set": {
                            "policy_id": policy.policy_id,
                            "authored_revision": initial_revision,
                            "path": "authored/wording-rules/145-v1.policy.yaml",
                            "hash": digest(canonical(policy_raw)),
                            "approval_receipt_hash": "sha256:a3b941d010708dac6f295ad563b0954bf1d4d7608d0ae77319795c6fc80e8198",
                        },
                        "rule_matches": [m.model_dump(mode="json") for m in matches],
                    },
                    "wording_order": [list(level) for level in levels],
                    "order_evidence": (
                        [
                            {
                                "before_level": 0,
                                "after_level": 1,
                                "basis": "reviewed_order",
                                "evidence_indexes": [],
                                "review_receipt": order_answer,
                            }
                        ]
                        if human
                        else []
                    ),
                    "selected_observation_key": selected,
                    "previous_order": {
                        "basis": "reviewed_order" if human else "same_content",
                        "evidence_indexes": [],
                        "review_receipt": order_answer
                        | {
                            "before_observation_keys": [old_selected],
                            "after_observation_keys": [selected],
                        }
                        if human
                        else None,
                    },
                },
                "evidence": [e.model_dump(mode="json") for e in evidence],
            }
        )
    )
    if human:
        data = record.data.model_copy(
            update={
                "review": record.data.review.model_copy(
                    update={"mode": "human", "rule_set": None, "rule_matches": ()}
                )
            }
        )
        record = record.model_copy(update={"data": data})
    install_adoptions(base.root, [record])
    revision = commit(repository)
    snapshot = load_adoptions(
        base.root,
        authored_revision=revision,
        registry=base.identity.snapshot,
        stores={"wording-store": store.root},
    )
    replayed = replay_adoptions(
        snapshot, Reconstructor(repository, {"wording-store": store.root})
    )
    return AdoptionCase(
        base,
        repository,
        store.root,
        face_id,
        reviews[1],
        reviews[2],
        snapshot,
        replayed,
        scope,
        base.vocabulary,
    )


def install_adoptions(
    root: Path,
    records: list[AdoptionRecord],
    *,
    sequence: str = "001",
    region: str = "jp",
) -> None:
    ordered = sorted(records, key=lambda r: r.record_key)
    wire: list[JsonValue] = [r.model_dump(mode="json") for r in ordered]
    members: list[JsonValue] = [
        [r.record_key, digest(canonical(r.model_dump(mode="json")))] for r in ordered
    ]
    checksum = digest(canonical(members))
    identifier = "d:" + checksum.removeprefix("sha256:")
    shard: dict[str, JsonValue] = {
        "wording_adoption_format": 1,
        "kind": "wording_adoption_shard",
        "default_decision_id": identifier,
        "records": wire,
        "decisions": [
            {
                "id": identifier,
                "state": "confirmed",
                "scope": "batch",
                "category": "wording_adoption",
                "policy_id": "wording-adoption-v1",
                "membership_hash": checksum,
                "members": members,
                "sample_ids": [r.record_key for r in ordered],
                "authored_by": "Synthetic tool",
                "authored_at": "2026-10-02T01:00:00Z",
                "reviewed_by": "gbaian10",
                "reviewed_at": "2026-10-02T00:00:00Z",
                "reviewed_precision": "day",
                "note": "政策核可；synthetic policy application, not per-card human review.",
            }
        ],
    }
    directory = root / "wording-adoptions" / region
    directory.mkdir(parents=True, exist_ok=True)
    (directory / (sequence + ".yaml")).write_bytes(canonical(shard))
    index = directory.parent / "index.yaml"
    includes = {} if not index.exists() else object_value(read_yaml(index))["includes"]
    assert isinstance(includes, dict)
    includes["wording-adoptions/" + region + "/" + sequence + ".yaml"] = digest(
        canonical(shard)
    )
    (directory.parent / "index.yaml").write_bytes(
        canonical(
            {
                "wording_adoption_format": 1,
                "kind": "wording_adoption_index",
                "includes": includes,
            }
        )
    )
