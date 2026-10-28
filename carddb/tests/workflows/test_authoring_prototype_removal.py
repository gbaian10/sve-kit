"""Removing the last prototype deletes its shard after complete validation."""

from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_current import CardRejected
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_inputs import AuthorInputError, parse_document
from sve_carddb.domains.effects.author_set import load_author_set
from sve_carddb.workflows import authoring

from ..domains.effects.author_entry_cases import meta
from ..domains.effects.test_author_current_absent import CARD, synthetic_current

if TYPE_CHECKING:
    from pydantic import JsonValue

SCHEMAS = Path(__file__).resolve().parents[3] / "dsl"


@pytest.mark.parametrize(
    ("sibling", "alias", "alias_only", "cross_pack"),
    [
        (False, False, False, False),
        (True, False, False, False),
        (True, True, False, False),
        (True, True, True, False),
        (True, True, True, True),
    ],
)
def test_prototype_removal_deletes_only_exhausted_shards(
    tmp_path: Path, sibling: bool, alias: bool, alias_only: bool, cross_pack: bool
) -> None:
    # A cross-pack alias lives in another pack's prototype shard and index shard.
    alias_no = "OTH-02" if cross_pack else "SYN-02"
    old_pack = "OTH" if cross_pack else "SYN"
    repo = tmp_path / "repo"
    authored = repo / "authored"
    prototype_path = authored / f"rules/effects/{old_pack}.yaml"
    prototype_path.parent.mkdir(parents=True)
    prototype: dict[str, JsonValue] = {"status": "unsupported", "abilities": []}
    cards: dict[str, JsonValue] = {} if alias_only else {"SYN-01": prototype}
    if sibling:
        cards[alias_no] = prototype
    prototype_path.write_bytes(
        canonical({"format": 1, "kind": "effect_set", "cards": cards})
    )
    index = authored / f"rules/effects-index/{old_pack}.yaml"
    index.parent.mkdir(parents=True, exist_ok=True)
    index.write_bytes(
        canonical(
            {
                "format": 1,
                "kind": "effects_index",
                "records": [
                    {
                        "card_id": CARD,
                        "scope": "shared",
                        "file": f"rules/effects/{old_pack}.yaml",
                        "kind": "effect_set",
                        "record_key": alias_no if alias_only else "SYN-01",
                    }
                ],
            }
        )
    )
    untouched = index.with_name("OTHER.yaml")
    untouched_raw = b"# Preserve an unrelated pack exactly.\n" + canonical(
        {"format": 1, "kind": "effects_index", "records": []}
    )
    untouched.write_bytes(untouched_raw)
    hand = tmp_path / "main.yaml"
    hand.write_bytes(
        canonical(
            {
                "format": 2,
                "kind": "author_effect_set",
                "cards": {
                    "SYN-01": {
                        "meta": {"written_by": meta()["written_by"]},
                        "abilities": [],
                    }
                },
            }
        )
    )
    with synthetic_current(tmp_path / "source") as current:
        with current.db.transaction():
            sibling_id = CARD if alias else "c:" + "3" * 32
            if not alias:
                current.db.insert(
                    "card", dict(current.db.rows("card")[0].values) | {"id": sibling_id}
                )
            current.db.insert(
                "printing",
                dict(current.db.rows("printing")[0].values)
                | {
                    "id": "sibling",
                    "card_no": alias_no if sibling else "NEW-01",
                    "card_id": sibling_id,
                },
            )
        workspace = authoring.Workspace(
            authoring.Inputs(
                profile="public",
                repo=repo,
                jp_batch_id=current.jp_batch_id,
                revision="0" * 40,
            ),
            current,
            AuthorExpander(SCHEMAS),
            authored,
        )
        # An indexed prototype shard must select exactly its members.
        if sibling and not alias_only:
            index.write_bytes(
                canonical({"format": 1, "kind": "effects_index", "records": []})
            )
        report = authoring.write(
            workspace,
            main=hand,
            bodies=None,
            output_root=authored,
            cards=frozenset({"SYN-01"}),
            remove_prototype=True,
        )
        assert report["status"] == "ok"
        assert untouched.read_bytes() == untouched_raw
        assert prototype_path.exists() == (sibling and not alias)
        if sibling and not alias:
            assert object_value(parse_document(prototype_path.read_bytes()).value)[
                "cards"
            ] == {"SYN-02": prototype}
        inputs = load_author_set(authored, workspace.expander.schemas)
        assert len(inputs.index.records) == 1
        assert set(
            object_value(
                object_value(
                    parse_document(
                        (authored / "rules/author-effects/SYN/SYN-01.yaml").read_bytes()
                    ).value
                )["cards"]
            )
        ) == {"SYN-01"}
        if cross_pack:
            assert (
                object_value(
                    parse_document(
                        (authored / "rules/effects-index/OTH.yaml").read_bytes()
                    ).value
                )["records"]
                == []
            )
            assert not (authored / "rules/effects/OTH.yaml").exists()
        assert inputs.index.records[0].file == "rules/author-effects/SYN/SYN-01.yaml"
        assert all(
            file.path != f"rules/effects/{old_pack}.yaml"
            for file in inputs.prototype_files
        ) == (not sibling or alias)
        assert not any(
            isinstance(result, CardRejected)
            for result in current.validate_inputs(inputs, workspace.expander)
        )
        with pytest.raises(AuthorInputError, match="prototype_removal_set_mismatch"):
            authoring.write(
                workspace,
                main=hand,
                bodies=None,
                output_root=authored,
                cards=frozenset({"SYN-01"}),
                remove_prototype=True,
            )
