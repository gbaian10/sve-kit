"""Role examples from author syntax §14 A-016h and the selection contract."""

from typing import TYPE_CHECKING

from pydantic import ConfigDict, TypeAdapter

from sve_carddb.contracts import author_selections as author

if TYPE_CHECKING:
    from pydantic import JsonValue

CONFIG = ConfigDict(regex_engine="python-re")
SELECTIONS: dict[str, TypeAdapter[object]] = {
    "A.target.role-one": TypeAdapter(author.TargetRoleOne),
    "A.target.role-value": TypeAdapter(author.TargetRoleValue, config=CONFIG),
    "A.target.roles": TypeAdapter(author.TargetRoles, config=CONFIG),
}
ROLES: list[tuple[str, JsonValue]] = [
    ("A.target.role-one", {"if_nonempty": 1}),
    ("A.target.role-value", {"if_nonempty": 1}),
    ("A.target.role-value", "remaining"),
    ("A.target.roles", {"primary": {"if_nonempty": 1}, "rest": "remaining"}),
    ("A.target.roles", {"named_role": {"if_nonempty": 1}}),
    ("A.target.roles", {"item": "remaining"}),
]
