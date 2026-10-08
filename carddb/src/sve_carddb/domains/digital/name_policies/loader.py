"""Validate an entire immutable authored entry before returning current policies."""

import re
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from pydantic import JsonValue, ValidationError

from sve_carddb.core.authored import read, require_directory
from sve_carddb.core.json import canonical, object_value
from sve_carddb.core.models import RecordData
from sve_carddb.core.yaml import JSON_VALUE, MAX_BYTES, parse_yaml
from sve_carddb.domains.digital.name_policies.records import LinkPolicy
from sve_carddb.domains.digital.name_policies.records import Policy as CurrentPolicy


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

    def configuration(self) -> dict[str, JsonValue]:
        """Reuse current inputs within this command."""
        return {
            "authored_revision": self.authored_revision,
            "policies": [p.policy_id for p in self.current_names],
        }


def load(root: Path, revision: str) -> Snapshot:
    """Read only current policies in the dedicated data area."""
    directory = root / "digital-name-policies"
    require_directory(root, directory)
    files = []
    current_names = []
    links = []
    for group in sorted(directory.glob("*")):
        if group.is_symlink():
            raise ValueError("Symlink digital-name policy input")
        if not group.is_dir() or not _portable(group.name):
            continue
        path = group / "current.yaml"
        if not path.exists():
            continue
        exact, content = read(path, root=root)
        value = object_value(JSON_VALUE.validate_json(content))
        policy = (
            model(LinkPolicy, value)
            if value.get("purpose") == "links"
            else model(CurrentPolicy, value)
        )
        if policy.policy_id != group.name:
            raise ValueError("Digital-name policy identity mismatch")
        files.append((path.relative_to(root).as_posix(), exact))
        if isinstance(policy, LinkPolicy):
            links.append(policy)
        else:
            current_names.append(policy)
    if len(current_names) > 1 or len(links) > 1:
        raise ValueError("Digital-name policy purpose must select at most one policy")
    return Snapshot(
        revision, tuple(files), tuple(current_names), links[0] if links else None
    )
