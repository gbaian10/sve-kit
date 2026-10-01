"""Receipt constraints use invented hashes and wording, never official prose."""

import copy
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.build_inputs import BuildContext
from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.text_observations.presence import EffectPresence, PresenceResult
from sve_carddb.wording_adoptions.models import AdoptionRecord, Index, Observation

if TYPE_CHECKING:
    from sve_carddb.registry.records import Region

FACE = "f:" + "1" * 32


def record() -> dict[str, JsonValue]:
    region: Region = "jp"
    observations: list[JsonValue] = []
    evidence: list[JsonValue] = []
    for number in (1, 2):
        source = "src:v1:" + str(number) * 64
        result = PresenceResult(
            source_version_id=source,
            source_index=0,
            template_id="synthetic-card-detail-v1",
            container_locator="synthetic-main",
            state="present",
            reason_code="nonempty_container",
        )
        proof = EffectPresence(
            result=result, result_hash=digest(canonical(result.model_dump(mode="json")))
        )
        observation = Observation(
            observation_key="placeholder",
            printing_id="p:" + str(number) * 32,
            source_index=0,
            source_version_id=source,
            raw_hash=digest(f"synthetic raw {number}".encode()),
            parser_version="synthetic-text-v1",
            raw_face_hash=digest(f"synthetic face {number}".encode()),
            effect_presence=proof,
            corrections=(),
            content_hash=digest(f"synthetic content {number}".encode()),
        )
        observation = observation.model_copy(
            update={"observation_key": observation.key(FACE, region)}
        )
        observations.append(observation.model_dump(mode="json"))
        evidence.append(
            {
                "store_id": "synthetic",
                "batch_id": digest(b"synthetic batch"),
                "source_version_id": source,
                "locator": canonical(
                    {
                        "printing_id": observation.printing_id,
                        "face_id": FACE,
                        "source_index": 0,
                    }
                ).decode(),
                "role": "wording_observation",
            }
        )
    observations.sort(key=lambda o: str(object_value(o)["observation_key"]))
    keys = [object_value(o)["observation_key"] for o in observations]
    context = BuildContext.from_inputs(
        "a" * 40, {"carddb/uv.lock": b"synthetic lock"}, {}
    )
    return {
        "record_key": canonical(["wording_adoption", FACE, region, 1]).decode(),
        "kind": "wording_adoption",
        "filing_key": region,
        "data": {
            "face_id": FACE,
            "region": region,
            "adoption_no": 1,
            "review_context": {
                "context": context.model_dump(mode="json"),
                "source_batches": [
                    {"store_id": "synthetic", "batch_id": digest(b"synthetic batch")}
                ],
            },
            "observations": observations,
            "observations_hash": digest(canonical(observations)),
            "checked_observation_keys": keys,
            "previous": None,
            "equivalence": "equivalent",
            "review": {"mode": "human", "rule_set": None, "rule_matches": []},
            "wording_order": [[key] for key in keys],
            "order_evidence": [
                {
                    "before_level": 0,
                    "after_level": 1,
                    "basis": "reviewed_order",
                    "evidence_indexes": [],
                    "review_receipt": {
                        "reviewed_by": "Synthetic Reviewer",
                        "reviewed_at": "2026-10-01T00:00:00Z",
                        "reviewed_precision": "day",
                        "before_observation_keys": [keys[0]],
                        "after_observation_keys": [keys[1]],
                        "note": "Synthetic explicit adoption order",
                    },
                }
            ],
            "selected_observation_key": keys[-1],
            "previous_order": None,
        },
        "evidence": sorted(evidence, key=canonical),
    }


def test_full_synthetic_receipt_is_structurally_valid_without_claiming_adoption() -> (
    None
):
    receipt = AdoptionRecord.model_validate_json(canonical(record()))
    assert len(receipt.data.observations) == 2
    assert receipt.data.review.mode == "human"


def _membership_change(value: dict[str, JsonValue], change: str) -> None:
    data = object_value(value["data"])
    observations = array(data["observations"])
    first = object_value(observations[0])
    keys = array(data["checked_observation_keys"])
    if change == "extra":
        data["unknown_field"] = True
    elif change == "filing":
        value["filing_key"] = "en"
    elif change == "primary":
        value["record_key"] = "other"
    elif change == "unchecked":
        data["checked_observation_keys"] = keys[:-1]
    elif change == "observations_hash":
        data["observations_hash"] = digest(b"not the checked array")
    elif change in {"raw_pin", "source_index", "presence"}:
        if change == "raw_pin":
            first["raw_face_hash"] = digest(b"different extractor face")
        elif change == "source_index":
            first["source_index"] = 1
        else:
            proof = object_value(first["effect_presence"])
            object_value(proof["result"])["reason_code"] = "empty_container"
            proof["result_hash"] = digest(canonical(proof["result"]))
        data["observations_hash"] = digest(canonical(observations))


def _answer_change(
    edge: dict[str, JsonValue], keys: list[JsonValue], change: str
) -> None:
    answer = object_value(edge["review_receipt"])
    if change == "no_answer":
        edge["review_receipt"] = None
    elif change == "wrong_answer":
        answer["before_observation_keys"] = [keys[1]]
    elif change == "day_precision":
        answer["reviewed_at"] = "2026-10-01T09:00:00Z"


def _order_change(value: dict[str, JsonValue], change: str) -> None:
    data = object_value(value["data"])
    keys = array(data["checked_observation_keys"])
    edge = object_value(array(data["order_evidence"])[0])
    evidence = array(value["evidence"])
    if change == "levels_duplicate":
        data["wording_order"] = [[keys[0]], [keys[0], keys[1]]]
    elif change == "levels_omit":
        data["wording_order"] = [[keys[1]]]
        data["order_evidence"] = []
    elif change == "mixed_level":
        data["wording_order"] = [keys]
        data["order_evidence"] = []
    elif change == "selected_old":
        data["selected_observation_key"] = keys[0]
    elif change == "edge_missing":
        data["order_evidence"] = []
    elif change == "edge_jump":
        edge["after_level"] = 2
    elif change in {"no_answer", "wrong_answer", "day_precision"}:
        _answer_change(edge, keys, change)
    elif change == "fake_official_order":
        edge["basis"] = "printing_availability"
        edge["evidence_indexes"] = [0]
    elif change == "evidence_bounds":
        edge["basis"] = "printing_availability"
        edge["review_receipt"] = None
        edge["evidence_indexes"] = [len(evidence)]


def _evidence_change(value: dict[str, JsonValue], change: str) -> None:
    data = object_value(value["data"])
    observations = array(data["observations"])
    keys = array(data["checked_observation_keys"])
    evidence = array(value["evidence"])
    if change == "missing_observation_evidence":
        value["evidence"] = evidence[:-1]
    elif change == "wrong_locator":
        object_value(evidence[0])["locator"] = "{}"
        value["evidence"] = sorted(evidence, key=canonical)
    elif change == "wrong_batch":
        object_value(
            object_value(
                array(object_value(data["review_context"])["source_batches"])[0]
            )
        )["batch_id"] = digest(b"other batch")
    elif change == "policy_missing":
        object_value(data["review"])["mode"] = "approved_rules"
    elif change == "human_policy":
        object_value(data["review"])["rule_matches"] = [{"unknown": True}]
    elif change == "null_previous_second":
        data["adoption_no"] = 2
        value["record_key"] = canonical(["wording_adoption", FACE, "jp", 2]).decode()
    elif change == "missing_previous_order":
        data["previous"] = {
            "kind": "mechanical",
            "review_context": data["review_context"],
            "observations": observations,
            "observations_hash": data["observations_hash"],
            "selected_observation_key": keys[0],
        }
    else:
        data["previous"] = {
            "kind": "adoption",
            "record_key": "other",
            "record_hash": digest(b"other record"),
            "decision_id": "d:" + "1" * 64,
        }
        data["previous_order"] = {
            "basis": "same_content",
            "evidence_indexes": [],
            "review_receipt": None,
        }


@pytest.mark.parametrize(
    "change",
    [
        "extra",
        "filing",
        "primary",
        "unchecked",
        "observations_hash",
        "raw_pin",
        "source_index",
        "presence",
        "levels_duplicate",
        "levels_omit",
        "mixed_level",
        "selected_old",
        "edge_missing",
        "edge_jump",
        "no_answer",
        "wrong_answer",
        "day_precision",
        "fake_official_order",
        "evidence_bounds",
        "missing_observation_evidence",
        "wrong_locator",
        "wrong_batch",
        "policy_missing",
        "human_policy",
        "null_previous_second",
        "missing_previous_order",
        "first_adoption_predecessor",
    ],
)
def test_each_receipt_constraint_has_an_independent_counterexample(change: str) -> None:
    value = copy.deepcopy(record())
    if change in {
        "extra",
        "filing",
        "primary",
        "unchecked",
        "observations_hash",
        "raw_pin",
        "source_index",
        "presence",
    }:
        _membership_change(value, change)
    elif change in {
        "levels_duplicate",
        "levels_omit",
        "mixed_level",
        "selected_old",
        "edge_missing",
        "edge_jump",
        "no_answer",
        "wrong_answer",
        "day_precision",
        "fake_official_order",
        "evidence_bounds",
    }:
        _order_change(value, change)
    else:
        _evidence_change(value, change)
    with pytest.raises(ValidationError):
        AdoptionRecord.model_validate_json(canonical(value))


@pytest.mark.parametrize("version", [True, "1", 2])
def test_adoption_format_does_not_coerce_or_accept_other_versions(
    version: JsonValue,
) -> None:
    with pytest.raises(ValidationError):
        Index.model_validate_json(
            canonical(
                {
                    "wording_adoption_format": version,
                    "kind": "wording_adoption_index",
                    "includes": {},
                }
            )
        )
