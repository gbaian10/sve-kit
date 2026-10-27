"""Serialize synthetic expanded frames into the current shared authored storage."""

from pydantic import JsonValue

from sve_carddb.contracts.four_layer import hash_payload
from sve_carddb.core.json import canonical, object_value, parse
from sve_carddb.domains.translations.four_layer_definitions import (
    AREA,
    INDEX,
    DefinitionIndex,
    DefinitionShard,
    Entry,
    Files,
    from_files,
)


def _sources(document: dict[str, JsonValue]) -> list[dict[str, JsonValue]]:
    result: list[dict[str, JsonValue]] = []
    records = document.get("records")
    if isinstance(records, list):
        for record in records:
            if (
                not isinstance(record, dict)
                or record.get("kind") != "sentence_template"
            ):
                continue
            data = record.get("data")
            source = data.get("source") if isinstance(data, dict) else None
            if isinstance(source, dict) and isinstance(
                source.get("semantic_definitions"), dict
            ):
                result.append(source)
    return result


def with_definitions(files: Files) -> Files:
    documents = []
    sources = []
    for path, raw, content in files:
        if path.startswith(AREA):
            continue
        try:
            document = object_value(parse(content))
        except ValueError:
            return files
        sources.extend(_sources(document))
        documents.append((path, raw, content, document))
    if not sources:
        return files
    entries = list(from_files(files).entries)
    handles = {entry.hash: entry.id for entry in entries}
    references = []
    for source in sources:
        value = source["semantic_definitions"]
        checksum = hash_payload(value)
        if checksum not in handles:
            identifier = max(handles.values(), default=0) + 1
            handles[checksum] = identifier
            entries.append(
                Entry(id=identifier, hash=checksum, value=escape_literals(value))
            )
        references.append(handles[checksum])
    for source, reference in zip(sources, references, strict=True):
        source["semantic_definitions"] = reference
    result = []
    for path, raw, content, document in documents:
        encoded = canonical(document)
        result.append((path, raw if encoded == content else encoded, encoded))
    index = DefinitionIndex(
        format=1, kind="frame_semantics_index", includes=("001.yaml",)
    )
    shard = DefinitionShard(
        format=1, kind="frame_semantics_shard", definitions=tuple(entries)
    )
    for path, model in ((INDEX, index), (AREA + "001.yaml", shard)):
        encoded = canonical(model.model_dump(mode="json"))
        result.append((path, encoded, encoded))
    return tuple(result)


def escape_literals(value: JsonValue) -> JsonValue:
    if isinstance(value, dict):
        return {
            ("@" + key if key.startswith("@") else key): escape_literals(item)
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [escape_literals(item) for item in value]
    if isinstance(value, str) and value.startswith("@"):
        return "@" + value
    return value
