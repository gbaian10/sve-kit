"""Check counter operands against the engine's rule-name registry."""

from typing import TYPE_CHECKING

from sve_carddb.core.authored import read
from sve_carddb.core.json import array, object_value, parse, string
from sve_carddb.domains.effects.author_current import CardAccepted
from sve_carddb.domains.effects.author_inputs import AuthorInputError, Diagnostic

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.domains.effects.author_current import CardResult
    from sve_carddb.domains.effects.author_inputs import PathParts


def _counter_operands(
    node: dict[str, JsonValue], path: PathParts
) -> Iterator[tuple[str, PathParts]]:
    counter = node.get("counter")
    if isinstance(counter, dict) and isinstance(name := counter.get("name"), str):
        yield name, (*path, "counter", "name")
    if node.get("rule") == "counter_cap" and isinstance(name := node.get("name"), str):
        yield name, (*path, "name")
    if isinstance(counters := node.get("counters"), dict):
        for name in counters:
            yield name, (*path, "counters")
    for key in ("stack_plus", "earth_rite"):
        if key in node:
            yield "stack_counter", (*path, key)
    if isinstance(keywords := node.get("keywords"), list) and "stack" in keywords:
        yield "stack_counter", (*path, "keywords", keywords.index("stack"))


def counter_uses(
    node: JsonValue, path: PathParts = ()
) -> Iterator[tuple[str, PathParts]]:
    """Find counter operands in a validated, expanded author tree."""
    if isinstance(node, dict):
        yield from _counter_operands(node, path)
        for key, value in node.items():
            yield from counter_uses(value, (*path, key))
    elif isinstance(node, list):
        for index, value in enumerate(node):
            yield from counter_uses(value, (*path, index))


def _registered_names(authored: Path) -> set[str]:
    _, content = read(authored / "rules/keywords.yaml", root=authored)
    entries = object_value(object_value(parse(content))["keywords"])
    names: set[str] = set()
    for key, value in entries.items():
        entry = object_value(value)
        if entry.get("role") == "counter":
            names.update((key, string(entry["ja"])))
            names.update(string(alias) for alias in array(entry.get("aliases", [])))
    return names


def check_counter_registration(authored: Path, results: tuple[CardResult, ...]) -> None:
    """Reject an unresolved name before persisting or accepting author programs."""
    registered: set[str] | None = None
    for result in results:
        if not isinstance(result, CardAccepted):
            continue
        for name, path in counter_uses(result.tree):
            if registered is None:
                registered = _registered_names(authored)
            if name not in registered:
                raise AuthorInputError(
                    Diagnostic(
                        "dependency",
                        "unregistered_counter",
                        path,
                        card_id=result.record.card_id,
                        card_no=result.record.record_key,
                        scope=result.record.scope,
                    )
                )
