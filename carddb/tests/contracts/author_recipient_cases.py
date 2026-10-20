"""Recipient examples retain the source scope of the official Japanese cards."""

from typing import TYPE_CHECKING

from pydantic import TypeAdapter

from sve_carddb.contracts import author_recipients as author

from .author_selector_cases import CONFIG

if TYPE_CHECKING:
    from pydantic import JsonValue

RECIPIENTS: dict[str, TypeAdapter[object]] = {
    "A.sel.leader-or": TypeAdapter(author.LeaderOr, config=CONFIG),
    "A.sel.recipient-atom": TypeAdapter(author.RecipientAtom, config=CONFIG),
    "A.sel.recipient-union": TypeAdapter(author.RecipientUnion, config=CONFIG),
    "A.sel.recipient": TypeAdapter(author.RecipientSel, config=CONFIG),
}
RECIPIENT_NAMES = {
    "A.sel.leader-or": "LeaderOr",
    "A.sel.recipient-atom": "RecipientAtom",
    "A.sel.recipient-union": "RecipientUnion",
    "A.sel.recipient": "RecipientSel",
}
RECIPIENT = "A.sel.recipient"
OBJECT_UNION = "A.sel.object-union"
RECIPIENT_UNION = "A.sel.recipient-union"
LEADER_OR: dict[str, JsonValue] = {
    "who": "opp",
    "kind": "follower",
    "leader_or": True,
}
LEADER_UNION: dict[str, JsonValue] = {
    "any_of": [
        {"leaders": "opponents"},
        {"who": "opponents", "zone": "field", "kind": "follower"},
    ]
}
CARD_QUERIES: list[tuple[str, dict[str, JsonValue]]] = [
    ("BP01-125", LEADER_OR),
    ("BP02-062", LEADER_OR),
    ("BP05-057", {**LEADER_OR, "zone": "field"}),
    ("BP02-052", LEADER_UNION),
    ("BP03-076", LEADER_UNION),
    ("BP04-012", LEADER_UNION),
    (
        "BP03-093-fanfare",
        {
            "any_of": [
                {"leader": "self"},
                {"who": "self", "zone": "field", "kind": "follower", "other": True},
            ]
        },
    ),
    (
        "BP21-037-token",
        {
            "any_of": [
                {"leaders": "opponents"},
                {"who": "opponents", "zone": "field", "kind": "follower"},
                {
                    "who": "self",
                    "zone": "field",
                    "name": "御伽の姫・アマリリス",
                },
            ]
        },
    ),
    (
        "BP07-050",
        {
            "any_of": [
                {"who": {"master_of": "t"}, "zone": "hand", "kind": "follower"},
                {"who": {"master_of": "t"}, "zone": "hand", "kind": "amulet"},
            ]
        },
    ),
    (
        "BP07-099",
        {
            "any_of": [
                {
                    "in": "looked",
                    "kind": "follower",
                    "trait": "童話",
                    "orig_cost_le": 5,
                },
                {"in": "looked", "kind": "amulet", "trait": "童話", "orig_cost_le": 5},
            ]
        },
    ),
    (
        "CP01-080",
        {
            "any_of": [
                {"in": "shown", "kind": "follower", "trait": "ウマ娘"},
                {"in": "shown", "kind": "amulet", "trait": "ウマ娘"},
            ]
        },
    ),
]
