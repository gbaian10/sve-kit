"""Validate an entire immutable authored entry before returning current policies."""

import re
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- Git object enumeration uses a validated revision and argument vector
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from pydantic import JsonValue, ValidationError

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.digital_name_policies.current_models import Index, LinkPolicy
from sve_carddb.digital_name_policies.current_models import Policy as CurrentPolicy
from sve_carddb.registry.inputs import JSON_VALUE
from sve_carddb.registry.records import RecordData
from sve_carddb.registry.storage import MAX_BYTES
from sve_carddb.registry.yaml_reader import parse_yaml
from sve_carddb.snapshot.values import canonical, digest, object_value

INDEX = "digital-name-policies/index.yaml"
DIRECTORIES = ("digital-name-policies",)


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


@dataclass(frozen=True)
class Snapshot:
    authored_revision: str
    files: tuple[tuple[str, bytes], ...]
    current_names: tuple[CurrentPolicy, ...] = ()
    links: LinkPolicy | None = None

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


def load(  # ruff: ignore[complex-structure,too-many-branches,too-many-statements,too-many-locals] -- whole index and policy closure must be checked before returning any policy
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
    index = model(Index, values[INDEX])
    current_names = []
    links = []
    for identifier, entry in index.policies.items():
        member = f"digital-name-policies/{identifier}/current.yaml"
        if entry.path != member or member not in values:
            raise ValueError("Digital-name policy indexed path mismatch")
        if digest(canonical(values[member])) != entry.hash:
            raise ValueError("Digital-name policy indexed hash mismatch")
        if object_value(values[member]).get("purpose") == "links":
            link = model(LinkPolicy, values[member])
            if link.policy_id != identifier:
                raise ValueError("Digital-name policy identity mismatch")
            links.append(link)
        else:
            policy = model(CurrentPolicy, values[member])
            if policy.policy_id != identifier:
                raise ValueError("Digital-name policy identity mismatch")
            current_names.append(policy)
    if {INDEX, *(e.path for e in index.policies.values())} != set(values):
        raise ValueError("Unindexed digital-name policy input")
    if len(current_names) > 1 or len(links) > 1:
        raise ValueError("Digital-name policy purpose must select at most one policy")
    return Snapshot(revision, files, tuple(current_names), links[0] if links else None)
