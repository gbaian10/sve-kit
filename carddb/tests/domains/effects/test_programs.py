"""Public programs omit author evidence while retaining expanded execution nodes."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build import Json
from sve_carddb.contracts.author_leaves import AuthorIndexRecord
from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_current import CardAccepted, CurrentAuthorSources
from sve_carddb.domains.effects.author_expansion import ExpansionCandidate
from sve_carddb.domains.effects.author_set import load_author_set
from sve_carddb.domains.effects.programs import populate, public_program

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.effects.author_expansion import AuthorExpander

    from ...support.author_current_fixtures import CurrentSourceFixture


def test_public_program_keeps_face_nodes_and_omits_author_and_source_evidence() -> None:
    record = AuthorIndexRecord(
        card_id="c:" + "a" * 32,
        scope="shared",
        file="rules/author-effects/SYN/000.yaml",
        kind="author_effect_set",
        record_key="SYN-001",
    )
    ability: dict[str, JsonValue] = {
        "line": 1,
        "sources": [{"private": "evidence"}],
        "ports": {"imports": {}, "exports": {}},
        "keywords": ["synthetic-keyword"],
    }
    result = CardAccepted(
        record,
        ExpansionCandidate(
            {
                "format": 1,
                "kind": "author_effect_set",
                "scope": "shared",
                "cards": {
                    "SYN-001": {
                        "meta": {"written_by": "Synthetic author", "status": "draft"},
                        "note": "Private review context",
                        "ignored": [],
                        "abilities": [ability],
                        "faces": {"face": {"abilities": [ability]}},
                    }
                },
            },
            (),
        ),
        b"{}",
        "direct",
        b"{}",
    )
    program = public_program(result)
    expected = {
        "line": 1,
        "ports": {"imports": {}, "exports": {}},
        "keywords": ["synthetic-keyword"],
    }
    assert program == {
        "card_id": record.card_id,
        "scope": "shared",
        "abilities": [expected],
        "faces": {"face": {"abilities": [expected]}},
    }


def test_light_admission_populates_programs_and_manual_states_for_every_card(
    program_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    tmp_path: Path,
) -> None:
    root = tmp_path / "authored"
    main = root / "rules/author-effects/BP01/000.yaml"
    main.parent.mkdir(parents=True)
    main.write_bytes(
        canonical(
            {
                "format": 2,
                "kind": "author_effect_set",
                "scope": "shared",
                "cards": {
                    "BP01-009": {
                        "admission": "light",
                        "container": "card",
                        "meta": {
                            "written_by": [
                                {
                                    "actor": {"who": "Synthetic author", "model": None},
                                    "date": "2026-10-23",
                                }
                            ]
                        },
                        "abilities": [{"line": 1, "evolve": {"cost": {"pp": 1}}}],
                    },
                },
            }
        )
    )
    index = root / "rules/effects-index/BP01.yaml"
    index.parent.mkdir()
    identifier = "c:9c5459446e0254818099800b4208cd96"
    index.write_bytes(
        canonical(
            {
                "format": 1,
                "kind": "effects_index",
                "records": [
                    {
                        "card_id": identifier,
                        "scope": "shared",
                        "kind": "author_effect_set",
                        "record_key": "BP01-009",
                        "file": "rules/author-effects/BP01/000.yaml",
                    }
                ],
            }
        )
    )
    inputs = load_author_set(root, expander.schemas)
    with program_current_fixture.template.copy() as db:
        current = CurrentAuthorSources(
            db,
            program_current_fixture.sources.stage(
                program_current_fixture.sources.build
            ),
            program_current_fixture.snapshot,
            program_current_fixture.vocabulary,
            program_current_fixture.rules,
            program_current_fixture.jp_batch_id,
        )
        with db.transaction():
            populate(db, current, inputs, expander)
        documents = db.rows("dsl_document")
        assert len(documents) == 1
        assert documents[0].values["card_id"] == identifier
        assert documents[0].values["status"] == "draft"
        ast = documents[0].values["ast"]
        assert isinstance(ast, Json)
        assert "meta" not in object_value(ast.value)
        states = db.rows("card_engine_support")
        assert {(r.values["card_id"], r.values["region"]) for r in states} == {
            (r.values["card_id"], r.values["region"]) for r in db.rows("printing")
        }
        assert all(r.values["automatic"] is False for r in states)
        assert {
            r.values["status"] for r in states if r.values["card_id"] == identifier
        } == {"draft"}
        assert {
            r.values["status"] for r in states if r.values["card_id"] != identifier
        } == {"missing_dsl"}
