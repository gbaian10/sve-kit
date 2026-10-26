"""Real draws enter only through complete indexed current source validation."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import EffectsIndex
from sve_carddb.core.json import array, canonical, object_value, parse
from sve_carddb.domains.effects.author_current import CardAccepted, CardRejected
from sve_carddb.domains.effects.author_expansion import (
    AuthorExpander,
    ability_nodes,
    equivalent_ability,
)
from sve_carddb.domains.effects.author_set import AuthorInputSet
from sve_carddb.domains.effects.author_source_hash import source_version

from ...support.author_current_fixtures import current_source_fixture
from .author_current_synthetic import effect, token_section
from .author_draw_cases import draw_ability, head
from .author_entry_cases import meta
from .test_author_current import current, input_file, write_inputs
from .test_author_entries import SCHEMAS

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.effects.author_current import CurrentAuthorSources


@dataclass(frozen=True)
class DrawCard:
    number: str
    card_id: str
    face_id: str
    head: str
    count: int
    cost: int | None
    attack: int | None
    defense: int | None


@pytest.fixture(params=["spell", "attack_time", "evolve_time"])
def draw_card(request: pytest.FixtureRequest) -> DrawCard:
    if request.param == "spell":
        return DrawCard(
            "CP02-068",
            "c:0850c7b7bda15bbaa0cbd019e7fba60c",
            "f:5090ff1ac6ec567691ddc8c4584ca8e1",
            "spell",
            3,
            4,
            None,
            None,
        )
    if request.param == "evolve_time":
        return DrawCard(
            "ECP01-002",
            "c:d02c2e5925ce5e7e94d6d7b3ea6be009",
            "f:c36dfb4e110457918ea92a19a9188265",
            "evolve_time",
            1,
            None,
            3,
            3,
        )
    return DrawCard(
        "BP10-122",
        "c:9c92781995b55d629c700b5f25179da6",
        "f:c30e2e641ac0529eb65db319fa54275c",
        "attack_time",
        1,
        2,
        3,
        2,
    )


def collection(
    source: CurrentAuthorSources, card: DrawCard, *, companion: bool
) -> AuthorInputSet:
    complete = source.card_payload(card.card_id)
    matched = complete.fields[0].matches[0]
    assert matched is not None
    use = SourceUse(
        binding_id=matched.binding.id,
        occurrence=matched.binding.occurrence_key(),
        ability_scope="ability_0",
    )
    entry, shards = draw_ability(
        matched.frame, use, companion=companion, count=card.count, name=card.head
    )
    metadata = meta()
    metadata["source"] = source_version(complete.payload)
    file = input_file(
        f"rules/author-effects/{card.number.split('-', 1)[0]}/000.yaml",
        {
            "format": 1,
            "kind": "author_effect_set",
            "scope": "shared",
            "cards": {card.number: {"meta": metadata, "abilities": [entry]}},
        },
    )
    index = input_file(
        "rules/effects-index.yaml",
        {
            "format": 1,
            "kind": "effects_index",
            "records": [
                {
                    "card_id": card.card_id,
                    "scope": "shared",
                    "file": file.path,
                    "kind": "author_effect_set",
                    "record_key": card.number,
                }
            ],
        },
    )
    bodies = tuple(
        input_file("rules/effect-bodies/draw/000.yaml", parse(raw)) for raw in shards
    )
    return AuthorInputSet(
        EffectsIndex.model_validate_json(index.raw), index, (file,), bodies, ()
    )


def check_accepted(result: CardAccepted | CardRejected, card: DrawCard) -> None:
    assert isinstance(result, CardAccepted)
    expected = canonical(
        head(card.count, name=card.head) | {"ports": {"imports": {}, "exports": {}}}
    )
    assert equivalent_ability(next(ability_nodes(result.candidate.tree))[1]) == expected
    evidence = result.candidate.sidecar[0]
    assert evidence.source_proof == "current_source_checked"
    inventory = object_value(object_value(result.lowering_input())["source_inventory"])
    required = object_value(array(inventory["abilities"])[0])
    assert evidence.current_source is not None
    assert required["source_use"] == evidence.current_source.model_dump(mode="json")
    assert canonical(required["body"]) == expected
    payload = object_value(parse(result.source_payload))
    face = object_value(array(payload["faces"])[0])
    assert face["face_id"] == card.face_id
    attributes = object_value(face["attributes"])
    assert attributes["type_code"] == {
        "state": "known",
        "value": "spell" if card.head == "spell" else "follower",
    }
    for stat, value in (
        ("cost", card.cost),
        ("attack", card.attack),
        ("defense", card.defense),
    ):
        assert attributes[stat] == {"state": "known", "value": value}
    if card.head == "evolve_time":
        assert attributes["special_kinds"] == {"state": "known", "value": ["evolve"]}


def check_mutation(
    source: CurrentAuthorSources,
    root: Path,
    inputs: AuthorInputSet,
    change: str,
    reason: str,
    case: DrawCard,
) -> None:
    expander = AuthorExpander(SCHEMAS)
    tree = expander.schemas.author_tree(inputs.author_files[0].raw)
    card = object_value(object_value(object_value(tree)["cards"])[case.number])
    node = object_value(array(card["abilities"])[0])
    if change == "missing":
        card["abilities"] = []
    elif change == "count":
        node[case.head] = head(2, name=case.head)[case.head]
    elif change == "line":
        node["line"] = 2
    elif change == "face":
        card["faces"] = {"f:" + "f" * 32: {"abilities": card.pop("abilities")}}
    else:
        use = object_value(array(node["sources"])[0])
        if change == "binding":
            use["binding_id"] = "bind:" + "f" * 64
        else:
            object_value(use["occurrence"])["owner"] = {
                "kind": "face_revision",
                "revision_id": "revision:stale",
            }
    (root / inputs.author_files[0].path).write_bytes(canonical(tree))
    rejected = source.validate(root, expander)[0]
    assert isinstance(rejected, CardRejected)
    assert rejected.diagnostic.reason == reason


def check_unknown_attributes(
    source: CurrentAuthorSources, root: Path, card: DrawCard
) -> None:
    revision = next(iter(source.card_payload(card.card_id).revisions))
    for attribute in (
        ("cost",)
        if card.head == "spell"
        else ("class_code", "attack", "defense")
        if card.head == "evolve_time"
        else ("cost", "attack", "defense")
    ):
        original = source.db.select(
            "face_revision", (attribute,), where={"id": revision}
        )[0].values[attribute]
        with source.db.transaction():
            source.db.update("face_revision", {"id": revision}, {attribute: None})
        rejected = source.validate(root, AuthorExpander(SCHEMAS))[0]
        assert isinstance(rejected, CardRejected)
        assert rejected.diagnostic.reason == "source_unknown"
        with source.db.transaction():
            source.db.update(
                "face_revision",
                {"id": revision},
                {attribute: original},
            )


def test_real_current_draw(tmp_path: Path, draw_card: DrawCard) -> None:
    fixture = current_source_fixture(
        tmp_path / "sources", card_numbers=frozenset({draw_card.number})
    )
    expander = AuthorExpander(SCHEMAS)
    with current(fixture) as source:
        for companion in (False, True):
            root = tmp_path / str(companion)
            inputs = collection(source, draw_card, companion=companion)
            write_inputs(root, inputs)
            check_accepted(source.validate(root, expander)[0], draw_card)
            selection = object_value(source.selection_input(root, expander))
            selected = object_value(array(selection["authors"])[0])
            assert selected["status"] == "accepted"
            for change, reason in (
                ("missing", "source_ability_closure_mismatch"),
                ("line", "source_location_mismatch"),
                ("binding", "source_occurrence_mismatch"),
                ("owner", "source_current_owner_mismatch"),
                ("face", "source_face_container_mismatch"),
            ):
                check_mutation(source, root, inputs, change, reason, draw_card)
            if not companion:
                check_mutation(
                    source,
                    root,
                    inputs,
                    "count",
                    "source_head_payload_mismatch",
                    draw_card,
                )
            write_inputs(root, inputs)
            check_unknown_attributes(source, root, draw_card)
            if draw_card.head != "spell":
                check_unregistered_sibling(source, root, draw_card, companion=companion)
        negative_root = tmp_path / "negative-sources"
        write_inputs(negative_root, collection(source, draw_card, companion=False))
        if draw_card.head == "evolve_time":
            check_evolve_kind_evidence(source, negative_root, draw_card)
        elif draw_card.head == "attack_time":
            check_other_kinds_keep_unknown_cost(source, negative_root, draw_card)
        check_unadmitted_current_sources(source, negative_root, draw_card)


def check_evolve_kind_evidence(
    source: CurrentAuthorSources, root: Path, card: DrawCard
) -> None:
    revision = next(iter(source.card_payload(card.card_id).revisions))
    for other_kind in ("advance", "token"):
        with source.db.transaction():
            source.db.update(
                "face_special_kind",
                {"revision_id": revision, "special_kind_code": "evolve"},
                {"special_kind_code": other_kind},
            )
        rejected = source.validate(root, AuthorExpander(SCHEMAS))[0]
        assert isinstance(rejected, CardRejected)
        assert rejected.diagnostic.reason == "source_pending"
        with source.db.transaction():
            source.db.update(
                "face_special_kind",
                {"revision_id": revision, "special_kind_code": other_kind},
                {"special_kind_code": "evolve"},
            )


def check_other_kinds_keep_unknown_cost(
    source: CurrentAuthorSources, root: Path, card: DrawCard
) -> None:
    revision = next(iter(source.card_payload(card.card_id).revisions))
    for other_kind in ("advance", "token"):
        with source.db.transaction():
            source.db.update("face_revision", {"id": revision}, {"cost": None})
            source.db.insert(
                "face_special_kind",
                {
                    "revision_id": revision,
                    "special_kind_code": other_kind,
                    "special_kind_kind": "special_kind",
                },
            )
        # The attack source still resolves, so rejection exercises unknown cost.
        rejected = source.validate(root, AuthorExpander(SCHEMAS))[0]
        assert isinstance(rejected, CardRejected)
        assert rejected.diagnostic.reason == "source_unknown"
        with source.db.transaction():
            source.db.update("face_revision", {"id": revision}, {"cost": card.cost})
            source.db.delete(
                "face_special_kind",
                {"revision_id": revision, "special_kind_code": other_kind},
            )


def check_unregistered_sibling(
    source: CurrentAuthorSources, root: Path, card: DrawCard, *, companion: bool
) -> None:
    revision = next(iter(source.card_payload(card.card_id).revisions))
    original = source.db.select(
        "face_revision", ("effect_unit_id",), where={"id": revision}
    )[0].values["effect_unit_id"]
    with pytest.MonkeyPatch.context() as patch:
        effect(
            source,
            patch,
            (
                "【攻撃時】1枚引く。\n【攻撃時】2枚引く。"
                if card.head == "attack_time"
                else "【進化時】1枚引く。\n【進化時】2枚引く。"
            ),
            revision=revision,
        )
        # A new sibling needs registry admission even when its draw shape is supported.
        write_inputs(root, collection(source, card, companion=companion))
        expander = AuthorExpander(SCHEMAS)
        rejected = source.validate(root, expander)[0]
        assert isinstance(rejected, CardRejected)
        assert rejected.diagnostic.reason == "source_registry_mismatch"
        selection = object_value(source.selection_input(root, expander))
        assert object_value(array(selection["authors"])[0])["status"] == "rejected"
    with source.db.transaction():
        source.db.update(
            "face_revision", {"id": revision}, {"effect_unit_id": original}
        )


def check_unadmitted_current_sources(
    source: CurrentAuthorSources, root: Path, card: DrawCard
) -> None:
    revision = next(iter(source.card_payload(card.card_id).revisions))
    original = source.db.select(
        "face_revision", ("effect_unit_id",), where={"id": revision}
    )[0].values["effect_unit_id"]
    expander = AuthorExpander(SCHEMAS)
    for text in (
        "3枚まで引く。",
        "相手は3枚引く。",
        "お互いに3枚引く。",
        "自分の場のフォロワーの数と同じ枚数を引く。",
        "自分の手札が3枚以下なら、3枚引く。",
        "手札1枚を捨てる：3枚引く。",
    ):
        with pytest.MonkeyPatch.context() as patch:
            effect(
                source,
                patch,
                (
                    {
                        "spell": "",
                        "attack_time": "【攻撃時】",
                        "evolve_time": "【進化時】",
                    }[card.head]
                )
                + text,
                revision=revision,
            )
            rejected = source.validate(root, expander)[0]
            assert isinstance(rejected, CardRejected)
            assert rejected.diagnostic.reason in {
                "source_pending",
                "source_unresolved_issue",
            }
    with source.db.transaction():
        source.db.update(
            "face_revision", {"id": revision}, {"effect_unit_id": original}
        )
    if card.head != "spell":
        prefix = "【攻撃時】" if card.head == "attack_time" else "【進化時】"
        for text in (
            "{クイック}\n" + prefix + "1枚引く。",
            prefix + "1枚引く。\n未知の追加能力。",
        ):
            with pytest.MonkeyPatch.context() as patch:
                effect(source, patch, text, revision=revision)
                rejected = source.validate(root, expander)[0]
                assert isinstance(rejected, CardRejected)
                assert rejected.diagnostic.reason in {
                    "source_pending",
                    "source_unresolved_issue",
                }
        with source.db.transaction():
            source.db.update(
                "face_revision", {"id": revision}, {"effect_unit_id": original}
            )
    with pytest.MonkeyPatch.context() as patch:
        token_section(
            source,
            patch,
            revision,
            text="『仮トークン』{ニュートラル}フォロワー{コスト1}{攻撃力}1/{体力}1\n"
            + (
                {"spell": "", "attack_time": "【攻撃時】", "evolve_time": "【進化時】"}[
                    card.head
                ]
            )
            + "3枚引く。",
            kind="unknown",
        )
        rejected = source.validate(root, expander)[0]
        assert isinstance(rejected, CardRejected)
        assert rejected.diagnostic.reason == "token_source_contract_unavailable"
