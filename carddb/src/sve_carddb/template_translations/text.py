"""Translation-contract §4.2 finite placeholders; no expressions or implicit N/X replacement."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.template_parameters.models import Schema

NAME = re.compile(r"[a-z][a-z0-9_]*\Z")


@dataclass(frozen=True)
class Literal:
    text: str


@dataclass(frozen=True)
class Parameter:
    name: str


def escape(text: str) -> str:
    """Source punctuation stays literal rather than becoming a template instruction."""
    return "".join("\\" + char if char in "\\{}" else char for char in text)


def parse(text: str, schema: Schema) -> tuple[Literal | Parameter, ...]:  # ruff: ignore[complex-structure] -- each delimiter guard is an independent finite-language refusal
    """Require every declared slot and refuse unescaped/unknown brace syntax."""
    declared = {slot.name for slot in schema.slots}
    used = set()
    parts: list[Literal | Parameter] = []
    literal: list[str] = []
    position = 0
    while position < len(text):
        char = text[position]
        if char == "\\":
            if position + 1 == len(text) or text[position + 1] not in "\\{}":
                raise ValueError(
                    "Template literal escape must precede a brace or backslash"
                )
            literal.append(text[position + 1])
            position += 2
        elif text.startswith("{{", position):
            end = text.find("}}", position + 2)
            if end < 0:
                raise ValueError("Template parameter must have closing double braces")
            name = text[position + 2 : end]
            if NAME.fullmatch(name) is None:
                raise ValueError("Template parameter must be a plain ASCII slot name")
            if name not in declared:
                raise ValueError("Template parameter is not declared by its schema")
            if literal:
                parts.append(Literal("".join(literal)))
                literal.clear()
            parts.append(Parameter(name))
            used.add(name)
            position = end + 2
        elif char in "{}":
            raise ValueError("Template literal braces must be escaped")
        else:
            literal.append(char)
            position += 1
    if literal:
        parts.append(Literal("".join(literal)))
    if used != declared:
        raise ValueError("Template text must use every declared parameter")
    return tuple(parts)
