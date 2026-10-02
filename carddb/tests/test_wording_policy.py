"""Fixed synthetic matcher cases are part of the immutable approved policy."""

import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- inspect immutable synthetic Git inputs
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    string,
)
from sve_carddb.text_observations.models import FaceContent
from sve_carddb.wording_adoptions.classification import classify
from sve_carddb.wording_adoptions.models import RuleSet
from sve_carddb.wording_adoptions.policy import (
    Policy,
    equivalent_matches,
    load_policy,
    verify_matches,
)

if TYPE_CHECKING:
    from pydantic import JsonValue

POLICY_HASH = "sha256:169d7eb1a71beb56bac3df4d5b3f6fd2e02e7acc5998322ebb9926b7b3b97a67"
APPROVAL_HASH = (
    "sha256:a3b941d010708dac6f295ad563b0954bf1d4d7608d0ae77319795c6fc80e8198"
)
POLICY_PATH = "authored/wording-rules/145-v1.policy.yaml"


def face(raw: JsonValue) -> FaceContent:
    value = object_value(raw)
    return FaceContent.model_validate_json(
        canonical(
            {
                "name": value["name"],
                "effect": value["text"],
                "sections": value["sections"],
                "class_raw": value["class"],
                "type_raw": value["type"],
                "stats": [value["cost"], value["attack"], value["defense"]],
                "traits": value["traits"],
                "title": value["title"],
            }
        )
    )


@pytest.fixture(scope="module")
def policy() -> Policy:
    raw = read_yaml(Path(__file__).resolve().parents[2] / POLICY_PATH)
    assert digest(canonical(raw)) == POLICY_HASH
    return Policy.model_validate_json(canonical(raw))


@pytest.mark.parametrize(
    "rule_id",
    [
        "wp:draw-plural-v1",
        "wp:eol-v1",
        "wp:identified-reminder-v1",
        "wp:layout-space-v1",
        "wp:punctuation-v1",
        "wp:repartition-v1",
        "wp:term-token-v1",
    ],
)
def test_each_matcher_version_uses_the_policy_fixed_positive_and_negative_cases(
    policy: Policy, rule_id: str
) -> None:
    rule = next(r for r in policy.rules if r.rule_id == rule_id)
    assert rule.match_condition
    assert rule.exclusions
    assert rule.finite_transform
    for outcome in ("positive", "negative"):
        cases = array(rule.examples[outcome])
        assert cases
        for raw in cases:
            case = object_value(raw)
            parameters = object_value(case["parameters"])
            region = string(case["region"])
            assert region in {"jp", "en"}
            result = classify(
                face(case["before"]),
                face(case["after"]),
                region="jp" if region == "jp" else "en",
                known_reminders=frozenset(
                    integer(v) for v in array(parameters.get("ordinals", []))
                ),
                term_pairs=tuple(
                    (string(array(v)[0]), string(array(v)[1]))
                    for v in array(parameters.get("pairs", []))
                ),
            )
            assert (rule_id in result.rule_ids) == (outcome == "positive"), case[
                "case_id"
            ]
            assert result.report()["automatic_equivalence"] is False


def test_policy_can_be_loaded_from_immutable_yaml(policy: Policy) -> None:
    root = Path(__file__).resolve().parents[2]
    repository = PinnedRepository(root)
    # The consumer branch is rebased after the policy has merged before CI.
    executable = shutil.which("git")
    assert executable is not None
    revision = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed executable and Git arguments without a shell
        [executable, "rev-parse", "HEAD"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    pin = RuleSet(
        policy_id=policy.policy_id,
        authored_revision=revision,
        path=POLICY_PATH,
        hash=POLICY_HASH,
        approval_receipt_hash=APPROVAL_HASH,
    )
    loaded, approval, files = load_policy(repository, pin)
    assert loaded == policy
    assert approval.reviewed_by == "gbaian10"
    assert approval.reviewed_precision == "day"
    assert len(files) == 2
    with pytest.raises(ValueError, match="mismatch"):
        load_policy(
            repository,
            pin.model_copy(update={"approval_receipt_hash": "sha256:" + "0" * 64}),
        )


def test_complete_unicode_diff_to_selected_and_predecessor(policy: Policy) -> None:
    template = face(
        object_value(array(policy.rules[1].examples["positive"])[0])["before"]
    )
    old = template.model_copy(
        update={"effect": "合成Ⓢ\r\nA\r\nB", "sections": ("合成\r\n末",)}
    )
    new = old.model_copy(update={"effect": "合成Ⓢ\nA\nB", "sections": ("合成\n末",)})
    matches = equivalent_matches(
        policy, {"old": old, "selected": new}, "selected", region="jp", previous=old
    )
    assert len(matches) == 6
    assert {m.from_observation_key for m in matches} == {"old", "previous"}
    assert {m.field for m in matches} == {"text", "sections/0"}
    assert next(m for m in matches if m.field == "text").before_range == (3, 4)
    verify_matches(matches, matches)
    for changed in (
        matches[:-1],
        (*matches, matches[0]),
        (matches[0].model_copy(update={"rule_id": "wp:layout-space-v1"}), *matches[1:]),
    ):
        with pytest.raises(ValueError, match="recomputed full diff"):
            verify_matches(changed, matches)
    assert old.effect == "合成Ⓢ\r\nA\r\nB"


@pytest.mark.parametrize(
    "change",
    [
        "space",
        "punctuation",
        "missing",
        "number",
        "standalone",
        "section",
        "unknown",
        "protected",
    ],
)
def test_diagnostic_or_partial_match_cannot_pass_equivalence(
    policy: Policy, change: str
) -> None:
    old = face(object_value(array(policy.rules[1].examples["positive"])[0])["before"])
    assert old.effect is not None
    new = old.model_copy(update={"effect": old.effect.replace("\r\n", "\n")})
    assert new.effect is not None
    effect = new.effect
    if change == "space":
        new = new.model_copy(update={"effect": effect + " "})
    elif change == "punctuation":
        new = new.model_copy(update={"effect": effect + "."})
    elif change == "missing":
        new = new.model_copy(update={"effect": "Synthetic\n"})
    elif change == "number":
        new = new.model_copy(update={"effect": effect + "2"})
    elif change == "standalone":
        old = old.model_copy(update={"sections": ("unchanged\rcontext",)})
        new = new.model_copy(update={"sections": old.sections})
    elif change == "section":
        new = new.model_copy(update={"sections": ("extra",)})
    elif change == "unknown":
        old = old.model_copy(update={"effect": None})
    else:
        new = new.model_copy(update={"stats": ("2", "1", "1")})
    with pytest.raises(ValueError, match=r"Uncovered|CRLF|Missing"):
        equivalent_matches(
            policy, {"old": old, "selected": new}, "selected", region="en"
        )


def test_predecessor_outside_new_scope_is_not_implicitly_checked(
    policy: Policy,
) -> None:
    before = face(
        object_value(array(policy.rules[1].examples["positive"])[0])["before"]
    )
    assert before.effect is not None
    selected = before.model_copy(update={"effect": before.effect.replace("\r\n", "\n")})
    predecessor = before.model_copy(update={"effect": "Synthetic changed rules"})
    with pytest.raises(ValueError, match="CRLF"):
        equivalent_matches(
            policy,
            {"selected": selected},
            "selected",
            region="en",
            previous=predecessor,
        )
