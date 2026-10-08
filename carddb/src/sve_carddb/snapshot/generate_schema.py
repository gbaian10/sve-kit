"""Expand declarative columns into the public, self-contained JSON Schema."""

import json
from importlib.resources import files
from pathlib import Path

from pydantic import JsonValue

from sve_carddb.core.json import array, object_value, parse, string
from sve_carddb.snapshot.profiles import MEDIA, PROFILES, profile
from sve_carddb.snapshot.schema_patterns import patterns


def _ref(name: str) -> dict[str, JsonValue]:
    return {"$ref": "#/$defs/" + name}


def _patterns(value: JsonValue) -> None:
    if isinstance(value, dict):
        if isinstance(value.get("pattern"), dict):
            name = string(object_value(value["pattern"])["use"])
            value["pattern"] = patterns()[name]
        for item in value.values():
            _patterns(item)
    elif isinstance(value, list):
        for item in value:
            _patterns(item)


def _object(properties: dict[str, JsonValue]) -> dict[str, JsonValue]:
    return {
        "type": "object",
        "properties": properties,
        "required": list(properties),
        "additionalProperties": False,
    }


def _fields(name: str, tuples: dict[str, JsonValue]) -> dict[str, JsonValue]:
    result: dict[str, JsonValue] = {}
    for column, raw in object_value(object_value(tuples[name])["fields"]).items():
        field = object_value(raw)
        if "from" in field:
            # References point only to earlier logical tuples, never payload types.
            parent = object_value(tuples[string(field["from"])])
            field = object_value(object_value(parent["fields"])[column])
            if "from" in field:
                raise ValueError("Tuple field inheritance must have one level")
        result[column] = field
    return result


def _tuple(name: str, tuples: dict[str, JsonValue]) -> dict[str, JsonValue]:
    fields = _fields(name, tuples)
    result: dict[str, JsonValue] = {
        "type": "array",
        "prefixItems": [object_value(field)["schema"] for field in fields.values()],
        "items": False,
        "minItems": len(fields),
        "maxItems": len(fields),
        "x-columns": list(fields),
        "x-types": [object_value(field)["descriptor"] for field in fields.values()],
    }
    return result | object_value(object_value(tuples[name])["constraints"])


def _fragment(
    table: str, partition: str, name: str, row: dict[str, JsonValue]
) -> dict[str, JsonValue]:
    return _object(
        {
            "owner": _ref("Owner"),
            "bucket": _ref("UInt"),
            "partition": {"const": partition},
            "base": (
                _ref("Base")
                if table in {"printing", "face_revision"} and partition == "detail"
                else {"type": "null"}
            ),
            "columns": {"const": row["x-columns"]},
            "rows": {"type": "array", "items": _ref(name)},
        }
    )


def _changes(
    category: str, tables: list[str], tuples: dict[str, JsonValue]
) -> dict[str, JsonValue]:
    branches: list[JsonValue] = []
    for table in tables:
        fields = _fields(table, tuples)
        constraints = object_value(object_value(tuples[table])["constraints"])
        keys = array(constraints["x-primary-key"])
        changed_fields: dict[str, JsonValue] = {
            "type": "array",
            "items": {"enum": list(fields)},
        }
        changed_fields.update(
            {"minItems": 1, "uniqueItems": True}
            if category == "modified"
            else {"maxItems": 0}
        )
        branches.append(
            _object(
                {
                    "entity": {"const": table},
                    "key": _object(
                        {
                            string(key): object_value(fields[string(key)])["schema"]
                            for key in keys
                        }
                    ),
                    "changed_fields": changed_fields,
                    "reason": _ref("Text"),
                }
            )
        )
    return {"type": "array", "items": {"oneOf": branches}}


def generate(format_version: str = MEDIA) -> bytes:
    """Regenerate schema bytes solely from the packaged declarative source."""
    source = object_value(
        parse(
            files("sve_carddb.snapshot")
            .joinpath("schema/" + profile(format_version).resource + "/source.json")
            .read_bytes()
        )
    )
    _patterns(source)
    definitions = object_value(source["definitions"])
    tuples = object_value(source["tuples"])
    for name in tuples:
        definitions[name] = _tuple(name, tuples)
    types: dict[str, JsonValue] = {}
    for raw in array(source["nested"]):
        name = string(raw)
        row = object_value(definitions[name])
        types[name] = {"const": {"columns": row["x-columns"], "items": row["x-types"]}}
    definitions["Types"] = {
        "type": "object",
        "properties": types,
        "additionalProperties": False,
    }
    fragments = object_value(source["fragments"])
    for table, parts in fragments.items():
        for partition, raw in object_value(parts).items():
            name = string(raw)
            definitions[name + "_fragment"] = _fragment(
                table, partition, name, object_value(definitions[name])
            )
    change = object_value(definitions["Changes"])
    properties = object_value(change["properties"])
    change["properties"] = {
        key: (
            _changes(key, list(fragments), tuples)
            if key in {"added", "modified", "retired"}
            else properties[key]
        )
        for key in map(string, array(change["required"]))
    }
    envelope = object_value(source["envelope"])
    result = {key: value for key, value in envelope.items() if key != "oneOf"} | {
        "$defs": {
            string(name): definitions[string(name)] for name in array(source["order"])
        },
        "oneOf": envelope["oneOf"],
    }
    return (json.dumps(result, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def main() -> None:
    """Replace the checked-in resource when run in a source checkout."""
    for item in PROFILES:
        Path(__file__).parent.joinpath(
            "schema/" + item.resource + "/contract.schema.json"
        ).write_bytes(generate(item.version))


if __name__ == "__main__":
    main()
