"""Current registered matcher switches do not require historical approval envelopes."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves closed input annotations

from typing import TYPE_CHECKING, Literal, Self

from pydantic import ValidationError, field_validator, model_validator

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.registry.records import RecordData
from sve_carddb.snapshot.values import canonical
from sve_carddb.template_parameter_rules.models import LEGACY_IDS, RuleId
from sve_carddb.template_parameter_rules.replay import resolve_roles
from sve_carddb.template_parameter_rules.repository import LIMIT, git
from sve_carddb.template_parameter_rules.repository import revision as check_revision
from sve_carddb.template_parameters.inventory import Candidates
from sve_carddb.template_parameters.rule_candidates import BY_ID
from sve_carddb.template_translations.files import json_bytes

if TYPE_CHECKING:
    from pathlib import Path

PATH = "authored/template-parameter-rules/current.yaml"


class Rule(RecordData):
    rule_id: RuleId
    enabled: bool
    origin: Literal["official", "project", "machine"]
    low_confidence: bool
    note: str


class Rules(RecordData):
    parameter_rule_format: Literal[2]
    kind: Literal["template_parameter_rules"]
    rules: tuple[Rule, ...]

    @field_validator("parameter_rule_format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Current parameter rule format must be integer two")
        return value

    @model_validator(mode="after")
    def _known(self) -> Self:
        keys = tuple(rule.rule_id for rule in self.rules)
        if keys != tuple(sorted(set(keys))):
            raise ValueError("Current parameter rule IDs must be sorted and unique")
        if any(key not in {*LEGACY_IDS, *BY_ID} for key in keys):
            raise ValueError("Current parameter rule is not registered")
        return self

    def enabled(self) -> tuple[str, ...]:
        """An explicit empty selection must not silently enable every registered rule."""
        return tuple(rule.rule_id for rule in self.rules if rule.enabled)


def parse(raw: bytes) -> Rules:
    """Invalid input errors omit official wording and caller-controlled field values."""
    try:
        return Rules.model_validate_json(json_bytes(raw))
    except ValidationError, ValueError, TypeError:
        raise ValueError("Invalid current parameter rules") from None


def load(repository: PinnedRepository, revision: str) -> Rules:
    """Read the current tree, never walk policy/approval ancestors."""
    check_revision(repository, revision)
    entry = git(repository, "ls-tree", "-l", "-z", revision, "--", PATH).rstrip(b"\0")
    header, separator, path = entry.partition(b"\t")
    fields = header.split()
    if (
        not separator
        or path != PATH.encode()
        or len(fields) != len(("mode", "kind", "oid", "size"))
        or fields[:2] != [b"100644", b"blob"]
        or not fields[3].isdigit()
    ):
        raise ValueError("Missing or unsafe current parameter rules")
    if int(fields[3]) >= LIMIT:
        raise ValueError("Current parameter rules must be smaller than one MiB")
    return parse(repository.read(revision, PATH))


def load_file(path: Path) -> Rules:
    """Local tool inputs require ordinary bounded files, including all parent paths."""
    if (
        not path.is_file()
        or any(p.is_symlink() for p in (path, *path.parents))
        or path.stat().st_size >= LIMIT
    ):
        raise ValueError("Missing or symlink current parameter rules")
    return parse(path.read_bytes())


def resolve(
    rules: Rules, candidates: Candidates
) -> tuple[tuple[bytes, ...], tuple[bytes, ...]]:
    """Keep positional, raw, reference and role checks in the existing matching algorithm."""
    roles = {
        key: "numeric" if key in LEGACY_IDS else BY_ID[key].role
        for key in rules.enabled()
    }
    return resolve_roles(candidates, roles, roles=("body", "reminder"))


def migrate(policy: bytes) -> Rules:
    """Transfer explicitly listed old rules; this does not infer extra matcher consent."""
    from sve_carddb.template_parameter_rules.loader import parse_policy  # ruff: ignore[import-outside-top-level] -- legacy envelopes remain separate from the current reader

    old = parse_policy(policy)
    return Rules.model_validate_json(
        canonical(
            {
                "parameter_rule_format": 2,
                "kind": "template_parameter_rules",
                "rules": [
                    {
                        "rule_id": rule.rule_id,
                        "enabled": True,
                        "origin": "project",
                        "low_confidence": False,
                        "note": "",
                    }
                    for rule in old.rules
                ],
            }
        )
    )
