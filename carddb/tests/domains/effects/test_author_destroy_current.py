"""Real Advance Fanfare maps without dropping the unresolved keyword obligation."""

from typing import TYPE_CHECKING

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import EffectsIndex
from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField
from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains.effects.author_current import CardRejected
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_set import AuthorInputSet
from sve_carddb.domains.effects.author_source_hash import source_version
from sve_carddb.domains.effects.author_sources import (
    SourceCandidate,
    entry_heads,
    entry_payload,
    resolve_source,
)
from sve_carddb.domains.translations.four_layer_destroy import KEY
from sve_carddb.domains.translations.four_layer_sources import semantic_context

from ...support.author_destroy_fixtures import destroy_fixture
from ...support.author_semantic_inputs import load_semantic_inputs
from .author_entry_cases import meta
from .test_author_current import current, input_file, write_inputs
from .test_author_destroy_sources import companion, head
from .test_author_entries import SCHEMAS, ability, shard
from .test_author_expansion import generated

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.effects.author_current import CurrentAuthorSources
    from sve_carddb.domains.effects.author_sources import SourceContext


def test_real_advance_body_requires_complete_keyword_declaration(
    tmp_path: Path,
) -> None:
    fixture = destroy_fixture(tmp_path / "store")
    with current(fixture) as source:
        card_id = source.printing_card("BP19-P32")
        assert source.printing_card("BP19-114") == card_id
        face = source.db.select("face", ("id",), where={"card_id": card_id})[0].values[
            "id"
        ]
        revision = source.db.select(
            "face_current", ("revision_id",), where={"face_id": face, "region": "jp"}
        )[0].values["revision_id"]
        assert isinstance(revision, str)
        owner = FaceRevisionOwner(kind="face_revision", revision_id=revision)
        location = OwnerField(owner=owner, field="effect", ordinal=None)
        _, fields = source._texts([location])
        field = fields[0]
        assert [m.frame.semantic_variant.key if m else None for m in field.matches] == [
            "card_keywords.v2",
            "metadata.layout.v1",
            KEY,
        ]
        matched = field.matches[2]
        assert matched is not None
        assert entry_heads(matched.frame) == ("fanfare",)
        context = semantic_context(source.db, field.source)
        assert context is not None
        assert context.special_kinds == frozenset({"advance"})
        assert context.phase == "advance"
        use = SourceUse(
            binding_id=matched.binding.id,
            occurrence=matched.binding.occurrence_key(),
            ability_scope="ability_0",
        )
        candidate = SourceCandidate(
            field.source.text, field.source.descriptor, source.classifier, context
        )
        resolved = resolve_source(candidate, use)
        assert resolved.frame.id == matched.frame.id
        assert entry_payload(resolved, "ability_0") == {
            **head(),
            "ports": {"imports": {}, "exports": {}},
        }
        meaning = object_value(matched.frame.source.semantic_definitions["resolved"])
        assert (
            object_value(meaning["trigger"])["event"]
            == "enter_field_from_outside_field"
        )
        assert meaning["destination"] == "each_card_owner_cemetery"
        check_current_requires_keyword_declaration(
            source, resolved, candidate, use, card_id, tmp_path
        )


def check_current_requires_keyword_declaration(
    source: CurrentAuthorSources,
    resolved: SourceContext,
    candidate: SourceCandidate,
    use: SourceUse,
    card_id: str,
    tmp_path: Path,
) -> None:
    expander = AuthorExpander(SCHEMAS)
    record = companion(resolved)
    metadata = meta()
    metadata["source"] = source_version(source.card_payload(card_id).payload)
    for number in ("BP19-P32", "BP19-114"):
        for node in (ability(head()), generated(record)):
            node["line"] = 2
            node["sources"] = [use.model_dump(mode="json")]
            file = input_file(
                "rules/author-effects/BP19/000.yaml",
                {
                    "format": 1,
                    "kind": "author_effect_set",
                    "scope": "shared",
                    "cards": {number: {"meta": metadata, "abilities": [node]}},
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
            inputs = AuthorInputSet(
                EffectsIndex.model_validate_json(index.raw),
                index,
                (file,),
                (input_file("rules/effect-bodies/BP19/000.yaml", shard(record)),),
                (),
            )
            root = tmp_path / "authored"
            write_inputs(root, inputs)
            result = source.validate_inputs(
                load_semantic_inputs(root, expander), expander
            )[0]
            assert isinstance(result, CardRejected)
            assert result.diagnostic.reason == "source_keyword_closure_mismatch"
            selected = object_value(
                source.selection_inputs(load_semantic_inputs(root, expander), expander)
            )
            assert (
                object_value(array(selected["authors"])[0])["reason"]
                == "source_keyword_closure_mismatch"
            )
            path = ("cards", number, "abilities", 0)
            expanded = expander.expand_sources(
                file.raw, [canonical(shard(record))], {path: candidate}
            )
            assert expanded.sidecar[0].source_proof == "derived_self_consistent"
