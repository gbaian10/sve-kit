"""Validate an entire immutable authored entry before returning current policies."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError

from sve_carddb.core.authored import read, require_directory
from sve_carddb.core.json import canonical, object_value
from sve_carddb.core.models import RecordData
from sve_carddb.core.yaml import JSON_VALUE
from sve_carddb.domains.digital.name_policies.records import LinkPolicy
from sve_carddb.domains.digital.name_policies.records import Policy as CurrentPolicy

if TYPE_CHECKING:
    from pathlib import Path


def model[T: RecordData](kind: type[T], raw: JsonValue) -> T:
    """Do not echo source-bearing validation values in CLI failures."""
    try:
        if isinstance(raw, dict) and any(
            key == "format" and type(value) is not int for key, value in raw.items()
        ):
            raise ValueError("Policy format must be an integer")
        return kind.model_validate_json(canonical(raw))
    except ValidationError:
        raise ValueError("Invalid digital-name policy fields") from None


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
    directory = root / "digital/policies"
    require_directory(root, directory)
    files = []
    current_names = []
    links = []
    for purpose, filename in (
        ("names", "names.yaml"),
        ("links", "links.yaml"),
    ):
        path = directory / filename
        # Optional purposes stay absent; dangling links still fail at the file boundary.
        if not path.exists() and not path.is_symlink():
            continue
        exact, content = read(path, root=root)
        value = object_value(JSON_VALUE.validate_json(content))
        policy = (
            model(LinkPolicy, value)
            if purpose == "links"
            else model(CurrentPolicy, value)
        )
        if policy.purpose != purpose:
            raise ValueError("Digital-name policy purpose mismatch")
        files.append((path.relative_to(root).as_posix(), exact))
        if isinstance(policy, LinkPolicy):
            links.append(policy)
        else:
            current_names.append(policy)
    return Snapshot(
        revision, tuple(files), tuple(current_names), links[0] if links else None
    )
