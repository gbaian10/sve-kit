"""Classify how an expanded ability uses the holdable keywords."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.domains.effects.author_inputs import PathParts

type Relation = Literal["has", "grants", "refers", "counts"]
type Use = tuple[str, Relation]

# Other registered keywords are expressed by their own semantic nodes (stack counters,
# limit-break conditions, Quick filters); their mentions have no adopted definition.
TRACKED = frozenset(
    {"assail", "aura", "bane", "drain", "intimidate", "rush", "storm", "ward"}
)
_COUNTING = frozenset(
    {
        "count",
        "count_of",
        "distinct_names",
        "distinct_orig_costs",
        "agg",
        "history_count",
        "history_sum",
    }
)
# Nodes that name a tracked keyword in their key instead of carrying its code.
_SEMANTIC: dict[str, Use] = {"ignore_ward": ("ward", "refers")}
_METADATA = frozenset({"line", "section", "sources", "imports", "ports"})


class UnclassifiedKeywordError(ValueError):
    """A tracked keyword appears where no relation rule covers it."""

    def __init__(self, path: PathParts) -> None:
        self.path = path
        super().__init__(
            "Unclassified keyword use at /" + "/".join(str(part) for part in path)
        )


@dataclass(frozen=True)
class _Context:
    aliases: frozenset[str]
    # Inside an ability granted to another object, "this" names that recipient.
    foreign: bool = False
    counting: bool = False
    # Recipient of an effect that omits "on"; None means the enclosing node does not say.
    implicit: Literal["self", "other"] | None = None


@dataclass
class _Found:
    uses: set[Use]
    handled: set[PathParts]

    def add(self, path: PathParts, code: JsonValue, relation: Relation) -> None:
        if isinstance(code, str) and code in TRACKED:
            self.uses.add((code, relation))
            self.handled.add(path)


def _self_aliases(node: JsonValue, found: set[str]) -> set[str]:
    """Collect result bindings of steps that move or change this card itself."""
    if isinstance(node, dict):
        if node.get("ref") == "this" and isinstance(name := node.get("result"), str):
            found.add(name)
        for value in node.values():
            _self_aliases(value, found)
    elif isinstance(node, list):
        for value in node:
            _self_aliases(value, found)
    return found


def _is_self(recipient: JsonValue, aliases: frozenset[str]) -> bool:
    if isinstance(recipient, dict) and recipient.keys() == {"follow"}:
        recipient = recipient["follow"]
    return isinstance(recipient, str) and (recipient == "this" or recipient in aliases)


def _grant(
    node: dict[str, JsonValue],
    path: PathParts,
    context: _Context,
    found: _Found,
    names: str,
) -> None:
    if "on" in node:
        to_self = _is_self(node["on"], context.aliases)
    elif context.implicit is None:
        raise UnclassifiedKeywordError(path)
    else:
        to_self = context.implicit == "self"
    own = to_self and not context.foreign
    branches: list[tuple[PathParts, JsonValue]] = [(path, node)]
    if isinstance(alternatives := node.get("instead"), list):
        branches.extend(
            ((*path, "instead", index), alternative)
            for index, alternative in enumerate(alternatives)
        )
    for branch_path, branch in branches:
        if not isinstance(branch, dict):
            continue
        key = names if branch is node else "keywords"
        if isinstance(listed := branch.get(key), list):
            for index, code in enumerate(listed):
                found.add((*branch_path, key, index), code, "has" if own else "grants")
        for name, value in branch.items():
            if name in {key, "instead"}:
                continue
            _walk(
                value,
                (*branch_path, name),
                replace(context, foreign=not own, implicit=None, counting=False)
                if name == "abilities"
                else context,
                found,
            )


def _enter(
    key: str, value: JsonValue, parent: dict[str, JsonValue], context: _Context
) -> _Context:
    if key in _COUNTING:
        return replace(context, counting=True)
    if key == "static" and isinstance(value, dict):
        zones = value.get("zone")
        equipped = "equipment" in (zones if isinstance(zones, list) else [zones])
        # An equipment ability usually affects its host, so an omitted recipient is unclear.
        return replace(context, implicit=None if equipped else "self")
    if key == "apply" and isinstance(value, dict):
        to_self = _is_self(value.get("on"), context.aliases)
        return replace(context, implicit="self" if to_self else "other")
    if key == "entry_effects":
        to_self = _is_self(parent.get("ref"), context.aliases)
        return replace(context, implicit="self" if to_self else "other")
    if key == "player_effect":
        return replace(context, implicit=None)
    return context


def _walk(node: JsonValue, path: PathParts, context: _Context, found: _Found) -> None:
    if isinstance(node, list):
        for index, value in enumerate(node):
            _walk(value, (*path, index), context, found)
        return
    if not isinstance(node, dict):
        return
    for key, value in node.items():
        here = (*path, key)
        if key == "grant" and isinstance(value, dict):
            _grant(value, here, context, found, "keywords")
        elif key == "keywords" and isinstance(value, dict):
            _grant(value, here, context, found, "list")
        elif key == "keyword":
            found.add(here, value, "counts" if context.counting else "refers")
        else:
            if key in _SEMANTIC:
                found.uses.add(_SEMANTIC[key])
            _walk(value, here, _enter(key, value, node, context), found)


def _unclassified(
    node: JsonValue, path: PathParts, handled: set[PathParts]
) -> PathParts | None:
    """Find a tracked code or key the rules above did not account for."""
    if isinstance(node, dict):
        children: list[tuple[PathParts, JsonValue]] = [
            ((*path, key), value) for key, value in node.items()
        ]
        for key in node:
            if TRACKED & set(key.split("_")) and key not in _SEMANTIC:
                return (*path, key)
    elif isinstance(node, list):
        children = [((*path, index), value) for index, value in enumerate(node)]
    else:
        return None
    for here, value in children:
        if isinstance(value, str) and value in TRACKED and here not in handled:
            return here
        if (missed := _unclassified(value, here, handled)) is not None:
            return missed
    return None


def ability_relations(ability: dict[str, JsonValue]) -> set[Use]:
    """Return every tracked keyword relation of one ability, or fail on an unknown use."""
    found = _Found(set(), set())
    context = _Context(frozenset(_self_aliases(ability, set())))
    head = {key: value for key, value in ability.items() if key not in _METADATA}
    if isinstance(declared := head.get("keywords"), list):
        for index, code in enumerate(declared):
            found.add(("keywords", index), code, "has")
    else:
        _walk(head, (), context, found)
    if (missed := _unclassified(head, (), found.handled)) is not None:
        raise UnclassifiedKeywordError(missed)
    return found.uses


def program_relations(program: dict[str, JsonValue]) -> set[Use]:
    """Union the relations of every face of one admitted whole-card program."""
    abilities: list[JsonValue] = []
    if isinstance(listed := program.get("abilities"), list):
        abilities.extend(listed)
    if isinstance(faces := program.get("faces"), dict):
        for face in faces.values():
            if isinstance(face, dict) and isinstance(
                own := face.get("abilities"), list
            ):
                abilities.extend(own)
    uses: set[Use] = set()
    for ability in abilities:
        if isinstance(ability, dict):
            uses |= ability_relations(ability)
    return uses
