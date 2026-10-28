"""Current-reader output for Rust's identity handoff tests, never a stored card table."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_current import (
    CardAccepted,
    CardRejected,
    CurrentAuthorSources,
)
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_inputs import AuthorInputError, parse_document

from .author_current_fixtures import REPOSITORY, current_source_fixture
from .current_keyword_fixtures import (
    current_combat_keyword_inputs,
    current_keyword_inputs,
)

if TYPE_CHECKING:
    from pathlib import Path


def current_lowering_inputs(root: Path) -> JsonValue:
    """Exercise accepted and provisional JP printings through the same current reader."""
    fixture = current_source_fixture(root / "sources")
    number = "BP01-009"
    card_id = "c:9c5459446e0254818099800b4208cd96"
    authored = root / "authored"
    file = authored / "rules/author-effects/BP01/000.yaml"
    with fixture.template.copy() as db:
        current = CurrentAuthorSources(
            db,
            fixture.sources,
            fixture.snapshot,
            fixture.vocabulary,
            fixture.rules,
            fixture.jp_batch_id,
        )
        expander = AuthorExpander(REPOSITORY / "dsl")
        prototype = authored / "rules/effects/BP01.yaml"
        prototype.parent.mkdir(parents=True)
        prototype.write_bytes(
            canonical(
                {
                    "format": 1,
                    "kind": "effect_set",
                    "cards": {
                        number: {
                            "status": "complete",
                            "abilities": [
                                {
                                    "line": 1,
                                    "kind": "evolve",
                                    "costs": [{"op": "pp", "amount": 1}],
                                    "body": {"op": "evolve", "subjects": "self"},
                                }
                            ],
                        }
                    },
                }
            )
        )
        switch: dict[str, JsonValue] = {
            "before": current.selection_input(authored, expander)
        }
        file.parent.mkdir(parents=True)
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
                                        "date": "2026-10-23",
                                    }
                                ],
                            },
                            "abilities": [
                                {
                                    "line": 1,
                                    "evolve": {"cost": {"pp": 1}},
                                }
                            ],
                        }
                    },
                }
            )
        )
        (authored / "rules/effects-index").mkdir(exist_ok=True)
        (authored / "rules/effects-index/BP01.yaml").write_bytes(
            canonical(
                {
                    "format": 1,
                    "kind": "effects_index",
                    "records": [
                        {
                            "card_id": card_id,
                            "scope": "shared",
                            "file": "rules/author-effects/BP01/000.yaml",
                            "kind": "author_effect_set",
                            "record_key": number,
                        }
                    ],
                }
            )
        )
        try:
            current.selection_input(authored, expander)
        except AuthorInputError as error:
            switch["overlap"] = error.diagnostic.reason
        else:
            raise AssertionError("synthetic prototype overlap was accepted")
        prototype.unlink()
        accepted = current.validate(authored, expander)[0]
        assert isinstance(accepted, CardAccepted)
        switch["after"] = current.selection_input(authored, expander)
        switch["missing"] = missing_selection(file, authored, current, expander)
        with db.transaction():
            for printing in db.select(
                "printing", ("id",), where={"region": "jp", "card_no": number}
            ):
                db.update(
                    "printing",
                    {"id": printing.values["id"]},
                    {"card_no_state": "provisional"},
                )
        rejected = current.validate(authored, expander)[0]
        assert isinstance(rejected, CardRejected)
        return {
            "accepted": accepted.lowering_input(),
            "keywords": current_keyword_inputs(root / "keywords"),
            "combat_keywords": current_combat_keyword_inputs(root / "combat-keywords"),
            "keyword_fanfare": {
                number: current_keyword_inputs(root / number, number)
                for number in ("BP19-P32", "BP19-114")
            },
            "rejected": {"reason": rejected.diagnostic.reason},
            "switch": switch,
        }


def missing_selection(
    file: Path, authored: Path, current: CurrentAuthorSources, expander: AuthorExpander
) -> JsonValue:
    original = file.read_bytes()
    tree = object_value(parse_document(original).value)
    object_value(object_value(tree["cards"])["BP01-009"])["abilities"] = []
    file.write_bytes(canonical(tree))
    try:
        return current.selection_input(authored, expander)
    finally:
        file.write_bytes(original)
