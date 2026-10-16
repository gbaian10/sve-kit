"""Lightweight names shared by every pending candidate, without adopting wording."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import array, canonical, object_value, string

if TYPE_CHECKING:
    from collections.abc import Mapping

type Row = dict[str, JsonValue]

_SELECTION = ("field", "ordinal", "target_lang", "translation_id", "basis")


def common_name(wording: Row, revisions: Mapping[str, Row]) -> Row | None:
    """Keep only names and translation selections shared by all candidates."""
    if object_value(wording["display"])["revision_id"] is not None:
        return None
    candidates = [object_value(raw) for raw in array(wording["candidates"])]
    if not candidates or any(row["revision_id"] is None for row in candidates):
        return None
    selected = [revisions[string(row["revision_id"])] for row in candidates]
    if len({row["name_unit_id"] for row in selected}) != 1:
        return None
    shared = _name_selections(selected[0])
    for row in selected[1:]:
        current = _name_selections(row)
        shared = {key: value for key, value in shared.items() if key in current}
    translations = sorted(shared.values(), key=lambda row: string(row["target_lang"]))
    return {
        "name_unit_id": selected[0]["name_unit_id"],
        "translations": list(translations),
    }


def _name_selections(revision: Row) -> dict[bytes, Row]:
    # Source pointers name each candidate's own revision, so agreement ignores them.
    selections: dict[bytes, Row] = {}
    for raw in array(revision["translations"]):
        entry = object_value(raw)
        if entry["field"] == "name":
            selection = {key: entry[key] for key in _SELECTION}
            selections[canonical(selection)] = selection
    return selections
