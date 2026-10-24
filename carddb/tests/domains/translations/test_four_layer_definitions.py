"""Shared storage preserves identity and rejects incomplete or corrupt closures."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.four_layer import hash_payload
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_authored import (
    FrameRecord,
    from_files,
    read_inputs,
)
from sve_carddb.domains.translations.four_layer_definitions import (
    AREA,
    INDEX,
    Definitions,
    Entry,
    pack,
)
from sve_carddb.domains.translations.four_layer_definitions import (
    from_files as definition_files,
)
from sve_carddb.domains.translations.four_layer_derivation import derive_source

from ...support.semantic_definition_fixtures import with_definitions
from .test_four_layer_n1 import n1_classifier
from .test_four_layer_normalizer import source

if TYPE_CHECKING:
    from pathlib import Path


def test_shared_authored_round_trip_preserves_frame_payload_and_binding(
    tmp_path: Path,
) -> None:
    raw = "Synthetic example."
    classifier = n1_classifier()
    derived = derive_source(raw, source(raw), classifier)
    frame, binding = derived.bind(0)
    content = canonical(
        {
            "format": 3,
            "kind": "translation_shard",
            "records": [
                {"kind": "sentence_template", "data": frame.model_dump(mode="json")}
            ],
        }
    )
    encoded = with_definitions(
        (("translations/templates/definitions/001.yaml", content, content),)
    )
    for relative, data, _ in encoded:
        path = tmp_path / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    loaded = read_inputs(tmp_path)
    record = loaded.records[0]
    assert isinstance(record, FrameRecord)
    expected = derived.field.parts[0].canonical_source
    assert canonical(record.data.payload(expected)) == canonical(
        frame.payload(expected)
    )
    assert record.data.id == frame.id
    derived.verify_binding(raw, classifier, record.data, binding)
    assert "bind:" + hash_payload(binding.payload()) == binding.id
    with pytest.raises(ValueError, match="requires a semantic definition reference"):
        from_files((("translations/templates/definitions/001.yaml", content, content),))


def test_repeated_content_escapes_literals_and_appends_without_renumbering() -> None:
    shared: JsonValue = {
        "a_long_semantic_name": ["@literal", "@@literal", 1, True, None]
    }
    first: JsonValue = {"@key": shared, "other": shared}
    refs, entries = pack([first, first])
    assert refs[0] == refs[1]
    loaded = Definitions(entries)
    assert canonical(loaded.get(refs[0])) == canonical(first)
    later, extended = pack([shared, first], existing=entries)
    assert later[1] == refs[0]
    assert extended[: len(entries)] == entries
    assert canonical(Definitions(extended).get(later[0])) == canonical(shared)
    detached = loaded.get(refs[0])
    assert isinstance(detached, dict)
    detached.clear()
    assert canonical(loaded.get(refs[0])) == canonical(first)


@pytest.mark.parametrize("value", ["@2", "@1", "@01", {"@2": 1}])
def test_missing_cyclic_or_malformed_nested_reference_is_rejected(
    value: JsonValue,
) -> None:
    with pytest.raises(ValueError, match=r"[Mm]issing|[Cc]yclic|[Ii]nvalid"):
        Definitions((Entry(id=1, hash=hash_payload({}), value=value),))


def test_content_hash_distinguishes_boolean_from_integer() -> None:
    with pytest.raises(ValueError, match="hash mismatch"):
        Definitions(
            (Entry(id=1, hash=hash_payload({"value": True}), value={"value": 1}),)
        )


def test_duplicate_handles_and_hashes_are_rejected() -> None:
    first = Entry(id=1, hash=hash_payload("first"), value="first")
    second = Entry(id=1, hash=hash_payload("second"), value="second")
    for entries in [(first, second), (first, first.model_copy(update={"id": 2}))]:
        with pytest.raises(ValueError, match="Duplicate"):
            Definitions(entries)


def test_expanded_keys_must_be_strings_and_unique() -> None:
    values: list[JsonValue] = [17, "key"]
    for value in values:
        child = Entry(id=1, hash=hash_payload(value), value=value)
        parent = Entry(id=2, hash=hash_payload({}), value={"@1": 0, "key": 1})
        with pytest.raises(ValueError, match="expanded definition key"):
            Definitions((child, parent))


@pytest.mark.parametrize(
    "includes", [["../001.yaml"], ["001.yaml", "001.yaml"], ["002.yaml"], []]
)
def test_index_covers_exactly_the_safe_shard_set(includes: list[str]) -> None:
    paths: list[JsonValue] = list(includes)
    index = canonical({"format": 1, "kind": "frame_semantics_index", "includes": paths})
    shard = canonical({"format": 1, "kind": "frame_semantics_shard", "definitions": []})
    with pytest.raises(ValueError, match="index"):
        definition_files(((INDEX, index, index), (AREA + "001.yaml", shard, shard)))


def test_missing_index_or_duplicate_file_cannot_be_ignored() -> None:
    shard = canonical({"format": 1, "kind": "frame_semantics_shard", "definitions": []})
    file = (AREA + "001.yaml", shard, shard)
    for files in [(file,), (file, file)]:
        with pytest.raises(ValueError, match=r"index|Duplicate"):
            definition_files(files)
