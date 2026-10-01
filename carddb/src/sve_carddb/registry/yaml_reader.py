"""One-pass libyaml event reader with the registry's YAML 1.2 scalar rules."""

import re
from typing import TYPE_CHECKING

import yaml
from ruamel.yaml.resolver import VersionedResolver
from yaml.events import (
    AliasEvent,
    CollectionStartEvent,
    DocumentEndEvent,
    DocumentStartEvent,
    MappingEndEvent,
    MappingStartEvent,
    ScalarEvent,
    SequenceEndEvent,
    SequenceStartEvent,
    StreamEndEvent,
    StreamStartEvent,
)

try:
    from yaml.cyaml import CSafeLoader
except ImportError as exc:
    raise RuntimeError("Authored YAML requires PyYAML's libyaml C extension") from exc

if TYPE_CHECKING:
    from yaml.events import Event

_CORE_TAGS = {"tag:yaml.org,2002:" + name for name in ("bool", "int", "float", "null")}
# Share the old reader's exact 1.2 patterns, including its numeric spelling rules.
_RESOLVERS = {
    key: [
        (tag, re.compile(pattern.pattern, pattern.flags))
        for tag, pattern in rules
        if tag in _CORE_TAGS
    ]
    for key, rules in VersionedResolver(version=(1, 2)).versioned_resolver.items()
}


def _integer(value: str) -> int:
    value = value.replace("_", "")
    unsigned = value.lstrip("+-")
    if unsigned.startswith(("0b", "0o", "0x")):
        return int(value, 0)
    # YAML 1.2 leading zeroes are decimal, unlike PyYAML's 1.1 constructor.
    return int(value, 10)


def _scalar(event: ScalarEvent) -> object:
    if not event.implicit[0]:
        return event.value
    value = event.value
    for tag, pattern in _RESOLVERS.get(value[:1], ()):
        if not pattern.match(value):
            continue
        if tag.endswith(":null"):
            return None
        if tag.endswith(":bool"):
            return value.lower() == "true"
        if tag.endswith(":int"):
            return _integer(value)
        return _float(value)
    return value


def _float(value: str) -> float:
    cleaned = value.replace("_", "").lower()
    if cleaned.endswith(".inf"):
        return float(cleaned.replace(".", ""))
    if cleaned == ".nan":
        return float("nan")
    return float(cleaned)


def _node(loader: CSafeLoader, event: Event | None) -> object:
    if isinstance(event, AliasEvent):
        raise TypeError("Aliases are forbidden")
    if (
        isinstance(event, (ScalarEvent, CollectionStartEvent))
        and event.anchor is not None
    ):
        raise TypeError("Anchors are forbidden")
    if isinstance(event, (ScalarEvent, CollectionStartEvent)) and event.tag is not None:
        raise TypeError("Explicit tags are forbidden")
    if isinstance(event, ScalarEvent):
        return _scalar(event)
    if isinstance(event, SequenceStartEvent):
        items = []
        while not isinstance(child := loader.get_event(), SequenceEndEvent):
            items.append(_node(loader, child))
        return items
    if isinstance(event, MappingStartEvent):
        return _mapping(loader)
    raise ValueError("Expected a YAML value")


def _mapping(loader: CSafeLoader) -> dict[str, object]:
    result: dict[str, object] = {}
    while not isinstance(event := loader.get_event(), MappingEndEvent):
        key = _node(loader, event)
        if not isinstance(key, str):
            raise TypeError("YAML mapping keys must be strings")
        if key == "<<":
            raise ValueError("Merge keys are forbidden")
        if key in result:
            raise ValueError("Duplicate YAML mapping key")
        result[key] = _node(loader, loader.get_event())
    return result


def parse_yaml(data: bytes) -> object:
    """Parse a single UTF-8 document, rejecting forbidden syntax as events arrive."""
    if not yaml.__with_libyaml__:
        raise RuntimeError("Authored YAML requires PyYAML's libyaml C extension")
    loader = CSafeLoader(data.decode("utf-8"))
    try:
        if not isinstance(loader.get_event(), StreamStartEvent):
            raise TypeError("Expected a YAML stream")
        document = loader.get_event()
        if isinstance(document, StreamEndEvent):
            return None
        if not isinstance(document, DocumentStartEvent):
            raise TypeError("Expected a YAML document")
        if document.version is not None and document.version != (1, 2):
            raise ValueError("Only YAML 1.2 is supported")
        value = _node(loader, loader.get_event())
        if not isinstance(loader.get_event(), DocumentEndEvent) or not isinstance(
            loader.get_event(), StreamEndEvent
        ):
            raise TypeError("Only one YAML document is supported")
    except yaml.YAMLError as exc:
        raise ValueError("Invalid authored YAML syntax") from exc
    else:
        return value
    finally:
        loader.dispose()
