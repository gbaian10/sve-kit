"""Current registered matcher switches do not require historical approval envelopes."""

from typing import TYPE_CHECKING, Literal, Self

from pydantic import ValidationError, field_validator, model_validator

from sve_carddb.core.authored import authored_root, check_path
from sve_carddb.core.models import RecordData
from sve_carddb.core.yaml import MAX_BYTES as LIMIT
from sve_carddb.template_parameter_rules.models import LEGACY_IDS, RuleId
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
    note: str = ""


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


def load(repository: Path) -> Rules:
    """Read the current matcher switches in the working tree."""
    return load_file(
        authored_root(repository) / "template-parameter-rules/current.yaml",
        root=authored_root(repository),
    )


def load_file(path: Path, *, root: Path) -> Rules:
    """Only links at or below the data root can redirect an authored input."""
    check_path(root, path)
    if not path.is_file() or path.stat().st_size >= LIMIT:
        raise ValueError("Missing or symlink current parameter rules")
    return parse(path.read_bytes())
