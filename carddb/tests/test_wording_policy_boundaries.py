"""Independent refusal paths and action/region gates of the finite policy consumer."""

import re
from pathlib import Path

import pytest
from pydantic import ValidationError

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.wording_adoptions.models import RuleSet
from sve_carddb.wording_adoptions.policy import (
    Approval,
    Policy,
    equivalent_matches,
    load_policy,
)

from .test_wording_policy import POLICY_PATH, face
from .test_wording_policy import policy as policy  # ruff: ignore[useless-import-alias] -- shared immutable policy
from .wording_adoption_fixtures import commit, git


def test_coherently_replaced_policy_still_requires_the_supported_matcher_hash(
    policy: Policy, tmp_path: Path
) -> None:
    root = tmp_path / "repository"
    path = root / POLICY_PATH
    path.parent.mkdir(parents=True)
    wire = policy.model_dump(mode="json")
    wire["rules"][0]["match_condition"] += " Synthetic alternate condition."
    checksum = digest(canonical(wire))
    approval_path = path.with_name("145-v1.approval.yaml")
    approval = object_value(
        read_yaml(
            Path(__file__).resolve().parents[2]
            / "authored/wording-rules/145-v1.approval.yaml"
        )
    )
    approval["rule_set_hash"] = checksum
    path.write_bytes(canonical(wire))
    approval_path.write_bytes(canonical(approval))
    git(root, "init")
    revision = commit(root)
    pin = RuleSet(
        policy_id=policy.policy_id,
        authored_revision=revision,
        path=POLICY_PATH,
        hash=checksum,
        approval_receipt_hash=digest(canonical(approval)),
    )
    with pytest.raises(
        ValueError,
        match=r"\APolicy conditions have no supported fixed matcher contract\Z",
    ):
        load_policy(PinnedRepository(root), pin)


@pytest.mark.parametrize("change", ["action", "region", "other", "empty"])
def test_eol_difference_needs_the_exact_rule_action_and_region(
    policy: Policy, change: str
) -> None:
    rule = next(r for r in policy.rules if r.rule_id == "wp:eol-v1")
    if change == "action":
        rule = rule.model_copy(update={"action": "classify_only"})
    elif change == "region":
        rule = rule.model_copy(update={"regions": ("en",)})
    elif change == "other":
        rule = rule.model_copy(update={"rule_id": "wp:layout-space-v1"})
    restricted = policy.model_copy(
        update={"rules": () if change == "empty" else (rule,)}
    )
    template = face(
        object_value(array(policy.rules[1].examples["positive"])[0])["before"]
    )
    before = template.model_copy(
        update={"effect": "Synthetic\r\nparagraph", "sections": ()}
    )
    after = before.model_copy(update={"effect": "Synthetic\nparagraph"})
    with pytest.raises(
        ValueError, match=r"\ADifference has no approved equivalent rule\Z"
    ):
        equivalent_matches(
            restricted, {"old": before, "selected": after}, "selected", region="jp"
        )
    assert (
        equivalent_matches(restricted, {"selected": after}, "selected", region="jp")
        == ()
    )


@pytest.mark.parametrize("change", ["selected", "reserved", "unknown"])
def test_policy_inventory_refusals_are_independent(policy: Policy, change: str) -> None:
    template = face(
        object_value(array(policy.rules[1].examples["positive"])[0])["before"]
    )
    selected = "missing" if change == "selected" else "selected"
    observations = {"selected": template}
    if change == "reserved":
        observations["previous"] = template
    elif change == "unknown":
        observations["selected"] = template.model_copy(update={"effect": None})
    message = {
        "selected": "Selected policy observation is outside the full inventory",
        "reserved": "Observation key cannot use the predecessor reserved word",
        "unknown": "Missing text cannot become equivalent",
    }[change]
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        equivalent_matches(policy, observations, selected, region="jp")


def test_policy_path_refusal_precedes_any_blob_read(
    policy: Policy, tmp_path: Path
) -> None:
    pin = RuleSet(
        policy_id=policy.policy_id,
        authored_revision="0" * 40,
        path="authored/unsupported.policy.yaml",
        hash="sha256:" + "0" * 64,
        approval_receipt_hash="sha256:" + "0" * 64,
    )
    with pytest.raises(ValueError, match=r"\AUnsupported wording policy path\Z"):
        load_policy(PinnedRepository(tmp_path), pin)


@pytest.mark.parametrize(
    "change",
    ["policy-format", "rules", "approval-format", "answer", "actions"],
)
def test_policy_and_approval_models_refuse_each_format_violation(
    policy: Policy, change: str
) -> None:
    approval = object_value(
        read_yaml(
            Path(__file__).resolve().parents[2]
            / "authored/wording-rules/145-v1.approval.yaml"
        )
    )
    wire = (
        policy.model_dump(mode="json")
        if change in {"policy-format", "rules"}
        else approval
    )
    model = Policy if change in {"policy-format", "rules"} else Approval
    if change == "policy-format":
        wire["rule_policy_format"] = True
    elif change == "rules":
        wire["rules"] = []
    elif change == "approval-format":
        wire["rule_approval_format"] = True
    elif change == "answer":
        wire["note"] = " "
    else:
        wire["rules"] = []
    message = {
        "policy-format": "Policy format must be an integer",
        "rules": "Policy rules must be sorted, unique and nonempty",
        "approval-format": "Approval format must be an integer",
        "answer": "Policy approval requires an actual answer",
        "actions": "Approved actions must be sorted, unique and nonempty",
    }[change]
    with pytest.raises(ValidationError) as error:
        model.model_validate_json(canonical(wire))
    assert [e["msg"] for e in error.value.errors()] == ["Value error, " + message]
