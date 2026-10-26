"""A real printed declaration through C0 selection, generated for Rust at test time."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_current import CurrentAuthorSources
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_sources import SourceError, entry_payload

from .author_current_fixtures import (
    REPOSITORY,
    CurrentSourceFixture,
    current_source_fixture,
    entry_snapshot,
)
from .author_destroy_fixtures import destroy_fixture
from .current_author_storage import write_current_anchors

if TYPE_CHECKING:
    from pathlib import Path


COMBAT_CARDS = (
    "BP01-096",
    "BP01-158",
    "BP01-163",
    "BP01-P10",
    "BP03-033",
    "BP04-P07",
    "BP14-032",
    "DSD01b-004",
    "DSD01b-005",
    "SD07-006",
    "BP09-114",
)


def current_combat_keyword_inputs(root: Path) -> dict[str, JsonValue]:
    """Use real printings with current evidence, retaining every sibling and declaration."""
    fixture = current_source_fixture(
        root / "sources",
        card_numbers=frozenset(COMBAT_CARDS),
        snapshot=entry_snapshot(include_keywords=True),
    )
    inputs: dict[str, JsonValue] = {}
    failures: list[str] = []
    for number in COMBAT_CARDS:
        try:
            inputs[number] = current_keyword_inputs(
                root / number, number, fixture=fixture
            )
        except SourceError as error:
            failures.append(f"{number}: {error}")
    if failures:
        # Report every card at once; the message carries only numbers and reason codes.
        raise SourceError("; ".join(failures))
    return inputs


def current_keyword_inputs(
    root: Path,
    number: str = "BP01-138",
    *,
    fixture: CurrentSourceFixture | None = None,
    expander: AuthorExpander | None = None,
) -> JsonValue:
    """Rebuild a complete real declaration and all siblings from public current inputs."""
    fixture = fixture or (
        destroy_fixture(root / "sources")
        if number.startswith("BP19-")
        else current_source_fixture(
            root / "sources",
            card_numbers=frozenset({"BP01-138"}),
            snapshot=entry_snapshot(include_keywords=True),
        )
    )
    authored = root / "authored"
    author_path = f"rules/author-effects/{number.split('-', maxsplit=1)[0]}/000.yaml"
    file = authored / author_path
    file.parent.mkdir(parents=True)
    with fixture.template.copy() as db:
        source = CurrentAuthorSources(
            db,
            fixture.sources.stage(fixture.sources.build),
            fixture.snapshot,
            fixture.vocabulary,
            fixture.rules,
            fixture.jp_batch_id,
        )
        identity = db.select(
            "printing", ("card_id",), where={"region": "jp", "card_no": number}
        )[0].values["card_id"]
        assert isinstance(identity, str)
        complete = source.card_payload(identity)
        abilities: list[JsonValue] = []
        for obligation in source.keyword_obligations(identity):
            occurrence = obligation.source_use.occurrence
            entry: dict[str, JsonValue] = {
                "line": occurrence.line_ordinal + 1,
                **obligation.declaration_payload(),
            }
            if occurrence.ordinal is not None:
                entry["section"] = occurrence.ordinal
            abilities.append(entry)
        for (binding, scope), context in source.contexts(
            source.checked_sources(complete)
        ).items():
            use = SourceUse(
                binding_id=binding,
                occurrence=context.binding.occurrence_key(),
                ability_scope=scope,
            )
            body: dict[str, JsonValue] = {
                "line": use.occurrence.line_ordinal + 1,
                **entry_payload(context, scope),
            }
            if use.occurrence.ordinal is not None:
                body["section"] = use.occurrence.ordinal
            abilities.append(body)
        file.write_bytes(
            canonical(
                {
                    "format": 2,
                    "kind": "author_effect_set",
                    "scope": "shared",
                    "cards": {
                        number: {
                            "meta": {
                                "written_by": [
                                    {
                                        "actor": {"who": "test fixture", "model": None},
                                        "date": "2026-10-26",
                                    }
                                ],
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
                            "file": author_path,
                            "kind": "author_effect_set",
                            "record_key": number,
                        }
                    ],
                }
            )
        )
        expander = expander or AuthorExpander(REPOSITORY / "dsl")
        write_current_anchors(authored, source, expander, number)
        selection = source.selection_input(authored, expander)
        author = object_value(selection)["authors"]
        assert isinstance(author, list)
        assert len(author) == 1
        assert object_value(author[0])["status"] == "accepted"
        return selection
