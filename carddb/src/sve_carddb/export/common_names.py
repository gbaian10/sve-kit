"""Lightweight names shared by every pending candidate, without adopting wording."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import array, canonical, object_value, string

if TYPE_CHECKING:
    from collections.abc import Mapping

type Row = dict[str, JsonValue]


def common_name(wording: Row, revisions: Mapping[str, Row]) -> Row | None:
    """Keep only names and translation selections shared by all candidates."""
    if object_value(wording["display"])["revision_id"] is not None:
        return None
    candidates = [object_value(raw) for raw in array(wording["candidates"])]
    if not candidates or any(row["revision_id"] is None for row in candidates):
        return None
    selected = [revisions[string(row["revision_id"])] for row in candidates]
    names = {row["name_unit_id"] for row in selected}
    if len(names) != 1:
        return None
    translations: dict[bytes, Row] | None = None
    for row in selected:
        choices = {
            canonical(value): value
            for raw in array(row["translations"])
            for entry in (object_value(raw),)
            if entry["field"] == "name"
            for value in (
                {
                    key: entry[key]
                    for key in (
                        "field",
                        "ordinal",
                        "target_lang",
                        "translation_id",
                        "basis",
                    )
                },
            )
        }
        translations = (
            choices
            if translations is None
            else {key: value for key, value in translations.items() if key in choices}
        )
    common_translations = sorted(
        (translations or {}).values(), key=lambda row: string(row["target_lang"])
    )
    return {
        "name_unit_id": selected[0]["name_unit_id"],
        "translations": list(common_translations),
    }
