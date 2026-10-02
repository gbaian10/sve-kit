"""Whole-source resolution preserves raw spellings and independent pending causes."""

import copy
import re
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.template_parameter_rules.replay import (
    ProposalInputs,
    Replay,
    compare_replays,
    replay,
)

from .recognition_policy_fixtures import policy_git
from .recognition_replay_fixtures import (
    SourceCase,
    changed_sign_source,
    recognition_source,
)

if TYPE_CHECKING:
    from sve_carddb.template_sources.models import Recipe

__all__ = ("changed_sign_source", "policy_git", "recognition_source")


def changed(case: SourceCase, key: str, value: JsonValue) -> Recipe:
    config = copy.deepcopy(case.recipe.config)
    config[key] = value
    return case.recipe.model_copy(
        update={"config": config, "config_hash": digest(canonical(config))}
    )


def run(case: SourceCase, recipe: Recipe | None = None) -> Replay:
    return replay(
        PinnedRepository(case.repository),
        recipe or case.recipe,
        {"test-store": case.store},
        main_revision=case.main,
        legacy_bytes=case.legacy,
        proposals=case.proposals,
    )


@pytest.fixture(scope="module")
def verified_replay(recognition_source: SourceCase) -> Replay:
    return run(recognition_source)


def test_full_source_replay_resolves_only_authorized_causes(
    verified_replay: Replay,
) -> None:
    result = verified_replay
    assert len(result.numeric_positions) == 1
    assert object_value(parse(result.numeric_positions[0]))["slot"] == "slot_1"
    matched = [object_value(parse(raw)) for raw in result.resolved_slots]
    assert {r["rule_id"] for r in matched} == {
        "suffix_unit_cards",
        "suffix_damage_amount",
        "prefix_cost_delta",
    }
    assert len(matched) == 4
    pending = [object_value(parse(raw)) for raw in result.remaining_slots]
    assert any("missing_card_name_concept" in array(r["issues"]) for r in pending)
    assert any(
        "legacy_parenthesis_classification_requires_review" in array(r["issues"])
        for r in pending
    )
    assert any("vocabulary_not_adopted" in array(r["issues"]) for r in pending)
    assert result.complete is False
    assert object_value(parse(result.source_coverage))["complete"] is True
    assert (
        object_value(object_value(parse(result.checkpoint))["fingerprints"])["expected"]
        == 4
    )
    compare_replays(result, result)


def test_null_policy_does_not_enable_candidate_rules(
    recognition_source: SourceCase,
) -> None:
    recipe = changed(recognition_source, "recognition_policy", None)
    result = run(recognition_source, recipe)
    assert result.policy_pin is None
    assert not result.resolved_slots
    assert result.remaining_slots


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        (
            "config_hash",
            "Recognition parameter recipe ID, program or config hash is invalid",
        ),
        (
            "code_hash",
            "Recognition replay implementation differs from its immutable recipe",
        ),
        (
            "source_recipes",
            "Recognition replay requires its complete source recipe pins",
        ),
        ("batch_scope", "Recognition frozen batch differs from the policy scope"),
        (
            "batch_fields",
            "Recognition replay requires one explicit frozen source batch",
        ),
        ("legacy_hash", "Recognition legacy baseline differs from its recipe hash"),
        (
            "glossary_index",
            "Recognition glossary exact bytes differ from their revision pin",
        ),
        (
            "glossary_shard",
            "Recognition glossary exact bytes differ from their revision pin",
        ),
        (
            "glossary_revision",
            "Recognition glossary exact bytes differ from their revision pin",
        ),
        (
            "glossary_shards_missing",
            "Recognition glossary pin must cover its complete Git file closure",
        ),
        (
            "glossary_canonical",
            "Recognition glossary canonical index or shard pins differ",
        ),
        (
            "glossary_concepts",
            "Recognition glossary exact concept bindings differ from the recipe",
        ),
    ],
)
def test_pinned_source_closure_refusals(  # ruff: ignore[complex-structure,too-many-branches] -- independent malformed pin variants target separate domain refusals
    recognition_source: SourceCase, mutation: str, message: str
) -> None:
    case = recognition_source
    config = copy.deepcopy(case.recipe.config)
    if mutation == "source_recipes":
        config["source_recipes"] = []
    elif mutation == "batch_scope":
        object_value(config["source_batch"])["batch_id"] = digest(b"wrong")
    elif mutation == "batch_fields":
        object_value(config["source_batch"])["extra"] = True
    elif mutation == "legacy_hash":
        config["legacy_file_hash"] = digest(b"wrong")
    elif mutation.startswith("glossary_"):
        glossary = object_value(object_value(config["references"])["glossary"])
        if mutation == "glossary_index":
            glossary["index_hash"] = digest(b"wrong")
        elif mutation == "glossary_shard":
            object_value(array(glossary["shards"])[0])["exact_hash"] = digest(b"wrong")
        elif mutation == "glossary_revision":
            glossary["authored_revision"] = case.loaded.historical_revision
            message = (
                "Recognition glossary pin must cover its complete Git file closure"
            )
        elif mutation == "glossary_shards_missing":
            glossary["shards"] = []
        elif mutation == "glossary_canonical":
            object_value(array(glossary["shards"])[0])["canonical_hash"] = digest(
                b"wrong"
            )
        else:
            object_value(config["references"])["exact_concepts_hash"] = digest(b"wrong")
    recipe = case.recipe.model_copy(
        update={"config": config, "config_hash": digest(canonical(config))}
    )
    if mutation == "config_hash":
        recipe = recipe.model_copy(update={"config_hash": digest(b"wrong")})
    elif mutation == "code_hash":
        recipe = recipe.model_copy(update={"code_hash": digest(b"wrong")})
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        run(case, recipe)


def test_missing_store_is_a_refusal(recognition_source: SourceCase) -> None:
    with pytest.raises(
        ValueError, match=r"^Recognition frozen source store is not configured$"
    ):
        replay(
            PinnedRepository(recognition_source.repository),
            recognition_source.recipe,
            {},
            main_revision=recognition_source.main,
            legacy_bytes=recognition_source.legacy,
        )


@pytest.mark.parametrize("mutation", ["slot", "value", "raw_hash", "source_segments"])
def test_identity_not_total_count(verified_replay: Replay, mutation: str) -> None:
    previous = verified_replay
    identity = object_value(parse(previous.numeric_positions[0]))
    if mutation == "slot":
        identity["slot"] = "slot_other"
    elif mutation == "value":
        identity["value"] = 3
    elif mutation == "raw_hash":
        identity["raw_hash"] = digest(b"wrong")
    else:
        identity["source_segments"] = [{"start": 0, "end": 1}]
    current = replace(previous, numeric_positions=(canonical(identity),))
    assert len(current.numeric_positions) == len(previous.numeric_positions)
    with pytest.raises(
        ValueError, match=r"^Recognition replay changed an existing numeric position$"
    ):
        compare_replays(previous, current)


def test_fingerprint_use_cannot_change_at_equal_count(
    verified_replay: Replay,
) -> None:
    previous = verified_replay
    current = replace(
        previous,
        fingerprints=(digest(b"different").encode(), *previous.fingerprints[1:]),
    )
    with pytest.raises(
        ValueError,
        match=r"^Recognition replay changed a legacy fingerprint or source use$",
    ):
        compare_replays(previous, current)


def test_sign_bridge_requires_zero_original_position_impact(
    changed_sign_source: SourceCase,
) -> None:
    with pytest.raises(
        ValueError,
        match=r"^Recognition sign restriction changes an original numeric position$",
    ):
        run(changed_sign_source)


def test_unknown_coverage_cannot_be_claimed_complete(verified_replay: Replay) -> None:
    unknown = replace(
        verified_replay,
        source_coverage=canonical({"complete": False, "field_states": {"unknown": 1}}),
        remaining_slots=(),
    )
    assert unknown.complete is False
    complete = replace(unknown, source_coverage=canonical({"complete": True}))
    assert complete.complete is True


def test_different_batch_cannot_borrow_an_old_baseline(verified_replay: Replay) -> None:
    recipe = object_value(parse(verified_replay.recipe))
    object_value(object_value(recipe["config"])["source_batch"])["batch_id"] = digest(
        b"other batch"
    )
    different = replace(verified_replay, recipe=canonical(recipe))
    with pytest.raises(
        ValueError,
        match=r"^Recognition replay comparison requires the same frozen batch$",
    ):
        compare_replays(verified_replay, different)


@pytest.mark.parametrize("mutation", ["missing", "vocabulary", "basis"])
def test_proposed_vocabulary_is_pinned_and_never_adopted(
    recognition_source: SourceCase, mutation: str
) -> None:
    case = recognition_source
    proposals = (
        None
        if mutation == "missing"
        else ProposalInputs(
            b"{}" if mutation == "vocabulary" else case.proposals.vocabulary,
            b"different" if mutation == "basis" else case.proposals.basis,
        )
    )
    message = (
        "Recognition replay cannot omit pinned vocabulary proposal inputs"
        if mutation == "missing"
        else "Recognition vocabulary proposal or basis hash differs from its recipe"
    )
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        replay(
            PinnedRepository(case.repository),
            case.recipe,
            {"test-store": case.store},
            main_revision=case.main,
            legacy_bytes=case.legacy,
            proposals=proposals,
        )
