"""Validate an entire immutable authored entry before returning detached policies."""

import re
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- Git object enumeration uses a validated revision and argument vector
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.digital_name_policies.current_models import CurrentEntry
from sve_carddb.digital_name_policies.current_models import Index as CurrentIndex
from sve_carddb.digital_name_policies.current_models import Policy as CurrentPolicy
from sve_carddb.digital_name_policies.models import (
    Approval,
    CardTargetExclusion,
    CataloguePins,
    Entry,
    Exclusions,
    Index,
    LinkNameExclusion,
    LinkRegistryPins,
    NameExclusion,
    Policy,
)
from sve_carddb.registry.inputs import JSON_VALUE
from sve_carddb.registry.records import RecordData
from sve_carddb.registry.storage import MAX_BYTES
from sve_carddb.registry.yaml_reader import parse_yaml
from sve_carddb.snapshot.values import canonical, digest, object_value, parse

if TYPE_CHECKING:
    from sve_carddb.digital_name_policies.models import Purpose

INDEX = "digital-name-policies/index.yaml"
DIRECTORIES = ("digital-name-policies", "digital-name-exclusions")
CURRENT_FORMAT = 2

CONTENT_KEYS = {
    "names": frozenset(
        [
            "policy_id",
            "actual_answers",
            "normative_rules",
            "plain_language_rules",
            "catalogue_pins",
            "scope",
            "game_priority",
            "target_minimum_check",
            "display_label",
            "excluded_names",
            "visible_notices",
            "shared_answer_bindings",
            "shared_future_scope_rules",
            "final_exclusions_hash",
            "proposed_clause_replacements",
        ]
    ),
    "links": frozenset(
        [
            "policy_id",
            "actual_answers",
            "rule",
            "review",
            "limits",
            "precedence",
            "exclusions",
            "ids",
            "catalogue_pins",
            "registry_pins",
            "warning_disclosure",
            "coverage",
            "publication",
            "f1",
            "versioning",
            "plain_language_rules",
            "visible_notices",
            "fixed_receipt_disclosure",
            "shared_answer_bindings",
            "shared_future_scope_rules",
            "proposed_clause_replacements",
        ]
    ),
}
# These identify the supported rule semantics, including the disclosed name wording.
# Source pins and separately approved exclusion lists are checked independently.
SEMANTICS = {
    "names": "sha256:cf7946648d3a639210fb24cd2b70b1474cbaf04e29be657ee0537b3fc6561dcd",
    "links": "sha256:8514cf4a651bc8fb1881de24956c36896fa78eb07946a503fa63c883a51079ca",
}
INITIAL_NAMES_DOCUMENT = (
    "sha256:f38dc612c694abb743466778824da7baec0b0dff07d03d32e2a1a53b74c4f9f3"
)
ADOPTED_PROJECTIONS = {
    INITIAL_NAMES_DOCUMENT: "sha256:16d6f2f8fe23dd435fad9d1f436e35379c10d857b88ec766ce4c5c31089fe576",
    "sha256:0742f89d50384f076eb3ab219b6a369af60708f3d5d52d3d1b53b33d98d1389d": "sha256:6d752c9f50170e2d2dc236d2c5f1a6bb255b09c229b6946b99d85f8ca46d40e5",
}

ADOPTED_APPROVALS = {
    "sha256:f38dc612c694abb743466778824da7baec0b0dff07d03d32e2a1a53b74c4f9f3": "sha256:96ae0305bd71e907e5956eb2f1c9dfe91c74b2d49290911c4ada8f4235561a14",
    "sha256:0742f89d50384f076eb3ab219b6a369af60708f3d5d52d3d1b53b33d98d1389d": "sha256:b1ba6c901a6e5d92ba5e98fcf92244f129cac18cdb498abde36f2d46b32159fa",
}


def model[T: RecordData](kind: type[T], raw: JsonValue) -> T:
    """Do not echo source-bearing validation values in CLI failures."""
    try:
        if isinstance(raw, dict) and any(
            key.endswith("_format") and type(value) is not int
            for key, value in raw.items()
        ):
            raise ValueError("Policy format must be an integer")
        return kind.model_validate_json(canonical(raw))
    except ValidationError:
        raise ValueError("Invalid digital-name policy fields") from None


def decoded(raw: bytes) -> JsonValue:
    """Reuse strict YAML 1.2 without consulting mutable disk copies."""
    if len(raw) >= MAX_BYTES:
        raise ValueError("Digital-name policy file exceeds size limit")
    try:
        value = JSON_VALUE.validate_python(parse_yaml(raw), strict=True)
        canonical(value)
    except ValueError, TypeError, UnicodeError:
        raise ValueError("Invalid digital-name policy YAML") from None
    else:
        return value


def _portable(name: str) -> bool:
    path = PurePosixPath(name)
    return (
        bool(name)
        and not path.is_absolute()
        and path.as_posix() == name
        and all(
            part not in {".", ".."} and re.fullmatch(r"[A-Za-z0-9_.-]+", part)
            for part in path.parts
        )
    )


def semantics(content: dict[str, JsonValue], purpose: str) -> str:
    """Separate approved rule semantics from explicitly verified provenance pins."""
    detached = object_value(parse(canonical(content)))
    for key in ("policy_id", "catalogue_pins", "proposed_clause_replacements"):
        detached.pop(key, None)
    if purpose == "names":
        detached.pop("excluded_names", None)
        detached.pop("final_exclusions_hash", None)
    else:
        detached.pop("registry_pins", None)
        object_value(detached["exclusions"]).pop("initial_exclusions_hash", None)
        object_value(object_value(detached["review"])["private_application"]).pop(
            "policy_id", None
        )
    return digest(canonical(detached))


@dataclass(frozen=True)
class LoadedPolicy:
    policy: bytes
    approval: bytes
    exclusions: bytes

    def document(self) -> Policy:
        """Return a detached operation document."""
        return model(Policy, parse(self.policy))

    def receipt(self) -> Approval:
        """Return a detached approval without claiming to replay private evidence."""
        return model(Approval, parse(self.approval))

    def excluded(self) -> Exclusions:
        """Return the independently approved initial exclusion list."""
        return model(Exclusions, parse(self.exclusions))

    def catalogue(self) -> CataloguePins:
        """Read the closed frozen source pins."""
        return model(CataloguePins, self.document().content["catalogue_pins"])


@dataclass(frozen=True)
class Snapshot:
    authored_revision: str
    files: tuple[tuple[str, bytes], ...]
    policies: tuple[LoadedPolicy, ...]
    current_names: tuple[CurrentPolicy, ...] = ()

    def effective(self, purpose: Purpose) -> LoadedPolicy:
        """Select one terminal policy per explicitly requested purpose."""
        latest: dict[str, LoadedPolicy] = {}
        for loaded in self.policies:
            document = loaded.document()
            if document.purpose == purpose:
                latest[document.policy_id] = loaded
        if len(latest) != 1:
            raise ValueError(
                "Digital-name policy purpose must select exactly one policy"
            )
        return next(iter(latest.values()))

    def pins(self) -> dict[str, JsonValue]:
        """Pin exact bytes separately from canonical policy values."""
        return {
            "authored_revision": self.authored_revision,
            "files": [
                {
                    "path": name,
                    "exact_hash": digest(raw),
                    "canonical_hash": digest(canonical(decoded(raw))),
                }
                for name, raw in self.files
            ],
        }


def load(  # ruff: ignore[complex-structure,too-many-branches,too-many-statements,too-many-locals] -- whole index and receipt closure must be checked before returning any policy
    root: Path, repository: Path, revision: str
) -> Snapshot:
    """Verify Git file modes, complete index closure and exact on-disk bytes."""
    if re.fullmatch(r"[0-9a-f]{40}", revision) is None:
        raise ValueError("Policy authored revision must be a full Git SHA")
    pinned = PinnedRepository(repository)
    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- no shell and revision is a full validated SHA
        [
            pinned.executable,
            "-C",
            str(repository),
            "ls-tree",
            "-rz",
            revision,
            "--",
            *("authored/" + d for d in DIRECTORIES),
        ],
        check=False,
        capture_output=True,
    )
    if result.returncode:
        raise ValueError("Policy immutable tree is unavailable")
    names = []
    for item in result.stdout.split(b"\0"):
        if not item:
            continue
        metadata, encoded = item.split(b"\t", 1)
        mode, kind, _ = metadata.split()
        name = encoded.decode().removeprefix("authored/")
        if mode not in {b"100644", b"100755"} or kind != b"blob" or not _portable(name):
            raise ValueError("Unsafe immutable digital-name policy file")
        names.append(name)
    if INDEX not in names:
        raise ValueError("Digital-name policy index is missing")
    present = set()
    for directory in DIRECTORIES:
        base = root / directory
        for path in (base, *base.rglob("*")):
            if any(p.is_symlink() for p in (path, *path.parents)):
                raise ValueError("Symlink digital-name policy input")
            if path.is_file():
                present.add(path.relative_to(root).as_posix())
    if present != set(names):
        raise ValueError("Digital-name policy disk closure differs from immutable tree")
    blobs = pinned.read_many(revision, tuple("authored/" + n for n in names))
    files = tuple((name, blobs["authored/" + name]) for name in sorted(names))
    if any((root / name).read_bytes() != raw for name, raw in files):
        raise ValueError("Digital-name policy bytes differ from authored revision")
    values = {name: decoded(raw) for name, raw in files}
    raw_index = object_value(values[INDEX])
    current_names = []
    expected = {INDEX}
    if raw_index.get("digital_name_policy_index_format") == CURRENT_FORMAT:
        current_index = model(CurrentIndex, raw_index)
        legacy: dict[str, JsonValue] = {}
        for identifier, current_entry in current_index.policies.items():
            if isinstance(current_entry, CurrentEntry):
                current_path = f"digital-name-policies/{identifier}/current.yaml"
                if current_entry.path != current_path or current_path not in values:
                    raise ValueError("Current name policy indexed path mismatch")
                if digest(canonical(values[current_path])) != current_entry.hash:
                    raise ValueError("Current name policy indexed hash mismatch")
                current_policy = model(CurrentPolicy, values[current_path])
                if current_policy.policy_id != identifier:
                    raise ValueError("Current name policy identity mismatch")
                current_names.append(current_policy)
                expected.add(current_path)
            else:
                legacy[identifier] = [e.model_dump(mode="json") for e in current_entry]
        # This is a format-one view of the remaining links; no policy or proof is invented.
        index = (
            model(
                Index,
                {
                    "digital_name_policy_index_format": 1,
                    "kind": "digital_name_policy_index",
                    "policies": legacy,
                },
            )
            if legacy
            else None
        )
    else:
        index = model(Index, values[INDEX])
    policies = []
    for identifier, entries in sorted(
        {}.items() if index is None else index.policies.items()
    ):
        if not entries or tuple(e.version for e in entries) != tuple(
            range(1, len(entries) + 1)
        ):
            raise ValueError("Digital-name policy version sequence is incomplete")
        previous: Entry | None = None
        for entry in entries:
            stem = f"{entry.version:03}"
            policy_path = f"digital-name-policies/{identifier}/{stem}.policy.yaml"
            receipt_path = policy_path.replace(".policy.yaml", ".approval.yaml")
            exclusion_path = f"digital-name-exclusions/{identifier}/{stem}.yaml"
            if entry.path != policy_path or entry.exclusions_path != exclusion_path:
                raise ValueError("Digital-name policy indexed path mismatch")
            predecessor = (
                None
                if previous is None
                else digest(canonical(previous.model_dump(mode="json")))
            )
            if entry.predecessor != predecessor:
                raise ValueError("Digital-name policy predecessor mismatch")
            paths = (policy_path, receipt_path, exclusion_path)
            if any(p not in values for p in paths):
                raise ValueError("Digital-name policy indexed member is missing")
            hashes = (entry.hash, entry.approval_receipt_hash, entry.exclusions_hash)
            if any(
                digest(canonical(values[p])) != h
                for p, h in zip(paths, hashes, strict=True)
            ):
                raise ValueError("Digital-name policy indexed hash mismatch")
            policy, receipt, excluded = (
                model(Policy, values[policy_path]),
                model(Approval, values[receipt_path]),
                model(Exclusions, values[exclusion_path]),
            )
            if (
                any(
                    v.policy_id != identifier or v.version != entry.version
                    for v in (policy, receipt, excluded)
                )
                or excluded.purpose != policy.purpose
            ):
                raise ValueError("Digital-name policy envelope identity mismatch")
            if current_names and policy.purpose != "links":
                raise ValueError("Current name policy cannot mix a legacy name policy")
            _content(policy)
            _approval(policy, receipt, excluded, entry)
            expected.update(paths)
            policies.append(LoadedPolicy(*(canonical(values[p]) for p in paths)))
            previous = entry
    if expected != set(values):
        raise ValueError("Unindexed digital-name policy input")
    if len(current_names) > 1:
        raise ValueError("Current names must select exactly one policy")
    return Snapshot(revision, files, tuple(policies), tuple(current_names))


def _content(policy: Policy) -> None:
    content = policy.content
    if (
        set(content) != CONTENT_KEYS[policy.purpose]
        or content["policy_id"] != policy.policy_id
    ):
        raise ValueError("Digital-name policy content projection mismatch")
    if ADOPTED_PROJECTIONS.get(
        policy.approved_document_hash, digest(canonical(policy.model_dump(mode="json")))
    ) != digest(canonical(policy.model_dump(mode="json"))):
        raise ValueError("Previously adopted digital-name document projection changed")
    if semantics(content, policy.purpose) != SEMANTICS[policy.purpose]:
        raise ValueError("Unsupported digital-name policy rule semantics")
    pins = model(CataloguePins, content["catalogue_pins"])
    if (
        pins.source_batches
        != pins.parser_and_registry_configuration.digital_link_sources
        or not pins.source_batches
        or tuple(sorted({(b.store_id, b.batch_id) for b in pins.source_batches}))
        != tuple((b.store_id, b.batch_id) for b in pins.source_batches)
    ):
        raise ValueError("Digital-name policy catalogue batch closure mismatch")
    recipes = pins.parser_and_registry_configuration.translation_recipes
    if set(recipes) != {"translation-" + p + "-v1" for p in ("jp", "sv1", "svwb")}:
        raise ValueError("Digital-name policy catalogue recipes are incomplete")
    for provider in ("jp", "sv1", "svwb"):
        recipe = recipes["translation-" + provider + "-v1"]
        if (
            recipe.version != "translation-" + provider + "-v1"
            or recipe.code_path != "carddb/src/sve_carddb/translations/sources.py"
            or recipe.config != {"provider": provider}
        ):
            raise ValueError("Unsupported digital-name policy catalogue recipe")
    if policy.purpose == "links":
        registry = model(LinkRegistryPins, content["registry_pins"])
        expected = pins.parser_and_registry_configuration.catalog_registry
        if (
            registry.revision != expected.authored_revision
            or registry.index_hash != expected.index_hash
            or registry.source_replay_revision != pins.count_replay_main_revision
        ):
            raise ValueError("Digital-name link registry pins mismatch")


def _approval(
    policy: Policy, receipt: Approval, exclusions: Exclusions, entry: Entry
) -> None:
    if (
        receipt.policy_hash != entry.hash
        or receipt.approved_document_hash != policy.approved_document_hash
        or receipt.initial_exclusions_hash != entry.exclusions_hash
    ):
        raise ValueError("Digital-name approval hash closure mismatch")
    final_hash = (
        policy.content["final_exclusions_hash"]
        if policy.purpose == "names"
        else object_value(policy.content["exclusions"])["initial_exclusions_hash"]
    )
    if final_hash != exclusions.approved_list_hash:
        raise ValueError("Digital-name approved exclusion hash mismatch")
    keys: list[tuple[str, ...]] = []
    for item in exclusions.entries:
        if not item.reason.strip():
            raise ValueError("Digital-name exclusion reason is blank")
        if (policy.purpose == "names" and type(item) is not NameExclusion) or (
            policy.purpose == "links"
            and type(item) not in {LinkNameExclusion, CardTargetExclusion}
        ):
            raise ValueError("Digital-name exclusion kind differs from policy purpose")
        if isinstance(item, CardTargetExclusion):
            if (
                re.fullmatch(
                    r"[0-9]{9}" if item.game == "sv1" else r"[0-9]{8}", item.official_id
                )
                is None
            ):
                raise ValueError("Digital-name exclusion target ID mismatch")
            keys.append((item.kind, item.card_id, item.game, item.official_id))
        else:
            keys.append(("name", item.source_lang, item.source_name_hash))
    if keys != sorted(set(keys)):
        raise ValueError("Digital-name exclusions must be sorted and unique")
    _events(policy, receipt)


def _events(  # ruff: ignore[complex-structure] -- button, messages and disclosure are one approval evidence graph
    policy: Policy, receipt: Approval
) -> None:
    # These locators bind the first approval layout in contract §4; a new layout needs a loader and SEMANTICS update.
    events = receipt.approval_events
    times = tuple(datetime.fromisoformat(e.at) for e in events)
    if times != tuple(sorted(times)) or len(
        {canonical(e.model_dump(mode="json")) for e in events}
    ) != len(events):
        raise ValueError("Digital-name approval events must be sorted and unique")
    if any(not _portable(name) for name in receipt.evidence_hashes):
        raise ValueError("Unsafe digital-name approval evidence key")
    if (
        receipt.evidence_hashes.get(policy.purpose + "-policy.canonical.json")
        != policy.approved_document_hash
        or receipt.evidence_hashes.get(policy.purpose + "-policy.plain.md")
        != receipt.presented_text_hash
    ):
        raise ValueError("Digital-name approval evidence hash mismatch")
    buttons = [e for e in events if e.kind == "page_button"]
    if len(buttons) != 1:
        raise ValueError(
            "Digital-name approval requires exactly one explicit policy button"
        )
    button = buttons[0]
    expected = {
        "at": receipt.reviewed_at,
        "id": policy.purpose,
        "note": "",
        "policy_hash": policy.approved_document_hash,
        "text_sha256": receipt.presented_text_hash,
        "value": "agree",
    }
    if (
        button.uuid is not None
        or button.at != receipt.reviewed_at
        or button.value != expected
        or button.locator != f"name2_policy/{policy.purpose}.json"
        or receipt.evidence_hashes.get(button.locator) != button.source_hash
    ):
        raise ValueError(
            "Digital-name approval button does not approve this policy text"
        )
    messages = {e.uuid: e for e in events if e.kind == "message"}
    if any(
        e.uuid is None
        or e.value is not None
        or e.locator != e.uuid
        or e.source_hash not in receipt.evidence_hashes.values()
        for e in messages.values()
    ) or len(messages) != len([e for e in events if e.kind == "message"]):
        raise ValueError("Digital-name approval message evidence mismatch")
    changes = receipt.disclosed_changes
    if tuple(c.rule_id for c in changes) != tuple(sorted({c.rule_id for c in changes})):
        raise ValueError("Digital-name disclosures must be sorted and unique")
    for change in changes:
        accepted = messages.get(change.accepted_message_uuid)
        if (
            accepted is None
            or datetime.fromisoformat(change.disclosed_at)
            > datetime.fromisoformat(receipt.reviewed_at)
            or datetime.fromisoformat(accepted.at)
            < datetime.fromisoformat(change.disclosed_at)
        ):
            raise ValueError("Digital-name disclosed change lacks timely acceptance")
    if policy.approved_document_hash == INITIAL_NAMES_DOCUMENT and (
        set(messages) != {"b5d164b7-8e06-4336-af4e-de21f0306da4"}
        or messages["b5d164b7-8e06-4336-af4e-de21f0306da4"].at
        != "2026-10-02T20:33:39.423Z"
        or tuple(c.rule_id for c in changes) != ("label-and-authority", "new-jp")
    ):
        raise ValueError(
            "Initial adopted name policy requires its disclosed oral acceptance"
        )
    approved_receipt_hash = ADOPTED_APPROVALS.get(policy.approved_document_hash)
    if (
        approved_receipt_hash is not None
        and digest(canonical(receipt.model_dump(mode="json"))) != approved_receipt_hash
    ):
        raise ValueError("Previously adopted digital-name approval changed")
