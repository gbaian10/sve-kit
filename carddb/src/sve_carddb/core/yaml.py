"""Strict single-document YAML and bounded authored input constraints."""

import yamlrocks
from pydantic import JsonValue, TypeAdapter

MAX_BYTES = 1_048_576
JSON_VALUE: TypeAdapter[JsonValue] = TypeAdapter(JsonValue)

_OPTIONS = (
    yamlrocks.OPT_DUPLICATE_KEYS_ERROR
    | yamlrocks.OPT_REJECT_COMPLEX_KEYS
    | yamlrocks.OPT_PASSTHROUGH_TAG
)


def parse_yaml(data: bytes) -> object:
    """Decode UTF-8; leave value types to the caller's strict JSON boundary."""
    text = data.decode("utf-8")
    try:
        if yamlrocks.yaml_version(text) not in {None, "1.2"}:
            raise ValueError("Only YAML 1.2 is supported")
        documents = yamlrocks.loads_all(text, option=_OPTIONS)
    except yamlrocks.YAMLRocksDecodeError:
        # Parser diagnostics can include source text; expose only a safe category.
        raise ValueError("Invalid authored YAML syntax") from None
    if len(documents) > 1:
        raise ValueError("Only one YAML document is supported")
    result: object = documents[0] if documents else None
    return result
