"""Each model refusal uses a minimal shape rather than a multi-error YAML input."""

import pytest
from pydantic import ValidationError

from sve_carddb.snapshot.values import canonical
from sve_carddb.template_parameter_rules.cases import _example, fixed_examples
from sve_carddb.template_parameter_rules.models import (
    Approval,
    CaseInput,
    Examples,
    Policy,
)

from .recognition_policy_fixtures import pair


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("roles_gap", "Recognition case roles must partition the full field"),
        ("roles_short", "Recognition case roles must partition the full field"),
        ("slots_reversed", "Recognition case slots must be sorted inside the field"),
        ("role_mismatch", "Recognition case slot disagrees with its role"),
        ("issues_order", "Recognition items must be sorted and unique"),
    ],
)
def test_case_evidence_shape(mutation: str, message: str) -> None:
    case = _example("shape", "甲2回復", "2", True, ()).input.model_dump(mode="json")
    if mutation == "roles_gap":
        case["role_spans"] = [{"role": "body", "start": 1, "end": 5}]
    elif mutation == "roles_short":
        case["role_spans"] = [{"role": "body", "start": 0, "end": 3}]
    elif mutation == "slots_reversed":
        case["slot_segments"] = [{"start": 2, "end": 3}, {"start": 1, "end": 2}]
    elif mutation == "role_mismatch":
        case["role"] = "reminder"
    else:
        case["original_issues"] = ["z_reason", "a_reason"]
    with pytest.raises(ValidationError) as error:
        CaseInput.model_validate_json(canonical(case))
    assert len(error.value.errors()) == 1
    assert error.value.errors()[0]["msg"] == "Value error, " + message


def test_full_partition_overflow() -> None:
    case = _example("shape", "甲2回復", "2", True, ()).input.model_dump(mode="json")
    case["role_spans"] = [{"role": "body", "start": 0, "end": 20}]
    with pytest.raises(ValidationError) as error:
        CaseInput.model_validate_json(canonical(case))
    assert len(error.value.errors()) == 1
    assert (
        error.value.errors()[0]["msg"]
        == "Value error, " + "Recognition case roles must partition the full field"
    )


@pytest.mark.parametrize("mutation", ["duplicate_id", "wrong_answer"])
def test_fixed_case_lists_are_closed(mutation: str) -> None:
    examples = fixed_examples("suffix_recovery_amount")
    bad = examples.model_dump(mode="json")
    if mutation == "duplicate_id":
        bad["negative"][0]["case_id"] = bad["positive"][0]["case_id"]
        message = "Recognition fixed case IDs must be unique"
    else:
        bad["positive"][0]["expected_match"] = False
        message = "Recognition fixed case answers disagree with their group"
    with pytest.raises(ValidationError) as error:
        Examples.model_validate_json(canonical(bad))
    assert len(error.value.errors()) == 1
    assert error.value.errors()[0]["msg"] == "Value error, " + message


def test_policy_precedence_is_not_array_order() -> None:
    policy, _ = pair("a" * 40)
    policy["precedence"] = ["recognition_rules", "existing_numeric_rules"]
    with pytest.raises(ValidationError) as error:
        Policy.model_validate_json(canonical(policy))
    assert len(error.value.errors()) == 1
    assert (
        error.value.errors()[0]["msg"]
        == "Value error, " + "Recognition policy precedence cannot change"
    )


def test_approval_requires_events() -> None:
    _, receipt = pair("a" * 40)
    receipt["events"] = {}
    with pytest.raises(ValidationError) as error:
        Approval.model_validate_json(canonical(receipt))
    assert len(error.value.errors()) == 1
    assert (
        error.value.errors()[0]["msg"]
        == "Value error, " + "Recognition approval requires a real event"
    )
