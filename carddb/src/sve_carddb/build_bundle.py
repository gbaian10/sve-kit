"""Publish and verify an inseparable offline DB, input record and report bundle."""

import os
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, field_validator

from sve_carddb.build_db import create_database
from sve_carddb.build_db.database import open_database
from sve_carddb.build_inputs import BuildContext, InputRecord, SourceUse
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.registry.records import Hash, RecordData
from sve_carddb.snapshot.values import canonical, digest, parse
from sve_carddb.source_archive import _fsync_dir, _rename_no_replace

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping

    from sve_carddb.build_db import CompiledSchema, Database


class BundleSeal(RecordData):
    bundle_format: Literal[1] = 1
    database_sha256: Hash
    inputs_sha256: Hash
    report_sha256: Hash

    @field_validator("bundle_format", mode="before")
    @classmethod
    def check_format(cls, value: object) -> object:
        """Do not accept a JSON boolean as the integer format version."""
        if type(value) is not int:
            raise ValueError("Build bundle format must be integer 1")
        return value


def publish_bundle(  # ruff: ignore[too-many-arguments] -- publication binds schema, expected inputs, population, report and store resolution
    schema: CompiledSchema,
    destination: Path,
    context: BuildContext,
    expected: tuple[SourceUse, ...],
    populate: Callable[[Database], InputRecord],
    report: JsonValue,
    *,
    stores: Mapping[str, Path],
) -> InputRecord:
    """Validate inside the build transaction and publish only a complete new directory."""
    if destination.exists() or destination.is_symlink():
        raise FileExistsError("Build bundle destination already exists")
    with TemporaryDirectory(
        prefix=".build-bundle-", dir=destination.parent
    ) as temporary:
        candidate = Path(temporary) / "bundle"
        candidate.mkdir()
        with create_database(schema, candidate / "build.sqlite") as db:
            with db.transaction():
                record = populate(db)
                record.verify(db, context, expected)
                _verify_sources(record, stores)
        inputs = record.content()
        (candidate / "inputs.json").write_bytes(inputs)
        (candidate / "report.json").write_bytes(
            canonical({"build_inputs_hash": digest(inputs), "report": report})
        )
        seal = BundleSeal(
            database_sha256=digest((candidate / "build.sqlite").read_bytes()),
            inputs_sha256=digest(inputs),
            report_sha256=digest((candidate / "report.json").read_bytes()),
        )
        (candidate / "seal.json").write_bytes(canonical(seal.model_dump(mode="json")))
        for path in candidate.iterdir():
            with path.open("rb") as file:
                os.fsync(file.fileno())
        _fsync_dir(candidate)
        _rename_no_replace(candidate, destination)
        _fsync_dir(destination.parent)
    return record


def verify_bundle(
    schema: CompiledSchema,
    root: Path,
    context: BuildContext,
    expected: tuple[SourceUse, ...],
    *,
    stores: Mapping[str, Path],
) -> InputRecord:
    """Verify exact artifact hashes, DB graph, expected uses and archived raw closure."""
    names = {"build.sqlite", "inputs.json", "report.json", "seal.json"}
    if root.is_symlink() or {path.name for path in root.iterdir()} != names:
        raise ValueError("Build bundle file closure mismatch")
    contents: dict[str, bytes] = {}
    for name in sorted(names):
        path = root / name
        if path.is_symlink() or not path.is_file():
            raise ValueError("Unsafe build bundle file")
        contents[name] = path.read_bytes()
    seal = BundleSeal.model_validate_json(contents["seal.json"])
    if canonical(seal.model_dump(mode="json")) != contents["seal.json"]:
        raise ValueError("Noncanonical build bundle seal")
    for name, checksum in (
        ("build.sqlite", seal.database_sha256),
        ("inputs.json", seal.inputs_sha256),
        ("report.json", seal.report_sha256),
    ):
        if digest(contents[name]) != checksum:
            raise ValueError("Build bundle content hash mismatch")
    record = InputRecord.model_validate_json(contents["inputs.json"])
    if record.content() != contents["inputs.json"]:
        raise ValueError("Noncanonical build input record")
    report = parse(contents["report.json"])
    if (
        not isinstance(report, dict)
        or set(report) != {"report", "build_inputs_hash"}
        or report["build_inputs_hash"] != seal.inputs_sha256
        or canonical(report) != contents["report.json"]
    ):
        raise ValueError("Build report input reference mismatch")
    with open_database(schema, root / "build.sqlite") as db:
        record.verify(db, context, expected)
        db.verify()
    _verify_sources(record, stores)
    return record


def _verify_sources(record: InputRecord, stores: Mapping[str, Path]) -> None:
    batches: dict[tuple[str, str], FrozenSources] = {}
    for source in {use.source for use in record.uses}:
        pin = source.archive
        key = pin.store_id, pin.batch_id
        if key not in batches:
            root = stores.get(pin.store_id)
            if root is None:
                raise ValueError(
                    "Build input requires an explicitly configured archive store"
                )
            batches[key] = FrozenSources(root, *key)
        actual, _, _ = batches[key].read(
            source.id, parser_version=source.parser_version
        )
        if actual != source:
            raise ValueError("Build input archive provenance mismatch")
