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


def _is_self(recipient: JsonValue, aliases: frozenset[str] | set[str]) -> bool:
    if isinstance(recipient, dict) and recipient.keys() == {"follow"}:
        recipient = recipient["follow"]
    return isinstance(recipient, str) and (recipient == "this" or recipient in aliases)


def _collect_aliases(node: JsonValue, found: set[str], owner: str | None) -> None:
    if isinstance(node, dict):
        # A transformed card is another object, even when this card was transformed.
        if (
            owner != "transform"
            and _is_self(node.get("ref"), found)
            and isinstance(name := node.get("result"), str)
        ):
            found.add(name)
        for key, value in node.items():
            # A granted ability binds its own names; they never reach the outer steps.
            if key != "abilities":
                _collect_aliases(value, found, key)
    elif isinstance(node, list):
        for value in node:
            _collect_aliases(value, found, owner)


def _self_aliases(ability: JsonValue) -> frozenset[str]:
    """Collect result bindings of steps that move or change this card itself."""
    found: set[str] = set()
    size = -1
    # A later step may act on an earlier result, in any authored order.
    while size != len(found):
        size = len(found)
        _collect_aliases(ability, found, None)
    return frozenset(found)


def _mentions_tracked(node: JsonValue) -> bool:
    if isinstance(node, dict):
        return any(
            key in _SEMANTIC or _mentions_tracked(value) for key, value in node.items()
        )
    if isinstance(node, list):
        return any(_mentions_tracked(value) for value in node)
    return isinstance(node, str) and node in TRACKED


def _receives_itself(
    node: dict[str, JsonValue], path: PathParts, context: _Context
) -> bool:
    if "on" in node:
        return _is_self(node["on"], context.aliases)
    if context.implicit is None:
        # Without a known recipient only a grant that names no tracked keyword is harmless.
        if _mentions_tracked(node):
            raise UnclassifiedKeywordError(path)
        return False
    return context.implicit == "self"


def _grant_branch(
    branch: dict[str, JsonValue],
    path: PathParts,
    context: _Context,
    found: _Found,
    *,
    names: str,
    own: bool,
) -> None:
    if isinstance(listed := branch.get(names), list):
        for index, code in enumerate(listed):
            found.add((*path, names, index), code, "has" if own else "grants")
    for name, value in branch.items():
        if name in {names, "instead"}:
            continue
        if name == "abilities" and isinstance(value, list):
            for index, granted in enumerate(value):
                inner = _Context(_self_aliases(granted), foreign=not own)
                _walk(granted, (*path, name, index), inner, found)
        else:
            _walk(value, (*path, name), context, found)


def _grant(
    node: dict[str, JsonValue],
    path: PathParts,
    context: _Context,
    found: _Found,
    names: str,
) -> None:
    own = _receives_itself(node, path, context) and not context.foreign
    _grant_branch(node, path, context, found, names=names, own=own)
    if isinstance(alternatives := node.get("instead"), list):
        for index, alternative in enumerate(alternatives):
            if isinstance(alternative, dict):
                _grant_branch(
                    alternative,
                    (*path, "instead", index),
                    context,
                    found,
                    names="keywords",
                    own=own,
                )


def _enter(
    key: str,
    value: JsonValue,
    parent: dict[str, JsonValue],
    owner: str | int | None,
    context: _Context,
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
        # Only moving this card keeps it the same object; created, found or transformed cards are others.
        to_self = owner == "move" and _is_self(parent.get("ref"), context.aliases)
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
            owner = path[-1] if path else None
            _walk(value, here, _enter(key, value, node, owner, context), found)


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
    context = _Context(_self_aliases(ability))
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
