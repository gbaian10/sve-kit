"""Load the complete author file inventory and enforce its two-way index closure."""

import re
import stat
from dataclasses import dataclass, field, replace
from typing import TYPE_CHECKING, Never

from sve_carddb.contracts.author_leaves import AuthorIndexRecord, EffectsIndex
from sve_carddb.core.authored import check_path, require_directory
from sve_carddb.core.json import canonical, digest, object_value
from sve_carddb.core.yaml import MAX_BYTES
from sve_carddb.domains.effects.author_expansion import BodyLibrary
from sve_carddb.domains.effects.author_handwritten import BodyFile, MainFile, decode
from sve_carddb.domains.effects.author_inputs import (
    AuthorInputError,
    AuthorSchemas,
    Diagnostic,
    parse_document,
)

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.contracts.author_leaves import IndexRecord

STORAGE_FORMAT = 2

_INDEX = "rules/effects-index"
_AUTHOR = "rules/author-effects"
_BODIES = "rules/effect-bodies"
_PROTOTYPE = "rules/effects"
_OWNER = r"[A-Za-z0-9_-]+"
_SHARD = r"[0-9]{3,}\.yaml"
_AUTHOR_SHARD = r"(?:[0-9]{3,}|[A-Za-z0-9_]+-[^/\\\x00-\x1f\x7f]+)\.yaml"


@dataclass(frozen=True)
class InputFile:
    path: str
    raw: bytes = field(repr=False)
    input_hash: str


@dataclass(frozen=True)
class AuthorInputSet:
    index: EffectsIndex
    index_files: tuple[InputFile, ...]
    author_files: tuple[InputFile, ...]
    body_files: tuple[InputFile, ...]
    prototype_files: tuple[InputFile, ...]


def _reject(reason: str, path: str, record: IndexRecord | None = None) -> Never:
    raise AuthorInputError(
        Diagnostic(
            "index",
            reason,
            (path,),
            card_id=record.card_id if record else None,
            card_no=record.record_key if record else None,
            scope=record.scope if record else None,
        )
    )


def _read(root: Path, path: Path) -> InputFile:
    relative = path.relative_to(root).as_posix()
    try:
        check_path(root, path)
        if not stat.S_ISREG(path.stat().st_mode):
            _reject("nonregular_input", relative)
        with path.open("rb") as stream:
            raw = stream.read(MAX_BYTES)
    except AuthorInputError:
        raise
    except OSError, ValueError:
        _reject("unsafe_or_missing_input", relative)
    if len(raw) >= MAX_BYTES:
        _reject("size_limit", relative)
    return InputFile(relative, raw, digest(raw))


def _directory(root: Path, path: Path, *, optional: bool = False) -> bool:
    relative = path.relative_to(root).as_posix()
    try:
        check_path(root, path)
        if optional and not path.exists():
            return False
        require_directory(root, path)
    except OSError, ValueError:
        _reject("unsafe_or_missing_directory", relative)
    return True


def _files(root: Path, area: str, *, sharded: bool) -> tuple[InputFile, ...]:
    directory = root / area
    if not _directory(root, directory, optional=True):
        return ()
    result: list[InputFile] = []
    for entry in sorted(directory.iterdir()):
        if sharded:
            if re.fullmatch(_OWNER, entry.name) is None:
                _reject("invalid_input_path", entry.relative_to(root).as_posix())
            _directory(root, entry)
            for shard in sorted(entry.iterdir()):
                if (
                    re.fullmatch(
                        _AUTHOR_SHARD if area == _AUTHOR else _SHARD, shard.name
                    )
                    is None
                ):
                    _reject("invalid_input_path", shard.relative_to(root).as_posix())
                result.append(_read(root, shard))
        else:
            if re.fullmatch(_OWNER + r"\.yaml", entry.name) is None:
                _reject("invalid_input_path", entry.relative_to(root).as_posix())
            result.append(_read(root, entry))
    return tuple(result)


def _path_matches(path: str, area: str) -> bool:
    suffix = (
        _OWNER + r"\.yaml"
        if area in {_PROTOTYPE, _INDEX}
        else _OWNER + "/" + (_AUTHOR_SHARD if area == _AUTHOR else _SHARD)
    )
    return re.fullmatch(re.escape(area) + "/" + suffix, path) is not None


def _inventory(files: tuple[InputFile, ...], area: str) -> dict[str, InputFile]:
    result: dict[str, InputFile] = {}
    for item in files:
        if not _path_matches(item.path, area):
            _reject("invalid_input_path", item.path)
        if item.path in result:
            _reject("duplicate_input_file", item.path)
        if digest(item.raw) != item.input_hash:
            _reject("input_hash_mismatch", item.path)
        result[item.path] = item
    return result


def _tree(
    schemas: AuthorSchemas, item: InputFile, context: AuthorIndexRecord | None
) -> dict[str, JsonValue]:
    try:
        if item.path.startswith(_PROTOTYPE + "/"):
            return object_value(schemas.prototype_tree(item.raw))
        if object_value(parse_document(item.raw).value).get("format") == STORAGE_FORMAT:
            return object_value(schemas.storage_tree(item.raw, "author_effect_set"))
        return object_value(schemas.author_inventory_tree(item.raw, context=context))
    except AuthorInputError as error:
        raise AuthorInputError(
            replace(error.diagnostic, path=(item.path, *error.diagnostic.path))
        ) from None


def _selections(
    index: EffectsIndex,
    authors: dict[str, InputFile],
    prototypes: dict[str, InputFile],
) -> dict[str, dict[str, IndexRecord]]:
    selections: dict[str, dict[str, IndexRecord]] = {}
    for record in index.records:
        expected = _AUTHOR if record.kind == "author_effect_set" else _PROTOTYPE
        if not _path_matches(record.file, expected):
            _reject("invalid_index_path", record.file, record)
        selected = selections.setdefault(record.file, {})
        if record.record_key in selected:
            _reject("duplicate_record_selection", record.file, record)
        selected[record.record_key] = record
        if record.file not in (authors if expected == _AUTHOR else prototypes):
            _reject("index_file_missing", record.file, record)
    return selections


def merge_index_files(
    files: tuple[InputFile, ...], schemas: AuthorSchemas
) -> EffectsIndex:
    """Validate pack ownership and retain global identity/scope uniqueness."""
    records: list[IndexRecord] = []
    for path, item in _inventory(files, _INDEX).items():
        index = schemas.effects_index(item.raw)
        owner = path.rsplit("/", 1)[-1].removesuffix(".yaml")
        for record in index.records:
            if record.record_key.split("-", 1)[0] != owner:
                _reject("wrong_index_owner", path, record)
        records.extend(index.records)
    records.sort(key=lambda record: (record.record_key, record.scope))
    return schemas.effects_index(
        canonical(
            EffectsIndex(
                format=1, kind="effects_index", records=tuple(records)
            ).model_dump(mode="json")
        )
    )


def _validate_index(inputs: AuthorInputSet, schemas: AuthorSchemas) -> None:
    if not inputs.index_files and (
        inputs.index.records or inputs.author_files or inputs.body_files
    ):
        _reject("index_missing", _INDEX)
    if merge_index_files(inputs.index_files, schemas) != inputs.index:
        _reject("index_input_mismatch", _INDEX)


def validate_author_set(inputs: AuthorInputSet, schemas: AuthorSchemas) -> None:
    """Revalidate the explicit byte inventory; only the loader inspects disk."""
    _validate_index(inputs, schemas)
    authors = _inventory(inputs.author_files, _AUTHOR)
    prototypes = _inventory(inputs.prototype_files, _PROTOTYPE)
    _inventory(inputs.body_files, _BODIES)
    selections = _selections(inputs.index, authors, prototypes)
    for path, item in {**authors, **prototypes}.items():
        selected = selections.get(path, {})
        context = next(
            (
                record
                for record in selected.values()
                if isinstance(record, AuthorIndexRecord)
            ),
            None,
        )
        tree = _tree(schemas, item, context)
        if path in authors and not selected:
            _reject("orphan_author_file", path)
        if selected:
            cards = object_value(tree["cards"])
            if set(cards) != set(selected):
                _reject("index_record_closure", path)
            for record in selected.values():
                if (
                    tree["kind"] != record.kind
                    or tree.get("scope", "shared") != record.scope
                ):
                    _reject("index_envelope_mismatch", path, record)

    _validate_storage(inputs, schemas)


def _validate_storage(inputs: AuthorInputSet, schemas: AuthorSchemas) -> None:
    formats = {
        object_value(parse_document(item.raw).value).get("format")
        for item in (*inputs.author_files, *inputs.body_files)
    }
    if len(formats) > 1:
        _reject("mixed_storage_formats", _AUTHOR)
    if formats == {STORAGE_FORMAT}:
        _handwritten_inventory(inputs, schemas)
    else:
        BodyLibrary(schemas, (item.raw for item in inputs.body_files))


def load_author_set(authored_root: Path, schemas: AuthorSchemas) -> AuthorInputSet:
    """Read every entry in the three fixed areas, with the index parsed first."""
    _directory(authored_root, authored_root)
    _directory(authored_root, authored_root / "rules", optional=True)
    index_files = _files(authored_root, _INDEX, sharded=False)
    if not index_files and any(
        _directory(authored_root, authored_root / area, optional=True)
        for area in (_AUTHOR, _BODIES)
    ):
        _reject("index_missing", _INDEX)
    result = AuthorInputSet(
        merge_index_files(index_files, schemas),
        index_files,
        _files(authored_root, _AUTHOR, sharded=True),
        _files(authored_root, _BODIES, sharded=True),
        _files(authored_root, _PROTOTYPE, sharded=False),
    )
    for item in (*result.author_files, *result.body_files):
        if object_value(parse_document(item.raw).value).get("format") != STORAGE_FORMAT:
            _reject("unsupported_storage_format", item.path)
    validate_author_set(result, schemas)
    return result


def _handwritten_inventory(inputs: AuthorInputSet, schemas: AuthorSchemas) -> None:
    names: set[tuple[str, int]] = set()
    for item in inputs.author_files:
        decode(parse_document(item.raw), MainFile)
        for number in object_value(
            object_value(parse_document(item.raw).value)["cards"]
        ):
            if item.path.split("/")[-2] != number.split("-", 1)[0]:
                _reject("wrong_owner", item.path)
    for item in inputs.body_files:
        try:
            schemas.storage_tree(item.raw, "effect_body_shard")
        except AuthorInputError as error:
            raise AuthorInputError(
                replace(error.diagnostic, path=(item.path, *error.diagnostic.path))
            ) from None
        for body in decode(parse_document(item.raw), BodyFile).records:
            key = body.id, body.edition
            if key in names:
                _reject("duplicate_body_id", item.path)
            names.add(key)
