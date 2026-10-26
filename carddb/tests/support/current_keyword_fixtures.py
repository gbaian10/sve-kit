"""A real printed declaration through C0 selection, generated for Rust at test time."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_current import CurrentAuthorSources
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_source_hash import source_version

from .author_current_fixtures import REPOSITORY, current_source_fixture, entry_snapshot

if TYPE_CHECKING:
    from pathlib import Path


def current_keyword_inputs(root: Path) -> JsonValue:
    """Rebuild BP01-138 from reviewed public inputs and complete current registry matches."""
    fixture = current_source_fixture(
        root / "sources",
        card_numbers=frozenset({"BP01-138"}),
        snapshot=entry_snapshot(include_keywords=True),
    )
    authored = root / "authored"
    file = authored / "rules/author-effects/BP01/000.yaml"
    file.parent.mkdir(parents=True)
    with fixture.template.copy() as db:
        source = CurrentAuthorSources(
            db,
            fixture.sources,
            fixture.snapshot,
            fixture.vocabulary,
            fixture.rules,
            fixture.jp_batch_id,
        )
        identity = db.select(
            "printing", ("card_id",), where={"region": "jp", "card_no": "BP01-138"}
        )[0].values["card_id"]
        assert isinstance(identity, str)
        complete = source._card_payload(identity)
        abilities: list[JsonValue] = []
        for obligation in source.keyword_obligations(identity):
            occurrence = obligation.source_use.occurrence
            entry: dict[str, JsonValue] = {
                "line": occurrence.line_ordinal + 1,
                "sources": [obligation.source_use.model_dump(mode="json")],
                **obligation.declaration_payload(),
            }
            if occurrence.ordinal is not None:
                entry["section"] = occurrence.ordinal
            abilities.append(entry)
        file.write_bytes(
            canonical(
                {
                    "format": 1,
                    "kind": "author_effect_set",
                    "scope": "shared",
                    "cards": {
                        "BP01-138": {
                            "meta": {
                                "dsl": "1.0",
                                "source": source_version(complete.payload),
                                "written_by": [
                                    {
                                        "actor": {"who": "test fixture", "model": None},
                                        "date": "2026-10-26",
                                    }
                                ],
                                "reviewed_by": [],
                                "status": "draft",
                                "verified_by_exam": [],
                                "rulings": [],
                            },
                            "abilities": abilities,
                        }
                    },
                }
            )
        )
        (authored / "rules/effects-index.yaml").write_bytes(
            canonical(
                {
                    "format": 1,
                    "kind": "effects_index",
                    "records": [
                        {
                            "card_id": identity,
                            "scope": "shared",
                            "file": "rules/author-effects/BP01/000.yaml",
                            "kind": "author_effect_set",
                            "record_key": "BP01-138",
                        }
                    ],
                }
            )
        )
        selection = source.selection_input(authored, AuthorExpander(REPOSITORY / "dsl"))
        author = object_value(selection)["authors"]
        assert isinstance(author, list)
        assert len(author) == 1
        assert object_value(author[0])["status"] == "accepted"
        return selection
