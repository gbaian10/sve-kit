"""Replay frozen sources before resolving individual role issues; retain all other causes."""

import tempfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import BuildContext
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.template_parameter_rules.legacy import (
    assert_restriction,
    historical_role,
)
from sve_carddb.template_parameter_rules.loader import Loaded, load_config
from sve_carddb.template_parameter_rules.models import LEGACY_IDS
from sve_carddb.template_parameter_rules.repository import git
from sve_carddb.template_parameter_rules.repository import revision as verify_revision
from sve_carddb.template_parameters.analysis import (
    VERSION_PARAMETERS,
    Position,
    prepared,
)
from sve_carddb.template_parameters.inventory import Candidates, build
from sve_carddb.template_parameters.numeric_rules import NUMERIC_RULE_PENDING
from sve_carddb.template_parameters.references import adopted
from sve_carddb.template_parameters.rule_candidates import BY_ID
from sve_carddb.template_sources.checkpoint import compare, parse_legacy
from sve_carddb.template_sources.inventory import Scan, coverage, fields, scan_batch
from sve_carddb.template_sources.normalizer import partition
from sve_carddb.template_sources.pins import PARSER, recipes
from sve_carddb.text_observations.vocabulary import Vocabulary
from sve_carddb.translations.sources import CODE_PATH as TRANSLATION_CODE
from sve_carddb.translations.sources import RUNTIME as TRANSLATION_RUNTIME
from sve_carddb.translations.sources import Sources, project

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_sources import PinnedRepository
    from sve_carddb.template_parameters.models import Candidate, Hint
    from sve_carddb.template_parameters.references import References
    from sve_carddb.template_sources.models import Recipe

CODE_PATH = "carddb/src/sve_carddb/template_parameter_rules/replay.py"
JP_BATCH = "sha256:2bf20b2ff9ae9e21be688dbf5a91d4cd7803fa0e253206ecbdcb55e334cbe9c4"


@dataclass(frozen=True)
class ProposalInputs:
    vocabulary: bytes
    basis: bytes


def _vocabulary(
    refs: References, recipe: Recipe, proposals: ProposalInputs | None
) -> None:
    pins = object_value(recipe.config["references"])
    declared = any(
        key in pins for key in ("vocabulary_proposals_hash", "vocabulary_basis_hash")
    )
    if proposals is None:
        if declared:
            raise ValueError(
                "Recognition replay cannot omit pinned vocabulary proposal inputs"
            )
        return
    if pins.get("vocabulary_proposals_hash") != digest(
        proposals.vocabulary
    ) or pins.get("vocabulary_basis_hash") != digest(proposals.basis):
        raise ValueError(
            "Recognition vocabulary proposal or basis hash differs from its recipe"
        )
    refs.vocabulary = Vocabulary.model_validate_json(proposals.vocabulary)
    refs.vocabulary.verify()


@dataclass(frozen=True)
class Replay:
    policy_pin: bytes | None
    source_coverage: bytes
    checkpoint: bytes
    numeric_positions: tuple[bytes, ...]
    fingerprints: tuple[bytes, ...]
    resolved_slots: tuple[bytes, ...]
    remaining_slots: tuple[bytes, ...]
    recipe: bytes

    @property
    def complete(self) -> bool:
        """Successful matches cannot conceal unknown sources or unrelated pending slots."""
        return (
            object_value(parse(self.source_coverage))["complete"] is True
            and not self.remaining_slots
        )


def _evidence(
    repository: PinnedRepository, recipe: Recipe, stores: dict[str, Path]
) -> Sources:
    dependencies = repository.read_many(recipe.code_revision, TRANSLATION_RUNTIME)
    parsers: dict[str, JsonValue] = {}
    for provider in ("jp", "sv1", "svwb"):
        config: dict[str, JsonValue] = {"provider": provider}
        parsers["translation-" + provider + "-v1"] = {
            "version": "translation-" + provider + "-v1",
            "program_revision": recipe.code_revision,
            "code_path": TRANSLATION_CODE,
            "code_hash": digest(dependencies[TRANSLATION_CODE]),
            "config": config,
            "config_hash": digest(canonical(config)),
        }
    context = BuildContext.from_inputs(
        recipe.code_revision, dependencies, {"translation_recipes": parsers}
    )
    return Sources(stores, repository.root, context)


def _references(
    repository: PinnedRepository, recipe: Recipe, stores: dict[str, Path]
) -> References:
    pin = object_value(object_value(recipe.config.get("references")).get("glossary"))
    revision = pin.get("authored_revision")
    if not isinstance(revision, str):
        raise TypeError("Recognition glossary requires its immutable authored revision")
    index_path = "authored/translations/index.yaml"
    expected = {index_path: pin.get("index_hash")}
    for value in array(pin.get("shards")):
        shard = object_value(value)
        name = str(shard.get("path"))
        path = PurePosixPath(name)
        if (
            not name.startswith("translations/")
            or ".." in path.parts
            or path.as_posix() != name
            or name == "translations/index.yaml"
        ):
            raise ValueError("Recognition glossary pin contains an unsafe shard")
        if "authored/" + name in expected:
            raise ValueError("Recognition glossary pin contains duplicate shards")
        expected["authored/" + name] = shard.get("exact_hash")
    content = _glossary_files(repository, revision, expected)
    with tempfile.TemporaryDirectory(prefix="recognition-glossary-") as folder:
        root = Path(folder)
        for name, raw in content.items():
            target = root / name.removeprefix("authored/")
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(raw)
        refs = adopted(root, _evidence(repository, recipe, stores))
    actual = object_value(refs.pins["glossary"])
    if canonical({**actual, "authored_revision": revision}) != canonical(pin):
        raise ValueError("Recognition glossary canonical index or shard pins differ")
    if refs.pins["exact_concepts_hash"] != object_value(
        recipe.config["references"]
    ).get("exact_concepts_hash"):
        raise ValueError(
            "Recognition glossary exact concept bindings differ from the recipe"
        )
    return refs


def numeric_identity(candidate: Candidate, hint: Hint, rule: str) -> bytes:
    """Counts cannot prove stability of ownership, coordinates, raw spelling and values."""
    return canonical(
        {
            "inventory_id": candidate.inventory_id,
            "slot": hint.name,
            "source_segments": [
                s.model_dump(mode="json") for s in hint.source_segments
            ],
            "numeric_rule": rule,
            "value": hint.value,
            "raw_hash": hint.raw_hash,
        }
    )


def _historical_positions(
    sources: FrozenSources, scan: Scan, candidates: Candidates
) -> tuple[bytes, ...]:
    by_id = {item.id: item for item in scan.entries}
    by_field: dict[tuple[str, str], list[Candidate]] = {}
    for candidate in candidates.entries:
        ref = by_id[candidate.inventory_id].source_ref
        by_field.setdefault((ref.source_version_id, ref.locator), []).append(candidate)
    old = []
    for current in sources.inventory.current:
        source, raw, _ = sources.read(current.source_version_id, parser_version=PARSER)
        _, document = project(raw, source.url, "jp")
        for locator, text, section in fields(document):
            if text is None:
                continue
            parts = partition(text, section=section)
            for candidate in by_field.get((source.id, locator), ()):
                selected = [
                    p
                    for p in parts
                    if tuple((s.start, s.end) for s in p.segments)
                    == tuple((s.start, s.end) for s in candidate.source_span.segments)
                    and p.role == candidate.source_span.role
                ]
                if len(selected) != 1:
                    raise ValueError(
                        "Recognition historical replay cannot locate an exact source part"
                    )
                template, _units = prepared(text, selected[0])
                for hint in candidate.slots:
                    if hint.semantic_role != "numeric":
                        continue
                    position = Position(
                        hint.occurrence.start,
                        hint.occurrence.end,
                        hint.transformation,
                        hint.semantic_role,
                    )
                    assert_restriction(
                        template.normalized,
                        position,
                        reminder=candidate.source_span.role == "reminder",
                    )
                    rule, _issues = historical_role(template.normalized, position)
                    if rule is not None:
                        old.append(numeric_identity(candidate, hint, rule))
    return tuple(sorted(old))


def _resolve(
    loaded: Loaded | None, candidates: Candidates
) -> tuple[tuple[bytes, ...], tuple[bytes, ...]]:
    rules = {} if loaded is None else {r.rule_id: r for r in loaded.policy.rules}
    matches = {
        (str(row["inventory_id"]), str(row["slot"])): row
        for row in candidates.rule_matches
    }
    if len(matches) != len(candidates.rule_matches):
        raise ValueError("Recognition matches must have unique slot ownership")
    resolved, remaining = [], []
    for candidate in candidates.entries:
        unresolved = set(candidate.issues) - {
            reason for hint in candidate.slots for reason in hint.issues
        }
        for hint in candidate.slots:
            issues = set(hint.issues)
            key = (candidate.inventory_id, hint.name)
            rule_id: str | None = hint.numeric_rule
            match = matches.get(key)
            if match is not None:
                if rule_id is not None:
                    raise ValueError(
                        "Recognition new rules cannot claim old numeric ownership"
                    )
                rule_id = str(match["rule_id"])
            allowed = rules.get(rule_id) if rule_id is not None else None
            if (
                allowed is not None
                and candidate.source_span.role
                in (loaded.policy.scope.roles if loaded else ())
                and (hint.value is not None or match is not None)
                and "invalid_safe_unsigned_decimal" not in issues
            ):
                reason = (
                    NUMERIC_RULE_PENDING
                    if rule_id in LEGACY_IDS
                    else BY_ID[str(rule_id)].reason
                )
                if reason not in issues:
                    raise ValueError(
                        "Recognition matched slot lacks its exact pending reason"
                    )
                issues.remove(reason)
                resolved.append(
                    canonical(
                        {
                            "inventory_id": candidate.inventory_id,
                            "slot": hint.name,
                            "rule_id": rule_id,
                            "recognized_role": allowed.recognized_role,
                            "raw_hash": hint.raw_hash,
                            "source_segments": [
                                s.model_dump(mode="json") for s in hint.source_segments
                            ],
                            "remaining_issues": list[JsonValue](sorted(issues)),
                            "match_evidence": match,
                            "value": hint.value,
                            "normalized_occurrence": hint.occurrence.model_dump(
                                mode="json"
                            ),
                        }
                    )
                )
            if issues:
                remaining.append(
                    canonical(
                        {
                            "inventory_id": candidate.inventory_id,
                            "slot": hint.name,
                            "issues": list[JsonValue](sorted(issues)),
                        }
                    )
                )
        if unresolved:
            remaining.append(
                canonical(
                    {
                        "inventory_id": candidate.inventory_id,
                        "slot": None,
                        "issues": list[JsonValue](sorted(unresolved)),
                    }
                )
            )
    return tuple(sorted(resolved)), tuple(sorted(remaining))


def replay(
    repository: PinnedRepository,
    recipe: Recipe,
    stores: dict[str, Path],
    *,
    main_revision: str,
    legacy_bytes: bytes,
    proposals: ProposalInputs | None = None,
) -> Replay:
    """Re-read source/glossary closures; caller hints and local worktree files supply no proof."""
    source_pins = _recipe(repository, recipe)
    loaded = load_config(recipe.config, repository, main_revision=main_revision)
    batch = object_value(recipe.config.get("source_batch"))
    if (
        set(batch) != {"store_id", "batch_id"}
        or not isinstance(batch.get("store_id"), str)
        or not isinstance(batch.get("batch_id"), str)
    ):
        raise ValueError("Recognition replay requires one explicit frozen source batch")
    if loaded is not None and canonical(batch) != canonical(
        loaded.policy.scope.source_batches[0].model_dump(mode="json")
    ):
        raise ValueError("Recognition frozen batch differs from the policy scope")
    store = stores.get(str(batch["store_id"]))
    if store is None:
        raise ValueError("Recognition frozen source store is not configured")
    if digest(legacy_bytes) != recipe.config.get("legacy_file_hash"):
        raise ValueError("Recognition legacy baseline differs from its recipe hash")
    sources = FrozenSources(store, str(batch["store_id"]), str(batch["batch_id"]))
    refs = _references(repository, recipe, stores)
    _vocabulary(refs, recipe, proposals)
    scan = scan_batch(sources, repository=repository.root, pins=source_pins)
    enabled = (
        tuple(r.rule_id for r in loaded.policy.rules if r.rule_id in BY_ID)
        if loaded
        else ()
    )
    candidates = build(sources, scan, refs, enabled_rules=enabled)
    current = tuple(
        sorted(
            numeric_identity(c, h, h.numeric_rule)
            for c in candidates.entries
            for h in c.slots
            if h.numeric_rule is not None
        )
    )
    if (
        loaded is not None
        and loaded.historical_revision is not None
        and _historical_positions(sources, scan, candidates) != current
    ):
        raise ValueError(
            "Recognition sign restriction changes an original numeric position"
        )
    checkpoint = compare(scan, parse_legacy(legacy_bytes))
    if (
        object_value(checkpoint["fingerprints"])["additional_ids"]
        or object_value(checkpoint["legacy_member_coverage"])["additional_members"]
    ):
        raise ValueError(
            "Recognition legacy baseline cannot omit source fingerprints or uses"
        )
    if any(
        object_value(checkpoint[key])["complete"] is not True
        for key in ("fingerprints", "legacy_member_coverage")
    ):
        raise ValueError(
            "Recognition legacy fingerprints and full use set must reproduce exactly"
        )
    if sources.batch_id == JP_BATCH and (
        len(current),
        object_value(checkpoint["fingerprints"])["expected"],
        object_value(checkpoint["legacy_member_coverage"])["expected"],
    ) != (14782, 3669, 13913):
        raise ValueError(
            "Recognition first JP batch differs from its fixed baseline counts"
        )
    resolved, remaining = _resolve(loaded, candidates)
    fingerprints = tuple(
        sorted(
            canonical([o.template, o.member_hash, digest(o.normalized.encode())])
            for o in scan.occurrences
        )
    )
    return Replay(
        None if loaded is None else canonical(loaded.pin.model_dump(mode="json")),
        canonical(coverage(scan)),
        canonical(checkpoint),
        current,
        fingerprints,
        resolved,
        remaining,
        canonical(recipe.model_dump(mode="json")),
    )


def compare_replays(previous: Replay, current: Replay) -> None:
    """Recipe or glossary upgrades must preserve identities, not merely usage totals."""
    prior_batch = object_value(object_value(parse(previous.recipe))["config"])[
        "source_batch"
    ]
    next_batch = object_value(object_value(parse(current.recipe))["config"])[
        "source_batch"
    ]
    if canonical(prior_batch) != canonical(next_batch):
        raise ValueError("Recognition replay comparison requires the same frozen batch")
    if previous.numeric_positions != current.numeric_positions:
        raise ValueError("Recognition replay changed an existing numeric position")
    if previous.fingerprints != current.fingerprints:
        raise ValueError(
            "Recognition replay changed a legacy fingerprint or source use"
        )


def _glossary_files(
    repository: PinnedRepository, revision: str, expected: dict[str, JsonValue]
) -> dict[str, bytes]:
    # Git modes are checked for the entire glossary closure, including parent symlinks.
    verify_revision(repository, revision)
    rows = git(repository, "ls-tree", "-r", "-z", revision, "--", "authored").split(
        b"\0"
    )
    names = set()
    for row in rows:
        if not row:
            continue
        header, _, encoded = row.partition(b"\t")
        name = encoded.decode("utf-8")
        if name not in {"authored", "authored/translations"} and not name.startswith(
            "authored/translations/"
        ):
            continue
        if not header.startswith(b"100644 blob "):
            raise ValueError("Recognition glossary Git input is not a regular file")
        names.add(name)
    if names != set(expected):
        raise ValueError(
            "Recognition glossary pin must cover its complete Git file closure"
        )
    content = repository.read_many(revision, tuple(sorted(expected)))
    if any(digest(raw) != expected[name] for name, raw in content.items()):
        raise ValueError(
            "Recognition glossary exact bytes differ from their revision pin"
        )
    return content


def _recipe(repository: PinnedRepository, recipe: Recipe) -> tuple[Recipe, ...]:
    if (
        recipe.id != VERSION_PARAMETERS
        or recipe.code_path != CODE_PATH
        or digest(canonical(recipe.config)) != recipe.config_hash
    ):
        raise ValueError(
            "Recognition parameter recipe ID, program or config hash is invalid"
        )
    content = repository.read(recipe.code_revision, CODE_PATH)
    if digest(content) != recipe.code_hash or content != Path(__file__).read_bytes():
        raise ValueError(
            "Recognition replay implementation differs from its immutable recipe"
        )
    source_pins = recipes(repository.root, recipe.code_revision)
    if canonical(recipe.config.get("source_recipes")) != canonical(
        [p.model_dump(mode="json") for p in source_pins]
    ):
        raise ValueError("Recognition replay requires its complete source recipe pins")
    return source_pins
