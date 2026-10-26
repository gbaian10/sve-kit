"""Committed anchors reject stale or incomplete current author programs."""

from copy import deepcopy
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.author_leaves import EffectsIndex
from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains.effects import author_current
from sve_carddb.domains.effects.author_anchors import (
    input_file,
    materialize_inputs,
    review_states,
)
from sve_carddb.domains.effects.author_current import CardAccepted, CardRejected
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_inputs import AuthorInputError, parse_document
from sve_carddb.domains.effects.author_set import AuthorInputSet

from ...support.author_current_fixtures import current_source_fixture, entry_snapshot
from ...support.time_budget import slow_fixture
from .author_current_synthetic import effect
from .test_author_current import current
from .test_author_entries import SCHEMAS

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.domains.effects.author_current import CurrentAuthorSources
    from sve_carddb.domains.effects.author_inputs import Diagnostic

    from ...support.author_current_fixtures import CurrentSourceFixture

NUMBER = "BP10-122"
BODY_KEY = "draw/1"
WRITER: list[JsonValue] = [
    {"actor": {"who": "Synthetic author", "model": None}, "date": "2026-10-26"}
]


@pytest.fixture(scope="module")
@slow_fixture(
    reason="Build the real current-source database for committed anchor validation "
    "once for this module; each test copies the database before writing."
)
def fixture(
    tmp_path_factory: pytest.TempPathFactory, author_source_preparation: bytes
) -> CurrentSourceFixture:
    return current_source_fixture(
        tmp_path_factory.mktemp("anchors"),
        card_numbers=frozenset({NUMBER}),
        preparation=author_source_preparation,
    )


@pytest.fixture(scope="module")
def expander() -> AuthorExpander:
    return AuthorExpander(SCHEMAS)


def collection(
    source: CurrentAuthorSources, expander: AuthorExpander
) -> AuthorInputSet:
    card_id = source.printing_card(NUMBER)
    file = input_file(
        "rules/author-effects/BP10/000.yaml",
        {
            "format": 2,
            "kind": "author_effect_set",
            "cards": {
                NUMBER: {
                    "meta": {"written_by": WRITER},
                    "abilities": [{"line": 1, "use": "draw", "params": {"n": 1}}],
                }
            },
        },
    )
    body = input_file(
        "rules/effect-bodies/BP10/000.yaml",
        {
            "format": 2,
            "kind": "effect_body_shard",
            "records": [
                {
                    "id": "draw",
                    "params": {"n": "Nat"},
                    "body": {
                        "attack_time": {
                            "do": [{"draw": {"who": "self", "n": {"param": "n"}}}]
                        }
                    },
                    "written_by": WRITER,
                }
            ],
        },
    )
    index = EffectsIndex.model_validate_json(
        canonical(
            {
                "format": 1,
                "kind": "effects_index",
                "records": [
                    {
                        "card_id": card_id,
                        "scope": "shared",
                        "file": file.path,
                        "kind": "author_effect_set",
                        "record_key": NUMBER,
                    }
                ],
            }
        )
    )
    inputs = AuthorInputSet(
        index,
        input_file("rules/effects-index.yaml", index.model_dump(mode="json")),
        (file,),
        (body,),
        (),
    )
    result = materialize_inputs(inputs, source, expander, refresh=frozenset({NUMBER}))
    assert result.failures == {}
    return replace(
        inputs,
        anchor_files=(
            input_file("rules/effect-anchors/BP10.yaml", result.anchors["BP10"]),
        ),
    )


def anchor_tree(inputs: AuthorInputSet) -> dict[str, JsonValue]:
    return object_value(parse_document(inputs.anchor_files[0].raw).value)


def replace_anchor(
    inputs: AuthorInputSet, tree: dict[str, JsonValue]
) -> AuthorInputSet:
    return replace(
        inputs, anchor_files=(input_file(inputs.anchor_files[0].path, tree),)
    )


def rejection(
    source: CurrentAuthorSources, inputs: AuthorInputSet, expander: AuthorExpander
) -> Diagnostic:
    try:
        results = source.validate_inputs(inputs, expander)
    except AuthorInputError as error:
        return error.diagnostic
    assert len(results) == 1
    assert isinstance(results[0], CardRejected)
    return results[0].diagnostic


def test_current_anchor_grants_the_same_complete_semantic_handoff(
    fixture: CurrentSourceFixture, expander: AuthorExpander
) -> None:
    with current(fixture) as source:
        inputs = collection(source, expander)
        first = source.validate_inputs(inputs, expander)
        second = source.validate_inputs(inputs, expander)
        assert first == second
        assert isinstance(first[0], CardAccepted)
        tree = anchor_tree(inputs)
        body = object_value(object_value(tree["bodies"])[BODY_KEY])
        assert body["params"] == {
            "n": {"type": "Nat", "source": {"slot": "leaf_0", "component": "value"}}
        }
        assert "body" not in body
        handoff = object_value(first[0].lowering_input())
        assert object_value(
            object_value(object_value(handoff["tree"])["cards"])[NUMBER]
        )["meta"]
        assert handoff["source_inventory"]
        assert (
            object_value(array(handoff["sidecar"])[0])["input_hash"]
            == inputs.author_files[0].input_hash
        )


@pytest.mark.parametrize(
    ("field", "stage"),
    [
        ("source", "source"),
        ("basis", "anchor"),
        ("binding_id", "binding"),
        ("frame_id", "frame"),
        ("face_id", "binding"),
        ("line", "binding"),
        ("scope", "binding"),
        ("body_version", "body"),
        ("params", "params"),
        ("registration", "body"),
        ("ports", "dependency"),
        ("count", "anchor"),
    ],
)
def test_tampered_anchor_rejects_the_whole_card(  # ruff: ignore[complex-structure] -- exercise independently tampered exact anchor fields
    fixture: CurrentSourceFixture, expander: AuthorExpander, field: str, stage: str
) -> None:
    with current(fixture) as source:
        inputs = collection(source, expander)
        tree = anchor_tree(inputs)
        card = object_value(object_value(tree["cards"])[NUMBER])
        use = object_value(array(card["uses"])[0])
        match field:
            case "source":
                card["source"] = "effect-source-v1:sha256:" + "0" * 64
            case "basis":
                card["basis"] = "effect-anchor-v1:sha256:" + "0" * 64
            case "binding_id":
                use[field] = "bind:" + "0" * 64
            case "frame_id":
                use[field] = "frame:" + "0" * 64
            case "face_id":
                use[field] = "f:" + "0" * 32
            case "line" | "scope":
                use[field] = 2
            case "body_version":
                use[field] = "effect-body-v1:sha256:" + "0" * 64
            case "params":
                use[field] = {"n": {"type": "Nat", "value": 2}}
            case "registration":
                use[field] = "other"
            case "ports":
                use[field] = {"imports": {}, "exports": {}}
            case "count":
                array(card["uses"]).append(deepcopy(use))
        changed = replace_anchor(inputs, tree)
        (result,) = source.validate_inputs(changed, expander)
        assert isinstance(result, CardRejected)
        assert result.diagnostic.stage == stage
        assert result.diagnostic.reason == (
            "anchor_use_closure" if field == "count" else "stale_anchor"
        )
        assert result.diagnostic.card_no == NUMBER
        assert (
            changed.anchor_files[0].raw
            == input_file(changed.anchor_files[0].path, tree).raw
        )


@pytest.mark.parametrize(
    ("mutation", "reason"),
    [
        ("missing_card", "missing_card_anchor"),
        ("missing_body", "missing_body_anchor"),
        ("extra_card", "extra_card_anchor"),
        ("extra_body", "extra_body_anchor"),
        ("body_basis", "stale_body_anchor"),
        ("extra_owner", "extra_anchor_owner"),
        ("wrong_owner", "wrong_anchor_owner"),
        ("recipe", "unsupported_anchor_recipe"),
    ],
)
def test_anchor_inventory_is_closed(
    fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    mutation: str,
    reason: str,
) -> None:
    with current(fixture) as source:
        inputs = collection(source, expander)
        tree = anchor_tree(inputs)
        cards, bodies = object_value(tree["cards"]), object_value(tree["bodies"])
        match mutation:
            case "missing_card":
                cards.clear()
            case "missing_body":
                bodies.clear()
            case "extra_card":
                cards["BP10-999"] = deepcopy(cards[NUMBER])
            case "extra_body":
                bodies["other/1"] = deepcopy(bodies[BODY_KEY])
            case "body_basis":
                object_value(bodies[BODY_KEY])["basis"] = (
                    "effect-anchor-v1:sha256:" + "0" * 64
                )
            case "extra_owner":
                tree.update({"owner": "OTHER", "cards": {}, "bodies": {}})
            case "wrong_owner":
                tree["owner"] = "BP01"
            case "recipe":
                tree["recipe"] = "effect-anchor-v0"
        changed = replace_anchor(inputs, tree)
        if mutation == "extra_owner":
            changed = replace(
                changed,
                anchor_files=(input_file("rules/effect-anchors/OTHER.yaml", tree),),
            )
        assert rejection(source, changed, expander).reason == reason


@pytest.mark.parametrize("recipe", ["adapter_recipe", "expansion_recipe"])
def test_recipe_change_never_refreshes_pins(
    fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
    recipe: str,
) -> None:
    with current(fixture) as source:
        inputs = collection(source, expander)
        before = inputs.anchor_files
        monkeypatch.setattr(expander, recipe, "sha256:" + "0" * 64)
        (result,) = source.validate_inputs(inputs, expander)
        assert isinstance(result, CardRejected)
        assert result.diagnostic.stage == "anchor"
        assert inputs.anchor_files == before


@pytest.mark.parametrize(
    "field",
    [
        "line",
        "face",
        "section",
        "scope",
        "params",
        "body",
        "normalizer",
        "classification",
        "rulings",
        "text",
    ],
)
def test_current_changes_require_explicit_rebinding(
    fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
    field: str,
) -> None:
    with current(fixture) as source:
        inputs = collection(source, expander)
        main = object_value(parse_document(inputs.author_files[0].raw).value)
        card = object_value(object_value(main["cards"])[NUMBER])
        ability = object_value(array(card["abilities"])[0])
        if field in {"line", "face", "section", "scope"}:
            ability[field] = 99
        elif field == "params":
            ability[field] = {"n": 2}
        elif field == "rulings":
            object_value(card["meta"])["rulings"] = ["synthetic-ruling"]
        elif field == "body":
            body = object_value(parse_document(inputs.body_files[0].raw).value)
            object_value(array(body["records"])[0])["body"] = {
                "attack_time": {"do": []}
            }
            inputs = replace(
                inputs, body_files=(input_file(inputs.body_files[0].path, body),)
            )
        elif field == "text":
            revision = next(
                iter(source.card_payload(source.printing_card(NUMBER)).revisions)
            )
            effect(source, monkeypatch, "【攻撃時】2枚引く。", revision=revision)
        elif field == "normalizer":
            monkeypatch.setattr(author_current, "VERSION", "four-layer-jp-v999")
        elif field == "classification":
            incomplete = replace(
                fixture,
                snapshot=entry_snapshot(
                    exclude_semantics=frozenset({"draw_exact_attack_time.v1"})
                ),
            )
            with current(incomplete) as changed_source:
                (result,) = changed_source.validate_inputs(inputs, expander)
                assert isinstance(result, CardRejected)
            return
        inputs = replace(
            inputs, author_files=(input_file(inputs.author_files[0].path, main),)
        )
        try:
            (result,) = source.validate_inputs(inputs, expander)
        except AuthorInputError:
            return
        assert isinstance(result, CardRejected)
        assert result.diagnostic.card_no == NUMBER


def test_review_freshness_uses_literal_and_preserves_disputed(
    fixture: CurrentSourceFixture, expander: AuthorExpander
) -> None:
    with current(fixture) as source:
        inputs = collection(source, expander)
        tree = anchor_tree(inputs)
        basis = object_value(object_value(tree["cards"])[NUMBER])["basis"]
        main = object_value(parse_document(inputs.author_files[0].raw).value)
        metadata = object_value(
            object_value(object_value(main["cards"])[NUMBER])["meta"]
        )
        use = object_value(
            array(object_value(object_value(tree["cards"])[NUMBER])["uses"])[0]
        )
        dependencies: dict[str, JsonValue] = {
            "sources": [
                {key: use[key] for key in ("binding_id", "occurrence", "ability_scope")}
            ],
            "frame_id": use["frame_id"],
            "ability_scope": use["ability_scope"],
            "semantic_variant": {
                "state": "resolved",
                "key": "draw_exact_attack_time.v1",
                "scope": None,
            },
            "body_version": use["body_version"],
            "interfaces": [],
            "source": object_value(object_value(tree["cards"])[NUMBER])["source"],
            "rulings": [],
            "expansion_recipe": {"id": "synthetic", "hash": "sha256:" + "0" * 64},
            "adapter_recipe": {"id": "synthetic", "hash": "sha256:" + "0" * 64},
            "imports": {},
            "applicability_scope": "jp",
            "domain_hash": "sha256:" + "0" * 64,
            "params": use["params"],
        }
        review: dict[str, JsonValue] = {
            "kind": "use",
            "reviewer": {"who": "Synthetic reviewer", "model": None},
            "date": "2026-10-26",
            "verdict": "ok",
            "reason": None,
            "dependencies": dependencies,
        }
        metadata["reviewed_by"] = [
            review,
            {**review, "anchor": basis},
            {
                **review,
                "verdict": "disputed",
                "anchor": "effect-anchor-v1:sha256:" + "0" * 64,
            },
        ]
        inputs = replace(
            inputs, author_files=(input_file(inputs.author_files[0].path, main),)
        )
        body_tree = object_value(parse_document(inputs.body_files[0].raw).value)
        body_dependencies: dict[str, JsonValue] = {
            key: value
            for key, value in dependencies.items()
            if key not in {"semantic_variant", "params", "imports", "source"}
        }
        body_dependencies["sources"] = [
            {**object_value(item), "source": dependencies["source"]}
            for item in array(dependencies["sources"])
        ]
        body_review = {**review, "kind": "body", "dependencies": body_dependencies}
        body_basis = object_value(object_value(tree["bodies"])[BODY_KEY])["basis"]
        object_value(array(body_tree["records"])[0])["registrations"] = [
            {
                "id": "jp",
                "domain": {"regions": ["jp"]},
                "reviews": [body_review, {**body_review, "anchor": body_basis}],
            }
        ]
        inputs = replace(
            inputs, body_files=(input_file(inputs.body_files[0].path, body_tree),)
        )
        assert isinstance(source.validate_inputs(inputs, expander)[0], CardAccepted)
        reports = [object_value(r) for r in review_states(inputs)]
        assert [r["state"] for r in reports] == [
            "stale",
            "recorded",
            "disputed",
            "stale",
            "recorded",
        ]
        assert reports[2]["anchor_matches"] is False
        object_value(object_value(tree["cards"])[NUMBER])["basis"] = (
            "effect-anchor-v1:sha256:" + "1" * 64
        )
        changed = replace_anchor(inputs, tree)
        assert object_value(review_states(changed)[1])["state"] == "stale"
        assert inputs.author_files == changed.author_files


def test_unreferenced_body_without_a_pin_is_rejected(
    fixture: CurrentSourceFixture, expander: AuthorExpander
) -> None:
    with current(fixture) as source:
        inputs = collection(source, expander)
        body = object_value(parse_document(inputs.body_files[0].raw).value)
        extra = deepcopy(object_value(array(body["records"])[0]))
        extra["id"] = "unbound"
        array(body["records"]).append(extra)
        changed = replace(
            inputs, body_files=(input_file(inputs.body_files[0].path, body),)
        )
        with pytest.raises(AuthorInputError) as error:
            source.validate_inputs(changed, expander)
        assert error.value.diagnostic.reason == "unbound_body_definition"


def test_mixed_storage_formats_are_rejected(
    fixture: CurrentSourceFixture, expander: AuthorExpander
) -> None:
    with current(fixture) as source:
        inputs = collection(source, expander)
        body = object_value(parse_document(inputs.body_files[0].raw).value)
        body["format"] = 1
        changed = replace(
            inputs, body_files=(input_file(inputs.body_files[0].path, body),)
        )
        with pytest.raises(AuthorInputError) as error:
            source.validate_inputs(changed, expander)
        assert error.value.diagnostic.reason == "mixed_storage_formats"
