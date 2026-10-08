"""Independent, in-memory contract reader for shared synthetic fixtures.

This reference implementation accepts already decompressed bytes. It is not the
browser's streaming store, a publisher, or a validator of build-only evidence.
"""

from dataclasses import dataclass
from graphlib import CycleError, TopologicalSorter
from operator import itemgetter
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.profiles import MEDIA, profile
from sve_carddb.contracts.snapshot import (
    decode,
    definition,
    descriptor,
    required_types,
    row_type,
    tables,
    validate,
)
from sve_carddb.core.json import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)
from sve_carddb.snapshot.reader_media import validate_digital, validate_media
from sve_carddb.snapshot.semantics import (
    validate_config,
    validate_fragments,
    validate_placement,
    validate_view,
)

if TYPE_CHECKING:
    from collections.abc import Mapping

type Row = dict[str, JsonValue]
type View = dict[str, list[Row]]


@dataclass
class Fragment:
    """A validated fragment and its exact source file."""

    file: str
    table: str
    value: Row
    rows: list[Row]

    @property
    def identity(self) -> bytes:
        """The table/owner/bucket/partition identity."""
        return canonical(
            [
                self.table,
                self.value["owner"],
                self.value["bucket"],
                self.value["partition"],
            ]
        )


def _reference(file: Row) -> Row:
    return {"key": file["key"], "sha256": file["sha256"]}


def _ordered(values: list[JsonValue]) -> None:
    if values != sorted(values, key=string) or len(
        {canonical(value) for value in values}
    ) != len(values):
        raise ValueError("Expected sorted unique values")


def _files(manifest: Row) -> dict[str, Row]:
    result = {
        string(object_value(item)["key"]): object_value(item)
        for item in array(manifest["files"])
    }
    if len(result) != len(array(manifest["files"])) or list(result) != sorted(result):
        raise ValueError("File keys must be sorted and unique")
    for file in result.values():
        dependencies = array(file["dependencies"])
        _ordered([object_value(item)["key"] for item in dependencies])
        for item in dependencies:
            dep = object_value(item)
            if dep != _reference(result[string(dep["key"])]):
                raise ValueError("Dependency hash mismatch")
    graph = {
        key: [string(object_value(dep)["key"]) for dep in array(file["dependencies"])]
        for key, file in result.items()
    }
    try:
        TopologicalSorter(graph).prepare()
    except CycleError as exc:
        raise ValueError("Cyclic dependencies") from exc
    return result


def _payload(file: Row, data: bytes) -> JsonValue:
    if digest(data) != file["sha256"] or len(data) != file["bytes"]:
        raise ValueError("Blob hash or length mismatch")
    if file["path"] != "snapshots/blobs/" + string(file["sha256"])[7:] + ".json":
        raise ValueError("Blob path does not match hash")
    value = parse(data)
    if canonical(value) != data:
        raise ValueError("Noncanonical payload bytes")
    return value


def _container(file: Row, value: Row) -> list[Fragment]:
    selected = profile(string(value["format_version"]))
    validate("Container", value, selected.version)
    result: list[Fragment] = []
    used: set[str] = set()
    counts: list[JsonValue] = []
    for table, entries in object_value(value["tables"]).items():
        for entry in array(entries):
            fragment = object_value(entry)
            part = string(fragment["partition"])
            role = (
                "images"
                if table in {"image_asset", "printing_image", "image_variant"}
                else "bootstrap"
                if part == "bootstrap"
                else "text"
            )
            if (
                role != file["role"]
                or not 0 <= integer(fragment["bucket"]) < selected.buckets
            ):
                raise ValueError("Fragment role or bucket does not match profile")
            name = row_type(table, part, selected.version)
            used |= required_types(name, selected.version)
            rows = [
                decode(name, row, selected.version) for row in array(fragment["rows"])
            ]
            result.append(Fragment(string(file["key"]), table, fragment, rows))
            counts.append(
                {
                    "table": table,
                    **{key: fragment[key] for key in ("owner", "bucket", "partition")},
                    "count": len(rows),
                }
            )
    expected: Row = {name: descriptor(name, selected.version) for name in sorted(used)}
    if value["types"] != expected:
        raise ValueError("Missing, unused or altered nested descriptor")
    if sorted(map(canonical, counts)) != sorted(
        canonical(item) for item in array(file["row_counts"])
    ):
        raise ValueError("Fragment row counts mismatch")
    return result


def _load(
    files: dict[str, Row], payloads: Mapping[str, bytes], version: str
) -> list[Fragment]:
    if set(files) != set(payloads):
        raise ValueError("Payload set does not match manifest")
    result: list[Fragment] = []
    for key, file in files.items():
        value = object_value(_payload(file, payloads[key]))
        if value["format_version"] != version:
            raise ValueError("Payload format differs from manifest")
        role = string(file["role"])
        if role in {"config", "programs"}:
            validate("Config" if role == "config" else "Programs", value, version)
            if role == "config":
                validate_config(value)
            if file["row_counts"] != []:
                raise ValueError("Non-table file has row counts")
        else:
            result.extend(_container(file, value))
    if len({fragment.identity for fragment in result}) != len(result):
        raise ValueError("Duplicate fragment identity")
    return result


def _base(
    detail: Fragment, fragments: list[Fragment], files: dict[str, Row]
) -> Fragment:
    base = object_value(detail.value["base"])
    reference = object_value(base["file"])
    key = string(reference["key"])
    if (
        key == detail.file
        or reference != _reference(files[key])
        or reference not in array(files[detail.file]["dependencies"])
    ):
        raise ValueError("Missing or incorrect base dependency")
    if base["table"] != detail.table or any(
        base[field] != detail.value[field] for field in ("owner", "bucket")
    ):
        raise ValueError("Base fragment identity mismatch")
    candidates = [
        f
        for f in fragments
        if f.file == key
        and f.table == detail.table
        and f.value["partition"] == "bootstrap"
        and all(f.value[field] == base[field] for field in ("owner", "bucket"))
    ]
    if len(candidates) != 1:
        raise ValueError("Base fragment not found")
    return candidates[0]


def _translations(left: JsonValue, right: JsonValue) -> list[JsonValue]:
    merged = array(left) + array(right)
    keys = [
        (
            string(object_value(item)["field"]),
            -1
            if object_value(item)["ordinal"] is None
            else integer(object_value(item)["ordinal"]),
            string(object_value(item)["target_lang"]),
        )
        for item in merged
    ]
    if len(set(keys)) != len(keys):
        raise ValueError("Duplicate translation selection")
    return [
        item for _, item in sorted(zip(keys, merged, strict=True), key=itemgetter(0))
    ]


def _printing(base: Row, detail: Row, faces: dict[str, Row]) -> Row:
    left = [object_value(item) for item in array(base["faces"])]
    right = [object_value(item) for item in array(detail["faces"])]
    ordinals = [integer(faces[string(item["face_id"])]["ordinal"]) for item in left]
    if (
        ordinals != sorted(set(ordinals))
        or [integer(item["face_ordinal"]) for item in right] != ordinals
    ):
        raise ValueError("Printing faces do not match exact permanent ordinals")
    merged: list[JsonValue] = []
    for first, second in zip(left, right, strict=True):
        if faces[string(first["face_id"])]["card_id"] != base["card_id"]:
            raise ValueError("Printing face belongs to another card")
        merged.append(
            first
            | {key: value for key, value in second.items() if key != "face_ordinal"}
        )
    return base | {"faces": merged}


def _join(detail: Fragment, base: Fragment, faces: dict[str, Row]) -> list[Row]:
    if [integer(row["row_index"]) for row in detail.rows] != list(
        range(len(base.rows))
    ):
        raise ValueError("Missing, duplicate, unordered or out-of-range row_index")
    result: list[Row] = []
    for first, second in zip(base.rows, detail.rows, strict=True):
        if detail.table == "printing":
            result.append(_printing(first, second, faces))
        else:
            if any(
                object_value(item)["field"] != "name"
                for item in array(first["translations"])
            ) or any(
                object_value(item)["field"] == "name"
                for item in array(second["translations"])
            ):
                raise ValueError("Translations placed in wrong column partition")
            result.append(
                first
                | {key: value for key, value in second.items() if key != "row_index"}
                | {
                    "translations": _translations(
                        first["translations"], second["translations"]
                    )
                }
            )
    return result


def _logical(fragments: list[Fragment], files: dict[str, Row]) -> View:
    view: View = {table: [] for table in tables()}
    faces = {
        string(row["id"]): row for f in fragments if f.table == "face" for row in f.rows
    }
    joined: set[bytes] = set()
    for fragment in fragments:
        part = fragment.value["partition"]
        if fragment.table in {"printing", "face_revision"} and part != "history":
            if part == "bootstrap":
                continue
            base = _base(fragment, fragments, files)
            if base.identity in joined:
                raise ValueError("Multiple details for one base")
            joined.add(base.identity)
            view[fragment.table].extend(_join(fragment, base, faces))
        else:
            view[fragment.table].extend(fragment.rows)
    expected = {
        f.identity
        for f in fragments
        if f.table in {"printing", "face_revision"}
        and f.value["partition"] == "bootstrap"
    }
    if joined != expected:
        raise ValueError("Missing detail fragment")
    return view


def _key(row: Row, fields: list[JsonValue]) -> bytes:
    return canonical([row[string(field)] for field in fields])


def _unique(view: View) -> None:
    for table, rows in view.items():
        fields = array(definition(table)["x-primary-key"])
        keys = [_key(row, fields) for row in rows]
        if len(set(keys)) != len(keys):
            raise ValueError("Duplicate public primary key")
        rows.sort(key=lambda row: _key(row, fields))


_REFERENCES = {
    "card_id": "card",
    "from_card_id": "card",
    "to_card_id": "card",
    "old_card_id": "card",
    "new_card_id": "card",
    "face_id": "face",
    "printing_id": "printing",
    "default_printing_id": "printing",
    "art_id": "art",
    "artist_id": "artist",
    "stamp_id": "stamp",
    "home_set_id": "product_family",
    "family_id": "product_family",
    "product_id": "product",
    "translation_id": "translation",
    "qa_id": "qa",
    "qa_version_id": "qa_version",
    "current_version_id": "qa_version",
    "cr_version_id": "cr_version",
    "cr_clause_id": "cr_clause",
    "revision_id": "face_revision",
    "replacement_revision_id": "ruling_revision",
    "rules_name_id": "rules_name",
    "profile_id": "rules_profile",
    "digital_card_id": "digital_card",
    "digital_art_id": "digital_art",
    "interaction_target_id": "digital_card",
    "voice_id": "voice",
    "keyword_id": "keyword",
    "image_id": "image_asset",
}
_ARRAY_REFERENCES = {
    "cards": "card",
    "debut_product_ids": "product",
    "active_scopes": "text_unit",
    "ruling_revision_ids": "ruling_revision",
    "complete_keyword_ids": "keyword",
    "partial_keyword_ids": "keyword",
    "undated_printing_ids": "printing",
}


def _walk(value: JsonValue, targets: dict[str, set[str]]) -> None:
    if isinstance(value, list):
        for item in value:
            _walk(item, targets)
    if not isinstance(value, dict):
        return
    for key, item in value.items():
        target = "text_unit" if key.endswith("_unit_id") else _REFERENCES.get(key)
        if (
            target is not None
            and item is not None
            and string(item) not in targets[target]
        ):
            raise ValueError(f"Dangling reference: {key}")
        if key in _ARRAY_REFERENCES and any(
            string(member) not in targets[_ARRAY_REFERENCES[key]]
            for member in array(item)
        ):
            raise ValueError(f"Dangling array reference: {key}")
        if key == "program_ref" and item is not None:
            raise ValueError("No programs supported in this profile")
        _walk(item, targets)


def _closure(view: View, manifest: Row) -> None:
    targets = {
        table: {string(row["id"]) for row in rows if "id" in row}
        for table, rows in view.items()
    }
    for rows in view.values():
        for row in rows:
            _walk(row, targets)
    for card in view["card"]:
        actual = [
            row["id"]
            for row in sorted(view["face"], key=lambda row: integer(row["ordinal"]))
            if row["card_id"] == card["id"]
        ]
        if card["faces"] != actual:
            raise ValueError("Card face closure mismatch")
    for key in ("qa_card_ids", "errata_card_ids"):
        _ordered(array(manifest[key]))
        if any(string(item) not in targets["card"] for item in array(manifest[key])):
            raise ValueError("Manifest card reference missing")
    for row in view["text_unit"]:
        expected = (
            "t:"
            + string(row["lang"])
            + ":"
            + digest(string(row["text"]).encode())[7:23]
        )
        if row["id"] != expected:
            raise ValueError("Text ID does not match exact text")


def _current(view: View, fragments: list[Fragment]) -> None:
    refs = {
        string(object_value(item)["revision_id"])
        for row in view["face"]
        for item in array(row["current"])
    }
    refs |= {
        string(display["revision_id"])
        for row in view["face"]
        for raw in array(row["wording"])
        for display in (object_value(object_value(raw)["display"]),)
        if display["revision_id"] is not None
    }
    current = {
        string(row["id"])
        for f in fragments
        if f.table == "face_revision" and f.value["partition"] == "bootstrap"
        for row in f.rows
    }
    if refs != current:
        raise ValueError("Current/display/history partition mismatch")
    revisions = {string(row["id"]): row for row in view["face_revision"]}
    for face in view["face"]:
        for item in array(face["current"]):
            entry = object_value(item)
            revision = revisions[string(entry["revision_id"])]
            if (
                revision["face_id"] != face["id"]
                or revision["region"] != entry["region"]
            ):
                raise ValueError("Current revision belongs to another face or region")


def _metadata(manifest: Row, files: dict[str, Row]) -> None:
    for field in ("regions", "languages", "required_capabilities"):
        _ordered(array(manifest[field]))
    text_files = {
        key
        for key, file in files.items()
        if file["role"] in {"bootstrap", "text", "config"}
    }
    if manifest["text_all"] is not None:
        expected: list[JsonValue] = [
            _reference(files[key]) for key in sorted(text_files)
        ]
        if object_value(manifest["text_all"])["contains"] != expected:
            raise ValueError("text_all contains does not match files")
    for key, file in files.items():
        dependencies = {
            string(object_value(dep)["key"]) for dep in array(file["dependencies"])
        }
        if key in text_files and not dependencies <= text_files:
            raise ValueError("Text dependencies escape offline closure")
        if file["role"] == "bootstrap" and any(
            files[dep]["role"] == "text" for dep in dependencies
        ):
            raise ValueError("Bootstrap cannot depend on detail/history")
        if file["role"] == "programs" and dependencies:
            raise ValueError("Empty programs cannot have dependencies")


def read_snapshot(manifest_value: JsonValue, payloads: Mapping[str, bytes]) -> View:
    """Verify every file before joining a complete snapshot into logical rows.

    Missing keys and invalid shapes intentionally propagate as errors; the reader
    never repairs a dependency or falls back to another snapshot.
    """
    manifest = object_value(manifest_value)
    validate("Manifest", manifest, string(manifest["format_version"]))
    selected = profile(string(manifest["format_version"]))
    if tuple(map(int, string(manifest["min_reader_version"]).split("."))) > tuple(
        map(int, selected.version.split("."))
    ) or set(map(string, array(manifest["required_capabilities"]))) != set(
        selected.capabilities
    ):
        raise ValueError("Unsupported reader version or capability")
    files = _files(manifest)
    _metadata(manifest, files)
    configs = [file for file in files.values() if file["role"] == "config"]
    programs = [file for file in files.values() if file["role"] == "programs"]
    if (
        len(configs) != 1
        or manifest["config_ref"] != _reference(configs[0])
        or len(programs) != 1
    ):
        raise ValueError("Expected exactly one config and programs file")
    for file in files.values():
        if file["role"] == "images" and manifest["config_ref"] not in array(
            file["dependencies"]
        ):
            raise ValueError("Images must depend on config")
    fragments = _load(files, payloads, selected.version)
    validate_fragments(fragments)
    view = _logical(fragments, files)
    _unique(view)
    _closure(view, manifest)
    _current(view, fragments)
    validate_view(view, manifest, fragments)
    validate_placement(view, manifest, fragments)
    validate_media(view, fragments, files, object_value(manifest["config_ref"]))
    config = object_value(
        parse(payloads[string(object_value(manifest["config_ref"])["key"])])
    )
    validate_digital(view, config)
    return view


def read_text_all(
    manifest_value: JsonValue, data: bytes, attachments: Mapping[str, bytes]
) -> View:
    """Validate alternative member bytes, then use the same independent reader."""
    manifest = object_value(manifest_value)
    validate("Manifest", manifest, string(manifest["format_version"]))
    description = object_value(manifest["text_all"])
    value = object_value(_payload(description, data))
    validate("TextAll", value, string(manifest["format_version"]))
    if value["format_version"] != manifest["format_version"]:
        raise ValueError("TextAll format differs from manifest")
    files = _files(manifest)
    expected = [
        _reference(file)
        for file in files.values()
        if file["role"] in {"bootstrap", "text", "config"}
    ]
    if description["contains"] != expected:
        raise ValueError("text_all membership mismatch")
    members = [object_value(item) for item in array(value["members"])]
    if [_reference(member) for member in members] != expected:
        raise ValueError("text_all members do not match contains")
    if any(
        string(object_value(dep)["key"])
        not in {string(item["key"]) for item in expected}
        for item in expected
        for dep in array(files[string(item["key"])]["dependencies"])
    ):
        raise ValueError("text_all dependencies escape its closure")
    payloads = {
        string(member["key"]): canonical(member["payload"]) for member in members
    }
    if set(payloads) & set(attachments):
        raise ValueError("Duplicate alternative payload")
    return read_snapshot(manifest, payloads | dict(attachments))


def _compatible(entry: Row) -> bool:
    try:
        selected = profile(string(entry["format_version"]))
    except ValueError:
        return False
    minimum = tuple(map(int, string(entry["min_reader_version"]).split(".")))
    return minimum <= tuple(map(int, selected.version.split("."))) and set(
        map(string, array(entry["required_capabilities"]))
    ) <= set(selected.capabilities)


def select_index_entry(value: JsonValue) -> Row | None:
    """Negotiate current, then previous; unknown capability is not corruption."""
    index = object_value(value)
    validate("Index", index, MEDIA)
    for raw in (index["current"], index["previous"]):
        if raw is not None and _compatible(object_value(raw)):
            return object_value(raw)
    return None


def read_index(value: JsonValue, manifests: Mapping[str, bytes]) -> Row:
    """Verify the readable retained window, without loading incompatible manifests."""
    index = object_value(value)
    validate("Index", index, MEDIA)
    entries = [object_value(index["current"])]
    if index["previous"] is not None:
        entries.append(object_value(index["previous"]))
    if len({entry["data_version"] for entry in entries}) != len(entries):
        raise ValueError("Index current and previous must be distinct releases")
    for entry in entries:
        _ordered(array(entry["required_capabilities"]))
        if (
            entry["manifest_path"]
            != "snapshots/manifests/" + string(entry["manifest_sha256"])[7:] + ".json"
        ):
            raise ValueError("Index manifest path must match its hash")
    readable = [e for e in entries if _compatible(e)]
    if set(manifests) != {string(e["manifest_sha256"]) for e in readable}:
        raise ValueError("Index manifest set must equal readable window")
    for entry in readable:
        raw = manifests[string(entry["manifest_sha256"])]
        manifest = object_value(parse(raw))
        if digest(raw) != entry["manifest_sha256"] or canonical(manifest) != raw:
            raise ValueError("Index manifest hash or canonical bytes mismatch")
        validate("Manifest", manifest, string(entry["format_version"]))
        if any(
            entry[field] != manifest[field]
            for field in (
                "data_version",
                "published_at",
                "format_version",
                "min_reader_version",
                "required_capabilities",
                "engine_support_target",
            )
        ):
            raise ValueError("Index entry differs from retained manifest")
    return index
