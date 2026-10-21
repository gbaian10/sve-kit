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
from sve_carddb.domains.effects.author_inputs import (
    AuthorInputError,
    AuthorSchemas,
    Diagnostic,
)

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.contracts.author_leaves import IndexRecord

_INDEX = "rules/effects-index.yaml"
_AUTHOR = "rules/author-effects"
_BODIES = "rules/effect-bodies"
_PROTOTYPE = "rules/effects"
_OWNER = r"[A-Za-z0-9_-]+"
_SHARD = r"[0-9]{3,}\.yaml"


@dataclass(frozen=True)
class InputFile:
    path: str
    raw: bytes = field(repr=False)
    input_hash: str


@dataclass(frozen=True)
class AuthorInputSet:
    index: EffectsIndex
    index_file: InputFile | None
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
                if re.fullmatch(_SHARD, shard.name) is None:
                    _reject("invalid_input_path", shard.relative_to(root).as_posix())
                result.append(_read(root, shard))
        else:
            if re.fullmatch(_OWNER + r"\.yaml", entry.name) is None:
                _reject("invalid_input_path", entry.relative_to(root).as_posix())
            result.append(_read(root, entry))
    return tuple(result)


def _path_matches(path: str, area: str) -> bool:
    suffix = _OWNER + r"\.yaml" if area == _PROTOTYPE else _OWNER + "/" + _SHARD
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


def _validate_index(inputs: AuthorInputSet, schemas: AuthorSchemas) -> None:
    try:
        raw = canonical(inputs.index.model_dump(mode="json"))
    except ValueError, TypeError:
        _reject("index_input_mismatch", _INDEX)
    schemas.effects_index(raw)
    if inputs.index_file is None:
        if inputs.index.records or inputs.author_files or inputs.body_files:
            _reject("index_missing", _INDEX)
    else:
        item = inputs.index_file
        if item.path != _INDEX or item.input_hash != digest(item.raw):
            _reject("index_input_mismatch", _INDEX)
        if schemas.effects_index(item.raw) != inputs.index:
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
    BodyLibrary(schemas, (item.raw for item in inputs.body_files))


def load_author_set(authored_root: Path, schemas: AuthorSchemas) -> AuthorInputSet:
    """Read every entry in the three fixed areas, with the index parsed first."""
    _directory(authored_root, authored_root)
    _directory(authored_root, authored_root / "rules", optional=True)
    index_path = authored_root / _INDEX
    try:
        check_path(authored_root, index_path)
    except ValueError:
        _reject("unsafe_or_missing_input", _INDEX)
    if index_path.exists():
        index_file = _read(authored_root, index_path)
        index = schemas.effects_index(index_file.raw)
    else:
        index_file = None
        if any(
            _directory(authored_root, authored_root / area, optional=True)
            for area in (_AUTHOR, _BODIES)
        ):
            _reject("index_missing", _INDEX)
        index = EffectsIndex(format=1, kind="effects_index", records=())
    result = AuthorInputSet(
        index,
        index_file,
        _files(authored_root, _AUTHOR, sharded=True),
        _files(authored_root, _BODIES, sharded=True),
        _files(authored_root, _PROTOTYPE, sharded=False),
    )
    validate_author_set(result, schemas)
    return result
