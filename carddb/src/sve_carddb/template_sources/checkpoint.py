"""Compare legacy IDs and their complete use set as two independent checks."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.values import array, digest, object_value, parse

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.template_sources.inventory import Scan


@dataclass(frozen=True)
class LegacyTemplate:
    identifier: str
    normalized: str
    members: tuple[str, ...]


def _legacy(row: bytes) -> LegacyTemplate:
    value = object_value(parse(row))
    identifier, normalized = value.get("template"), value.get("normalized")
    members = array(value.get("members"))
    if (
        not isinstance(identifier, str)
        or re.fullmatch(r"T[0-9a-f]{10}", identifier) is None
        or not isinstance(normalized, str)
        or not normalized
    ):
        raise ValueError("Invalid legacy template fingerprint or member set")
    if (
        identifier != "T" + digest(normalized.encode())[7:17]
        or not members
        or not all(isinstance(member, str) and member for member in members)
        or type(value.get("lines")) is not int
        or value["lines"] != len(members)
    ):
        raise ValueError("Invalid legacy template fingerprint or member set")
    return LegacyTemplate(identifier, normalized, tuple(str(m) for m in members))


def parse_legacy(content: bytes) -> tuple[LegacyTemplate, ...]:
    """Draft text is comparison input only; errors never include its contents."""
    result = []
    for row in content.splitlines():
        if not row.strip():
            continue
        try:
            result.append(_legacy(row))
        except ValueError, TypeError:
            raise ValueError(
                "Invalid legacy template fingerprint or member set"
            ) from None
    if not result or len({item.identifier for item in result}) != len(result):
        raise ValueError("Legacy template IDs must be nonempty and unique")
    members = [member for item in result for member in item.members]
    if len(set(members)) != len(members):
        raise ValueError("Legacy template members must be globally unique")
    return tuple(result)


def read_legacy(path: Path) -> tuple[LegacyTemplate, ...]:
    """Read once so a checkpoint's comparison and file hash use identical bytes."""
    return parse_legacy(path.read_bytes())


def compare(scan: Scan, legacy: tuple[LegacyTemplate, ...]) -> dict[str, JsonValue]:
    """A single matching occurrence proves a fingerprint, not complete source usage."""
    by_id: dict[str, str] = {}
    by_hash: dict[str, str] = {}
    by_member: dict[str, str] = {}
    for occurrence in scan.occurrences:
        checksum = digest(occurrence.normalized.encode())
        if (
            occurrence.template in by_id
            and by_id[occurrence.template] != occurrence.normalized
        ):
            raise ValueError(
                "Legacy short template ID collides with different normalized bytes"
            )
        if checksum in by_hash and by_hash[checksum] != occurrence.normalized:
            raise ValueError("Full normalized hash collides with different bytes")
        if occurrence.member_hash in by_member:
            raise ValueError(
                "Current JP template occurrence must have a unique legacy member"
            )
        by_id[occurrence.template] = occurrence.normalized
        by_hash[checksum] = occurrence.normalized
        by_member[occurrence.member_hash] = checksum
    rows: list[JsonValue] = []
    missing_members: list[JsonValue] = []
    expected_members: set[str] = set()
    for item in legacy:
        checksum = digest(item.normalized.encode())
        found = item.identifier in by_id
        if found and by_id[item.identifier] != item.normalized:
            raise ValueError(
                "Legacy short template ID collides with different normalized bytes"
            )
        member_hashes = tuple(digest(member.encode()) for member in item.members)
        expected_members.update(member_hashes)
        missing = []
        for member_hash in member_hashes:
            actual = by_member.get(member_hash)
            if actual != checksum:
                reason = (
                    "archived_member_normalized_changed"
                    if actual is not None
                    else "missing_archived_ability_member"
                )
                missing.append(
                    {
                        "member_hash": member_hash,
                        "reason": reason,
                        "actual_normalized_hash": actual,
                    }
                )
        missing_members.extend(
            {"template": item.identifier, **value} for value in missing
        )
        rows.append(
            {
                "template": item.identifier,
                "normalized_hash": checksum,
                "reproduced": found,
                "expected_members": len(member_hashes),
                "matched_members": len(member_hashes) - len(missing),
                "reasons": list[JsonValue](
                    sorted({str(value["reason"]) for value in missing})
                )
                if not found
                else [],
            }
        )
    expected_ids = {item.identifier for item in legacy}
    return {
        "fingerprints": {
            "complete": expected_ids <= by_id.keys(),
            "expected": len(legacy),
            "reproduced": len(expected_ids & by_id.keys()),
            "generated": len(by_id),
            "additional_ids": list[JsonValue](sorted(by_id.keys() - expected_ids)),
            "templates": rows,
        },
        "legacy_member_coverage": {
            "complete": not missing_members,
            "expected": len(expected_members),
            "matched": len(expected_members) - len(missing_members),
            "additional_members": len(by_member.keys() - expected_members),
            "failures": missing_members,
        },
    }
