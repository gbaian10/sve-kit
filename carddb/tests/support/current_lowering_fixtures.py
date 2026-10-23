"""Current-reader output for Rust's identity handoff tests, never a stored card table."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_current import (
    CardAccepted,
    CardRejected,
    CurrentAuthorSources,
)
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_source_hash import source_version

from .author_current_fixtures import REPOSITORY, current_source_fixture

if TYPE_CHECKING:
    from pathlib import Path


def current_lowering_inputs(root: Path) -> JsonValue:
    """Exercise accepted and provisional JP printings through the same current reader."""
    fixture = current_source_fixture(root / "sources")
    number = "BP01-009"
    card_id = "c:9c5459446e0254818099800b4208cd96"
    authored = root / "authored"
    file = authored / "rules/author-effects/BP01/000.yaml"
    file.parent.mkdir(parents=True)
    with fixture.template.copy() as db:
        current = CurrentAuthorSources(
            db,
            fixture.sources,
            fixture.snapshot,
            fixture.vocabulary,
            fixture.rules,
            fixture.jp_batch_id,
        )
        card = current._card_payload(card_id)
        matched = card.fields[0].matches[0]
        assert matched is not None
        use = SourceUse(
            binding_id=matched.binding.id,
            occurrence=matched.binding.occurrence_key(),
            ability_scope="ability_0",
        )
        file.write_bytes(
            canonical(
                {
                    "format": 1,
                    "kind": "author_effect_set",
                    "scope": "shared",
                    "cards": {
                        number: {
                            "meta": {
                                "dsl": "1.0",
                                "source": source_version(card.payload),
                                "written_by": [
                                    {
                                        "actor": {"who": "test fixture", "model": None},
                                        "date": "2026-10-23",
                                    }
                                ],
                                "reviewed_by": [],
                                "status": "draft",
                                "verified_by_exam": [],
                                "rulings": [],
                            },
                            "abilities": [
                                {
                                    "line": 1,
                                    "sources": [use.model_dump(mode="json")],
                                    "ports": {"imports": {}, "exports": {}},
                                    "evolve": {"cost": {"pp": 1}},
                                }
                            ],
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
        expander = AuthorExpander(REPOSITORY / "dsl")
        accepted = current.validate(authored, expander)[0]
        assert isinstance(accepted, CardAccepted)
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
            "rejected": {"reason": rejected.diagnostic.reason},
        }
