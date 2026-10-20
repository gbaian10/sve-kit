"""Recipient selections preserve object and leader membership as distinct types."""

from typing import Annotated, Literal, NotRequired, TypedDict

from pydantic import AfterValidator, ConfigDict, Field, with_config

from sve_carddb.contracts.author_object_filters import TrueFlag  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_references import LeaderRecipient, PlayerRef
from sve_carddb.contracts.author_selectors import ObjectAtom, ObjectSel

CONFIG = ConfigDict(extra="forbid", strict=True, regex_engine="python-re")


@with_config(CONFIG)
class LeaderOrFields(TypedDict):
    who: PlayerRef
    kind: Literal["follower"]
    leader_or: TrueFlag
    zone: NotRequired[Literal["field"]]


type LeaderOr = LeaderOrFields
type RecipientAtom = ObjectAtom | LeaderRecipient | LeaderOr


@with_config(CONFIG)
class RecipientUnionFields(TypedDict):
    any_of: Annotated[
        tuple[RecipientAtom, ...],
        Field(
            min_length=1,
            json_schema_extra={
                "contains": {
                    "anyOf": [
                        {"required": ["leader"]},
                        {"required": ["leaders"]},
                        {"required": ["leader_of"]},
                        {"required": ["leader_or"]},
                    ]
                }
            },
        ),
    ]


def _require_leader(value: RecipientUnionFields) -> RecipientUnionFields:
    # Validated object atoms are dicts; the existing leader atoms are models.
    if not any(
        not isinstance(limb, dict) or limb.get("leader_or") is True
        for limb in value["any_of"]
    ):
        raise ValueError("A recipient union requires a leader limb")
    return value


type RecipientUnion = Annotated[RecipientUnionFields, AfterValidator(_require_leader)]
type RecipientSel = ObjectSel | LeaderRecipient | LeaderOr | RecipientUnion
