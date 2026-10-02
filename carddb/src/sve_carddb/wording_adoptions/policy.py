"""Immutable policy approval and complete, code-point-exact equivalence checks."""

from difflib import SequenceMatcher
from typing import TYPE_CHECKING, Literal, Self

from pydantic import JsonValue, field_validator, model_validator

from sve_carddb.registry.inputs import JSON_VALUE
from sve_carddb.registry.records import Hash, Instant, RecordData, Text
from sve_carddb.registry.yaml_reader import parse_yaml
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.wording_adoptions.models import RuleMatch, ordered_objects

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.catalog.adoption_sources import PinnedRepository
    from sve_carddb.text_observations.models import FaceContent
    from sve_carddb.wording_adoptions.models import Decision, RuleSet


class Rule(RecordData):
    rule_id: Text
    category: Text
    regions: tuple[Literal["jp", "en"], ...]
    fields: tuple[Text, ...]
    matcher_version: Text
    parameters: dict[str, JsonValue]
    action: Literal["equivalent", "classify_only"]
    match_condition: Text
    exclusions: tuple[Text, ...]
    finite_transform: Text
    examples: dict[str, JsonValue]


class Policy(RecordData):
    rule_policy_format: Literal[1]
    kind: Literal["wording_rule_policy"]
    policy_id: Text
    common_boundary: dict[str, JsonValue]
    rules: tuple[Rule, ...]

    @field_validator("rule_policy_format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Policy format must be an integer")
        return value

    @model_validator(mode="after")
    def _rules(self) -> Self:
        keys = tuple(r.rule_id for r in self.rules)
        if keys != tuple(sorted(set(keys))) or not keys:
            raise ValueError("Policy rules must be sorted, unique and nonempty")
        return self


class ApprovedAction(RecordData):
    rule_id: Text
    action: Literal["equivalent", "classify_only"]


class Approval(RecordData):
    rule_approval_format: Literal[1]
    kind: Literal["wording_rule_approval"]
    policy_id: Text
    rule_set_hash: Hash
    reviewed_by: Text
    reviewed_at: Instant
    reviewed_precision: Literal["day", "instant"]
    rules: tuple[ApprovedAction, ...]
    note: Text

    @field_validator("rule_approval_format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Approval format must be an integer")
        return value

    @model_validator(mode="after")
    def _answer(self) -> Self:
        if not self.reviewed_by.strip() or not self.note.strip():
            raise ValueError("Policy approval requires an actual named answer")
        if self.reviewed_precision == "day" and not self.reviewed_at.endswith(
            "T00:00:00Z"
        ):
            raise ValueError("Day policy approval requires UTC midnight")
        keys = tuple(r.rule_id for r in self.rules)
        if keys != tuple(sorted(set(keys))) or not keys:
            raise ValueError("Approved actions must be sorted, unique and nonempty")
        return self


def load_policy(
    repository: PinnedRepository, pin: RuleSet
) -> tuple[Policy, Approval, dict[str, bytes]]:
    """Read both immutable blobs; no category or decision implies rule approval."""
    if not pin.path.startswith("authored/wording-rules/") or not pin.path.endswith(
        ".policy.yaml"
    ):
        raise ValueError("Unsupported wording policy path")
    receipt_path = pin.path.removesuffix(".policy.yaml") + ".approval.yaml"
    files = {
        name: repository.read(pin.authored_revision, name)
        for name in (pin.path, receipt_path)
    }
    policy_raw = JSON_VALUE.validate_python(parse_yaml(files[pin.path]), strict=True)
    approval_raw = JSON_VALUE.validate_python(
        parse_yaml(files[receipt_path]), strict=True
    )
    policy = Policy.model_validate_json(canonical(policy_raw))
    approval = Approval.model_validate_json(canonical(approval_raw))
    if (
        digest(canonical(policy_raw)) != pin.hash
        or digest(canonical(approval_raw)) != pin.approval_receipt_hash
        or (policy.policy_id, approval.policy_id) != (pin.policy_id, pin.policy_id)
        or approval.rule_set_hash != pin.hash
        or tuple((r.rule_id, r.action) for r in policy.rules)
        != tuple((r.rule_id, r.action) for r in approval.rules)
    ):
        raise ValueError("Policy content, approval pin or approved actions mismatch")
    if (
        pin.hash
        != "sha256:169d7eb1a71beb56bac3df4d5b3f6fd2e02e7acc5998322ebb9926b7b3b97a67"
    ):
        raise ValueError("Policy conditions have no supported fixed matcher contract")
    return policy, approval, files


def verify_policy_reviewer(approval: Approval, decision: Decision) -> None:
    """The application author is separate from the original policy approver."""
    if (
        decision.reviewed_by,
        decision.reviewed_at,
        decision.reviewed_precision,
    ) != (
        approval.reviewed_by,
        approval.reviewed_at,
        approval.reviewed_precision,
    ) or "政策核可" not in decision.note:
        raise ValueError("Adoption decision impersonates the policy approver")


def _eol_pair(before: FaceContent, after: FaceContent) -> dict[str, tuple[str, str]]:
    protected_before = before.wording_fields() | {"text": None, "sections": []}
    protected_after = after.wording_fields() | {"text": None, "sections": []}
    if (
        protected_before != protected_after
        or before.effect is None
        or after.effect is None
        or len(before.sections) != len(after.sections)
    ):
        raise ValueError("Uncovered current-bearing difference or missing text")
    pairs = {"text": (before.effect, after.effect)} | {
        f"sections/{i}": pair
        for i, pair in enumerate(zip(before.sections, after.sections, strict=True))
    }
    for old, new in pairs.values():
        a, b = old.replace("\r\n", "\n"), new.replace("\r\n", "\n")
        if "\r" in a or "\r" in b or a != b:
            raise ValueError("Complete difference is not CRLF/LF-only")
    return pairs


def equivalent_matches(
    policy: Policy,
    observations: Mapping[str, FaceContent],
    selected: str,
    *,
    region: Literal["jp", "en"],
    previous: FaceContent | None = None,
) -> tuple[RuleMatch, ...]:
    """Recompute every diff, including a predecessor outside the new inventory."""
    if selected not in observations:
        raise ValueError("Selected policy observation is outside the full inventory")
    target = observations[selected]
    candidates = dict(observations)
    if "previous" in candidates:
        raise ValueError("Observation key cannot use the predecessor reserved word")
    if previous is not None:
        candidates["previous"] = previous
    enabled = any(
        r.rule_id == "wp:eol-v1" and r.action == "equivalent" and region in r.regions
        for r in policy.rules
    )
    result: list[RuleMatch] = []
    for key, content in candidates.items():
        if content.wording_fields() == target.wording_fields():
            if content.effect is None:
                raise ValueError("Missing text cannot become equivalent")
            continue
        if not enabled:
            raise ValueError("Difference has no approved equivalent rule")
        for field, (old, new) in _eol_pair(content, target).items():
            result.extend(
                RuleMatch(
                    from_observation_key=key,
                    to_observation_key=selected,
                    rule_id="wp:eol-v1",
                    field=field,
                    before_range=(a, b),
                    after_range=(c, d),
                )
                for operation, a, b, c, d in SequenceMatcher(
                    None, old, new, autojunk=False
                ).get_opcodes()
                if operation != "equal"
            )
    matches = tuple(sorted(result, key=lambda r: canonical(r.model_dump(mode="json"))))
    ordered_objects(matches)
    return matches


def verify_matches(
    actual: tuple[RuleMatch, ...], expected: tuple[RuleMatch, ...]
) -> None:
    """Reject overlap, partial coverage and supplied ranges that were never matched."""
    if actual != expected:
        raise ValueError("Rule matches do not cover exactly the recomputed full diff")
