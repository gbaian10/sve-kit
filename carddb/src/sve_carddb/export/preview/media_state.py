"""Image version state: one high-water revision and the last export's groups."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, integer, object_value, parse

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.export.preview import Roots


MAX_SAFE = 9007199254740991
STATE = "media-state.json"


def _read(roots: Roots) -> tuple[int, JsonValue]:
    path = roots.destination(STATE, private=True)
    if not path.exists():
        return 0, None
    value = object_value(parse(path.read_bytes()))
    if set(value) != {"high_water", "committed"}:
        raise ValueError("Invalid preview media state")
    high_water = integer(value["high_water"])
    committed = value["committed"]
    if not 0 < high_water <= MAX_SAFE or (
        committed is not None
        and integer(object_value(committed)["revision"]) > high_water
    ):
        raise ValueError("Invalid preview media state")
    return high_water, committed


def _save(roots: Roots, high_water: int, committed: JsonValue) -> None:
    target = roots.destination(STATE, private=True)
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_name("." + STATE + ".pending")
    temporary.write_bytes(canonical({"high_water": high_water, "committed": committed}))
    temporary.replace(target)


def reserve(roots: Roots) -> tuple[int, JsonValue]:
    """Burn the next revision before export so a failed export never reuses it."""
    high_water, committed = _read(roots)
    if high_water >= MAX_SAFE:
        raise ValueError("Preview media revision domain exhausted")
    _save(roots, high_water + 1, committed)
    return high_water + 1, committed


def commit(roots: Roots, state: JsonValue) -> None:
    """Make this export the comparison basis before its pointer can expose tokens."""
    high_water, _ = _read(roots)
    _save(roots, max(high_water, integer(object_value(state)["revision"])), state)
