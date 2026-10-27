"""A real token's resolved movement never hides its pending leave-field sibling."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import EffectsIndex
from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField
from sve_carddb.core.json import array, canonical, object_value, parse
from sve_carddb.domains.effects.author_current import CardRejected
from sve_carddb.domains.effects.author_expansion import (
    AuthorExpander,
    ability_nodes,
    equivalent_ability,
)
from sve_carddb.domains.effects.author_set import AuthorInputSet
from sve_carddb.domains.effects.author_sources import entry_payload
from sve_carddb.domains.translations.four_layer_semantics import CardContext
from tests.support.author_source_fixtures import SourceCandidate, source_context

from ...support.author_move_fixtures import FRAME, NUMBERS
from ...support.author_semantic_inputs import load_semantic_inputs
from ..translations.test_four_layer_fixed_actions import MOVE_RAW
from .author_entry_cases import meta
from .author_move_cases import head, move_ability
from .test_author_current import current, input_file, write_inputs

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.domains.effects.author_current import CurrentAuthorSources

    from ...support.author_current_fixtures import CurrentSourceFixture


def text(value: object) -> str:
    assert isinstance(value, str)
    return value


def real_source(
    source: CurrentAuthorSources,
) -> tuple[SourceCandidate, SourceUse, Frame]:
    cards = {source.printing_card(number) for number in NUMBERS}
    assert len(cards) == 1
    card_id = cards.pop()
    faces = source.db.select("face", ("id",), where={"card_id": card_id})
    assert len(faces) == 1
    face_id = text(faces[0].values["id"])
    revision = text(
        source.db.select(
            "face_current",
            ("revision_id",),
            where={"face_id": face_id, "region": "jp"},
        )[0].values["revision_id"]
    )
    assert (
        source.db.select("face_revision", ("type_code",), where={"id": revision})[
            0
        ].values["type_code"]
        == "amulet"
    )
    assert [
        row.values["special_kind_code"]
        for row in source.db.select(
            "face_special_kind", ("special_kind_code",), where={"revision_id": revision}
        )
    ] == ["token"]
    owner = FaceRevisionOwner(kind="face_revision", revision_id=revision)
    _, fields = source._texts([OwnerField(owner=owner, field="effect", ordinal=None)])
    field = fields[0]
    assert field.source.text == MOVE_RAW
    assert field.derived.pending_causes[0]
    matched = field.matches[2]
    assert matched is not None
    assert matched.frame.id == FRAME
    assert matched.binding.occurrence_key().owner == owner
    assert matched.binding.line_ordinal == 1
    use = SourceUse(
        binding_id=matched.binding.id,
        occurrence=matched.binding.occurrence_key(),
        ability_scope="ability_0",
    )
    candidate = SourceCandidate(
        field.source.text,
        field.field.source,
        source.classifier,
        CardContext(
            field.field.source,
            card_id,
            face_id,
            "normal",
            "amulet",
            True,
            0,
            frozenset({"token"}),
        ),
    )
    return candidate, use, matched.frame


def check_amulet_stats_stay_unknown(
    source: CurrentAuthorSources, use: SourceUse
) -> None:
    owner = use.occurrence.owner
    assert isinstance(owner, FaceRevisionOwner)
    row = source.db.select(
        "face_revision",
        source.db.columns("face_revision"),
        where={"id": owner.revision_id},
    )[0].values
    attributes = object_value(source._attributes(row))
    for key in ("attack", "defense"):
        assert attributes[key] == {"state": "unknown", "value": None}


def collection(
    number: str, card_id: str, entry: JsonValue, bodies: list[bytes]
) -> AuthorInputSet:
    file = input_file(
        "rules/author-effects/BP07/000.yaml",
        {
            "format": 1,
            "kind": "author_effect_set",
            "scope": "shared",
            "cards": {number: {"meta": meta(), "abilities": [entry]}},
        },
    )
    index = input_file(
        "rules/effects-index.yaml",
        {
            "format": 1,
            "kind": "effects_index",
            "records": [
                {
                    "card_id": card_id,
                    "scope": "shared",
                    "file": file.path,
                    "kind": "author_effect_set",
                    "record_key": number,
                }
            ],
        },
    )
    return AuthorInputSet(
        EffectsIndex.model_validate_json(index.raw),
        index,
        (file,),
        tuple(
            input_file("rules/effect-bodies/BP07/000.yaml", parse(raw))
            for raw in bodies
        ),
        (),
    )


def test_real_permanent_token_sources_and_whole_card_rejection(
    move_current_fixture: CurrentSourceFixture, expander: AuthorExpander, tmp_path: Path
) -> None:
    with current(move_current_fixture) as source:
        candidate, use, frame = real_source(source)
        check_amulet_stats_stay_unknown(source, use)
        expected = canonical(head() | {"ports": {"imports": {}, "exports": {}}})
        assert (
            canonical(entry_payload(source_context(candidate, use), "ability_0"))
            == expected
        )
        for number in NUMBERS:
            for companion in (False, True):
                entry, bodies = move_ability(frame, use, companion=companion)
                inputs = collection(number, source.printing_card(number), entry, bodies)
                result = expander.expand_contexts(
                    inputs.author_files[0].raw,
                    bodies,
                    {("cards", number, "abilities", 0): source_context(candidate, use)},
                )
                assert (
                    equivalent_ability(next(ability_nodes(result.tree))[1]) == expected
                )
                assert result.sidecar[0].source_proof == "none"
                root = tmp_path / number / str(companion)
                write_inputs(root, inputs)
                rejected = source.validate_inputs(
                    load_semantic_inputs(root, expander), expander
                )[0]
                assert isinstance(rejected, CardRejected)
                assert rejected.diagnostic.reason == "source_pending"
                selected = object_value(
                    source.selection_inputs(
                        load_semantic_inputs(root, expander), expander
                    )
                )
                assert object_value(array(selected["authors"])[0]) == {
                    "status": "rejected",
                    "card_no": number,
                    "reason": "source_pending",
                }
