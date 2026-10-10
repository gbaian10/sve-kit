"""A mandatory selection reminder depends on its exact same-field body anchor."""

import re
from typing import TYPE_CHECKING
from unicodedata import normalize

from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.four_layer_units import count_context, source_unit

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_normalizer import (
        SourceField,
        SourcePart,
    )
    from sve_carddb.domains.translations.four_layer_units import CountContext

_REMINDER = re.compile(
    r"[（(](?P<count>[0-9０-９]+)枚を選べなければプレイできない[）)]"
)
_SELECTION = re.compile(
    r"(?:^|(?<=[。:：}】]))(?P<np>[^。:：]+?)(?P<count>N)枚(?:を)?選ぶ。"
)


def mandatory_selection_context(
    raw: str, part: SourcePart, value: int, field: SourceField | None
) -> CountContext | None:
    """Neither a matching note string nor a nearby unrelated ability proves its counted set."""
    if field is None or part not in field.parts or part.source_span.role != "reminder":
        return None
    if normalize_source(raw, field.source, reminders=field.reminders) != field:
        raise ValueError("Reminder field differs from exact normalized source")
    reminder = _REMINDER.fullmatch(part.canonical_source)
    anchor = part.source_span.anchor
    if (
        reminder is None
        or int(normalize("NFKC", reminder["count"])) != value
        or anchor is None
        or not 0 <= anchor < len(field.parts)
    ):
        return None
    body = field.parts[anchor]
    if body.source_span.role != "body" or body.line_ordinal != part.line_ordinal:
        return None
    selections = tuple(_SELECTION.finditer(body.canonical_source))
    if len(selections) != 1:
        return None
    selection = selections[0]
    spelling = "".join(
        raw[s.start : s.end] for s in body.units[selection.start("count")].origins
    )
    context = count_context(selection["np"])
    return (
        context
        if spelling.isdecimal()
        and int(normalize("NFKC", spelling)) == value
        and context is not None
        and source_unit(context, "枚").merge_allowed
        else None
    )
