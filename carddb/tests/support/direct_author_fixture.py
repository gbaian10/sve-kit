"""A pending sentence shared by named bodies reaches the real current selection."""

from pathlib import Path

from pydantic import JsonValue

from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.workflows import authoring

from ..domains.text_observations.test_effect_presence import page
from .direct_current_fixture import synthetic_current


def direct_author_input(
    root: Path,
    *,
    aliases: bool = False,
    steps: list[JsonValue] | None = None,
    lint_exceptions: list[JsonValue] | None = None,
) -> JsonValue:
    """Exercise writer, closure, named substitution and current handoff without anchors."""
    authored = root / "authored"
    (authored / "rules").mkdir(parents=True)
    main = root / "main.yaml"
    main.write_bytes(
        canonical(
            {
                "format": 2,
                "kind": "author_effect_set",
                "cards": {
                    "SYN-01": {
                        "admission": "light",
                        "abilities": [{"line": 1, "use": "draw", "params": {"n": 1}}],
                        **(
                            {"lint_exceptions": lint_exceptions}
                            if lint_exceptions is not None
                            else {}
                        ),
                    }
                },
            }
        )
    )
    bodies = root / "body.yaml"
    bodies.write_bytes(
        canonical(
            {
                "format": 2,
                "kind": "effect_body_shard",
                "records": [
                    {
                        "id": "draw",
                        "admission": "shared",
                        "params": {"n": "Nat"},
                        "written_by": [
                            {
                                "actor": {"who": "synthetic test", "model": None},
                                "date": "2026-10-27",
                            }
                        ],
                        "body": {
                            "spell": {
                                "do": steps
                                if steps is not None
                                else [{"draw": {"who": "self", "n": {"param": "n"}}}]
                            }
                        },
                    }
                ],
            }
        )
    )
    repo = Path(__file__).resolve().parents[3]
    with synthetic_current(
        root / "source",
        page("jp", '<div class="detail">A sentence outside the classifier.</div>'),
        kind="spell",
    ) as current:
        if aliases:
            printing = dict(current.db.rows("printing")[0].values)
            with current.db.transaction():
                for number, state in (
                    ("SYN-02", "official"),
                    ("SYN-03", "provisional"),
                ):
                    current.db.insert(
                        "printing",
                        printing
                        | {"id": number, "card_no": number, "card_no_state": state},
                    )
        workspace = authoring.Workspace(
            authoring.Inputs(
                profile="public", repo=repo, jp_batch_id="sha256:" + "0" * 64
            ),
            current,
            AuthorExpander(repo / "dsl"),
            authored,
        )
        authoring.write(
            workspace, main=main, bodies=bodies, output_root=authored, all_cards=True
        )
        return authoring.check(workspace)["selection"]
