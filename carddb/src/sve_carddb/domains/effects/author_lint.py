"""Check author forms whose timing changes the printed game effect."""

from dataclasses import dataclass
from itertools import pairwise
from typing import TYPE_CHECKING

from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains.effects.author_expansion import ability_nodes
from sve_carddb.domains.effects.author_inputs import Diagnostic

if TYPE_CHECKING:
    from collections.abc import Iterator

    from pydantic import JsonValue

    from sve_carddb.domains.effects.author_validation import PathParts

_REQUIRED_CHOOSE = "required_choose_use_targets"
_SPLIT_CREATE = "combine_consecutive_create"
_MESSAGES = {
    _REQUIRED_CHOOSE: "Declare an unconditional required selection in targets.",
    _SPLIT_CREATE: "Use one create with a token list for a single instruction.",
}
_GUARDED = frozenset(
    {"if", "optional", "pay", "choice", "either", "instead", "by_kind"}
)


@dataclass(frozen=True)
class Finding:
    rule: str
    path: PathParts


def _fixed_required(node: dict[str, JsonValue]) -> bool:
    count = node.get("n")
    return type(count) is int and count > 0 and not node.get("may")


def _tokens(step: JsonValue) -> frozenset[bytes]:
    if not isinstance(step, dict) or not isinstance(create := step.get("create"), dict):
        return frozenset()
    tokens = create["token"]
    return frozenset(
        canonical(t) for t in (tokens if isinstance(tokens, list) else [tokens])
    )


def findings(
    node: JsonValue, path: PathParts = (), *, guarded: bool = False
) -> Iterator[Finding]:
    """Walk expanded ability nodes, keeping each branch's step order and choice context."""
    if isinstance(node, list):
        for index, (first, second) in enumerate(pairwise(node)):
            left, right = _tokens(first), _tokens(second)
            if left and right and len(left | right) > 1:
                yield Finding(_SPLIT_CREATE, (*path, index, "create"))
        for index, value in enumerate(node):
            yield from findings(value, (*path, index), guarded=guarded)
    elif isinstance(node, dict):
        choose = node.get("choose")
        if not guarded and isinstance(choose, dict) and _fixed_required(choose):
            yield Finding(_REQUIRED_CHOOSE, (*path, "choose"))
        for key, value in node.items():
            in_branch = key in _GUARDED or (
                key == "repeat"
                and isinstance(value, dict)
                and bool(value.keys() & {"while", "until"})
            )
            # A delayed or granted ability has its own declaration timing.
            yield from findings(
                value,
                (*path, key),
                guarded=False
                if key in {"delayed", "abilities"}
                else guarded or in_branch,
            )


def lint(tree: JsonValue, exceptions: JsonValue) -> Diagnostic | None:
    """Reject the first finding without a matching source line, node and rule."""
    waivers = [object_value(value) for value in array(exceptions)]
    remaining = set(range(len(waivers)))
    for ability_path, ability in ability_nodes(tree):
        line = ability.get("line")
        for finding in findings(ability):
            path = "/" + "/".join(str(part) for part in finding.path)
            for index in sorted(remaining):
                waiver = waivers[index]
                if (
                    waiver["rule"] == finding.rule
                    and waiver["line"] == line
                    and waiver["path"] == path
                ):
                    remaining.remove(index)
                    break
            else:
                return Diagnostic(
                    "lint",
                    finding.rule,
                    (*ability_path, *finding.path),
                    expected=_MESSAGES[finding.rule],
                    card_line=line if type(line) is int else None,
                )
    if remaining:
        return Diagnostic(
            "lint", "unused_lint_exception", ("lint_exceptions", min(remaining))
        )
    return None
