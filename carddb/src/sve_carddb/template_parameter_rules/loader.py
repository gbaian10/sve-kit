"""Closed authored pairs and immutable matcher pins; loading is not source adoption."""

from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, cast

from pydantic import ValidationError

from sve_carddb.registry.inputs import JSON_VALUE
from sve_carddb.registry.yaml_reader import parse_yaml
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameter_rules.cases import evaluate, fixed_examples
from sve_carddb.template_parameter_rules.events import (
    verify_event_history,
    verify_events,
)
from sve_carddb.template_parameter_rules.legacy import historical
from sve_carddb.template_parameter_rules.models import LEGACY_IDS, Approval, Pin, Policy
from sve_carddb.template_parameter_rules.repository import ancestor, immutable
from sve_carddb.template_parameters.numeric_rules import (
    definition as numeric_definition,
)
from sve_carddb.template_parameters.rule_candidates import BY_ID, definition
from sve_carddb.template_sources.normalizer import VERSION
from sve_carddb.template_sources.pins import PARSER

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.catalog.adoption_sources import PinnedRepository
    from sve_carddb.template_parameter_rules.models import Rule
    from sve_carddb.template_parameters.models import NumericRule

RUNTIME = Path(__file__).resolve().parents[4]
MATCHER_PATHS = tuple(
    sorted(
        "carddb/src/sve_carddb/" + name
        for name in (
            "template_parameters/analysis.py",
            "template_parameters/numeric_rules.py",
            "template_parameters/candidate_matching.py",
            "template_parameters/rule_candidates.py",
            "template_parameters/provenance.py",
            "template_parameters/references.py",
            "template_parameters/models.py",
            "template_sources/normalizer.py",
        )
    )
)


@dataclass(frozen=True)
class Loaded:
    pin: Pin
    policy_bytes: bytes
    approval_bytes: bytes
    dependencies: tuple[tuple[str, str, str], ...]
    historical_revision: str | None

    @property
    def policy(self) -> Policy:
        """Do not leak mutable nested dictionaries from verified stored bytes."""
        return parse_policy(self.policy_bytes)

    @property
    def approval(self) -> Approval:
        """Receipts confer rule recognition, never template or translation adoption."""
        return parse_approval(self.approval_bytes)


def _json(raw: bytes) -> bytes:
    return canonical(JSON_VALUE.validate_python(parse_yaml(raw), strict=True))


def parse_policy(raw: bytes) -> Policy:
    """Strict YAML precedes the closed policy boundary; diagnostics never expose fields."""
    try:
        return Policy.model_validate_json(_json(raw))
    except ValidationError, ValueError, TypeError:
        raise ValueError("Invalid recognition policy envelope") from None


def parse_approval(raw: bytes) -> Approval:
    """The distinct receipt kind cannot be borrowed for a sampled adoption."""
    try:
        return Approval.model_validate_json(_json(raw))
    except ValidationError, ValueError, TypeError:
        raise ValueError("Invalid recognition approval envelope") from None


def _matcher(
    rule: Rule,
    repository: PinnedRepository,
    main: str,
    verified: dict[str, dict[str, bytes]],
) -> dict[str, bytes]:
    if rule.rule_id in LEGACY_IDS:
        expected = numeric_definition(cast("NumericRule", rule.rule_id))
        role = "numeric"
    elif rule.rule_id in BY_ID:
        expected = definition(BY_ID[rule.rule_id])
        role = BY_ID[rule.rule_id].role
    else:
        raise ValueError("Recognition policy rule is not registered")
    if (
        rule.matcher_version != expected["matcher_version"]
        or rule.recognized_role != role
        or rule.condition_hash != expected["condition_hash"]
        or canonical(rule.match_conditions) != canonical(expected["match_conditions"])
    ):
        raise ValueError(
            "Recognition policy conditions differ from the registered matcher"
        )
    if rule.examples != fixed_examples(rule.rule_id):
        raise ValueError(
            "Recognition policy must use the registered complete fixed cases"
        )
    for case in rule.examples.positive + rule.examples.negative:
        if evaluate(rule.rule_id, case.input) != case.expected_match:
            raise ValueError(
                "Recognition fixed case does not replay its declared answer"
            )
    return _code(repository, rule.matcher_commit, main, verified, presented=False)


def load_pairs(
    repository: PinnedRepository, revision: str, *, main_revision: str
) -> dict[str, Loaded]:
    """Validate every pair and historical event before selecting a caller's policy."""
    tree = immutable(repository, revision)
    content = repository.read_many(revision, tuple(sorted(tree))) if tree else {}
    result: dict[str, Loaded] = {}
    receipts: list[Approval] = []
    verified: dict[str, dict[str, bytes]] = {}
    for path in sorted(name for name in tree if name.endswith(".policy.yaml")):
        receipt_path = path.removesuffix(".policy.yaml") + ".approval.yaml"
        policy_bytes, approval_bytes = content[path], content[receipt_path]
        policy, receipt = parse_policy(policy_bytes), parse_approval(approval_bytes)
        if (
            path
            != "authored/template-parameter-rules/" + policy.policy_id + ".policy.yaml"
        ):
            raise ValueError("Recognition policy filename and ID disagree")
        if receipt.policy_hash != digest(_json(policy_bytes)):
            raise ValueError("Recognition receipt policy hash mismatch")
        if policy.scope.parser_ids != (PARSER,) or policy.scope.normalizer_ids != (
            VERSION,
            "template-parameters-jp-candidate-v1",
        ):
            raise ValueError(
                "Recognition policy parser or normalizer scope is unsupported"
            )
        verify_events(policy, receipt)
        _presentations(receipt, repository, main_revision, verified)
        dependencies = {
            (revision, path): policy_bytes,
            (revision, receipt_path): approval_bytes,
        }
        for rule in policy.rules:
            dependencies.update(
                {
                    (rule.matcher_commit, name): raw
                    for name, raw in _matcher(
                        rule, repository, main_revision, verified
                    ).items()
                }
            )
        legacy_revision = None
        if any(
            item.condition_hash is None
            for event in receipt.events.values()
            for item in event.authorized_rules
        ):
            legacy_revision, legacy_content = historical(repository, main_revision)
            dependencies.update(
                {(legacy_revision, name): raw for name, raw in legacy_content.items()}
            )
        pin = Pin(
            policy_id=policy.policy_id,
            authored_revision=revision,
            path=path,
            hash=receipt.policy_hash,
            approval_receipt_hash=digest(_json(approval_bytes)),
        )
        result[policy.policy_id] = Loaded(
            pin,
            policy_bytes,
            approval_bytes,
            tuple(
                sorted(
                    (commit, name, digest(raw))
                    for (commit, name), raw in dependencies.items()
                )
            ),
            legacy_revision,
        )
        receipts.append(receipt)
    verify_event_history(
        tuple(receipts), tuple(item.policy for item in result.values())
    )
    return result


def load_config(
    config: dict[str, JsonValue], repository: PinnedRepository, *, main_revision: str
) -> Loaded | None:
    """Only an explicit null disables recognition; a candidate switch supplies no consent."""
    if "recognition_policy" not in config:
        raise ValueError(
            "Recognition recipe must explicitly declare its policy pin or null"
        )
    value = config["recognition_policy"]
    if value is None:
        return None
    try:
        pin = Pin.model_validate_json(canonical(value))
    except ValidationError:
        raise ValueError(
            "Recognition policy requires the complete five-field pin"
        ) from None
    pairs = load_pairs(repository, pin.authored_revision, main_revision=main_revision)
    if pin.policy_id not in pairs:
        raise ValueError("Recognition pinned policy pair is absent")
    loaded = pairs[pin.policy_id]
    if loaded.pin != pin:
        raise ValueError("Recognition policy or approval receipt pin hash mismatch")
    return loaded


def _presentations(
    receipt: Approval,
    repository: PinnedRepository,
    main: str,
    verified: dict[str, dict[str, bytes]],
) -> None:
    for event in receipt.events.values():
        for item in event.presentation.presented_rules:
            if item.condition_hash is None:
                continue
            if item.rule_id in LEGACY_IDS:
                expected = numeric_definition(cast("NumericRule", item.rule_id))
            elif item.rule_id in BY_ID:
                expected = definition(BY_ID[item.rule_id])
            else:
                raise ValueError("Recognition presented rule is not registered")
            if (
                item.condition_hash != expected["condition_hash"]
                or item.matcher_version != expected["matcher_version"]
            ):
                raise ValueError(
                    "Recognition presented identity differs from the registered matcher"
                )
            assert item.matcher_commit is not None
            _code(repository, item.matcher_commit, main, verified, presented=True)


def _code(
    repository: PinnedRepository,
    commit: str,
    main: str,
    verified: dict[str, dict[str, bytes]],
    *,
    presented: bool,
) -> dict[str, bytes]:
    if commit not in verified:
        ancestor(repository, commit, main)
        content = repository.read_many(commit, MATCHER_PATHS)
        if any(raw != (RUNTIME / name).read_bytes() for name, raw in content.items()):
            context = "presented" if presented else "pinned"
            raise ValueError(
                "Recognition "
                + context
                + " matcher bytes differ from the supported runtime"
            )
        verified[commit] = content
    return verified[commit]
