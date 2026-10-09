"""Current catalog boundaries over invented values, without revision fixtures."""

import copy
import shutil
from dataclasses import replace
from typing import TYPE_CHECKING, cast

import pytest

from sve_carddb.core.json import array, object_value
from sve_carddb.domains.catalog.adoption_loader import load_adoptions
from sve_carddb.domains.registry.storage import read_yaml

from .adoption_fixtures import Case, envelope, make_case, record, write

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.domains.catalog.adoption_loader import Entry


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> Case:
    return make_case(tmp_path_factory.mktemp("catalog-loader") / "repository")


@pytest.fixture
def case(tmp_path: Path, baseline: Case) -> Case:
    repository = tmp_path / "repository"
    shutil.copytree(baseline.repository, repository)
    return replace(baseline, repository=repository, root=repository / "authored")


def test_complete_entry_and_explicit_empty(case: Case) -> None:
    assert (
        len(load_adoptions(case.root, entry="catalog/adoptions").current_records()) == 4
    )
    (case.root / "catalog/overrides/routes").mkdir(parents=True)
    assert not load_adoptions(case.root, entry="catalog/overrides").current_records()


@pytest.mark.parametrize("entry", ["catalog/adoptions", "catalog/overrides"])
@pytest.mark.parametrize("typo", [False, True])
def test_entry_requires_known_area(tmp_path: Path, entry: Entry, *, typo: bool) -> None:
    directory = tmp_path / entry
    directory.mkdir(parents=True)
    if typo:
        (directory / "misspelled-area").mkdir()
    with pytest.raises(ValueError, match="at least one known data area"):
        load_adoptions(tmp_path, entry=entry)


@pytest.mark.parametrize(
    "mutation",
    ["duplicate", "symlink", "unknown", "kind", "bool_format", "key", "area"],
)
def test_current_file_boundary_guards(case: Case, mutation: str) -> None:
    path = case.root / "catalog/adoptions/vocabulary/001.yaml"
    raw = object_value(read_yaml(path))
    row = object_value(array(raw["records"])[0])
    message = "Invalid current catalog fields"
    if mutation == "duplicate":
        path.with_name("002.yaml").write_bytes(path.read_bytes())
        message = "Duplicate current catalog selection key"
    elif mutation == "symlink":
        path.rename(path.with_suffix(".bak"))
        path.symlink_to(path.with_suffix(".bak"))
        message = "Symlink|symlink"
    elif mutation == "unknown":
        object_value(row["data"])["unknown"] = True
        write(case.root, path.relative_to(case.root).as_posix(), raw)
    elif mutation == "kind":
        row["kind"] = "unknown_adoption"
        write(case.root, path.relative_to(case.root).as_posix(), raw)
    elif mutation == "bool_format":
        raw["format"] = True
        message = "integer two"
        write(case.root, path.relative_to(case.root).as_posix(), raw)
    else:
        message = "area mismatch"
        if mutation == "key":
            message = "Invalid current catalog fields"
            row["record_key"] = "wrong"
            write(case.root, path.relative_to(case.root).as_posix(), raw)
        else:
            path.rename(case.root / "catalog/adoptions/languages/002.yaml")
    with pytest.raises(ValueError, match=message):
        load_adoptions(case.root, entry="catalog/adoptions")


@pytest.mark.parametrize("group", ["current", "shared"])
def test_nested_shard_directory_is_not_skipped(case: Case, group: str) -> None:
    path = case.root / "catalog/adoptions/vocabulary/001.yaml"
    nested = path.parent / group / path.name
    nested.parent.mkdir()
    path.rename(nested)
    with pytest.raises(ValueError, match=r"^Catalog area must hold shards directly$"):
        load_adoptions(case.root, entry="catalog/adoptions")


def test_unknown_entry(case: Case) -> None:
    with pytest.raises(ValueError, match="Unknown adoption entry"):
        load_adoptions(case.root, entry=cast("Entry", "other"))


def test_unordered_files_and_records_load_current_values(case: Case) -> None:
    path = case.root / "catalog/adoptions/languages/001.yaml"
    raw = object_value(read_yaml(path))
    raw["records"] = list(reversed(array(raw["records"])))
    path.unlink()
    write(case.root, "catalog/adoptions/languages/019.yaml", raw)
    records = load_adoptions(case.root, entry="catalog/adoptions").current_records()
    assert tuple(r.record_key for r in records) == tuple(
        sorted(r.record_key for r in records)
    )


def test_current_withdrawal_and_edit(case: Case) -> None:
    path = "catalog/adoptions/vocabulary/001.yaml"
    raw = object_value(read_yaml(case.root / path))
    row = object_value(array(raw["records"])[0])
    data = object_value(row["data"])
    original = copy.deepcopy(data["value"])
    data["value"] = None
    write(case.root, path, raw)
    assert (
        next(
            r
            for r in load_adoptions(
                case.root, entry="catalog/adoptions"
            ).current_records()
            if r.kind == "vocabulary_adoption"
        ).data.value
        is None
    )
    data["value"] = original
    write(case.root, path, raw)
    assert (
        next(
            r
            for r in load_adoptions(
                case.root, entry="catalog/adoptions"
            ).current_records()
            if r.kind == "vocabulary_adoption"
        ).data.value
        is not None
    )


@pytest.mark.parametrize(
    ("entry", "area", "kind", "subject", "value"),
    [
        (
            "catalog/adoptions",
            "aliases",
            "search_alias_adoption",
            {"kind": "type", "code": "follower", "lang": "ja", "text": "Synthetic"},
            {
                "normalized": "synthetic",
                "normalizer": {"version": "nfkc-casefold-v1", "config": {}},
            },
        ),
        (
            "catalog/adoptions",
            "rules-names",
            "rules_name_adoption",
            {"face_id": "face", "region": "jp", "role": "collab"},
            None,
        ),
        (
            "catalog/adoptions",
            "symbols",
            "text_symbol_adoption",
            {"id": "symbol:synthetic"},
            None,
        ),
        (
            "catalog/overrides",
            "routes",
            "route_override_adoption",
            {"region": "jp", "route_key": "synthetic"},
            None,
        ),
        (
            "catalog/overrides",
            "defaults",
            "default_printing_adoption",
            {"card_id": "card", "region": "jp"},
            {
                "printing_id": "printing",
                "candidates": ["printing"],
                "candidates_hash": "sha256:" + "a" * 64,
            },
        ),
    ],
)
def test_editable_catalog_and_display_kinds(
    case: Case,
    entry: Entry,
    area: str,
    kind: str,
    subject: dict[str, JsonValue],
    value: JsonValue,
) -> None:
    row = record(kind, subject, value)
    write(case.root, f"{entry}/{area}/001.yaml", envelope([row], entry=entry))
    assert any(
        r.kind == kind for r in load_adoptions(case.root, entry=entry).current_records()
    )


def test_offline_cannot_silently_discard_an_editable_alias(case: Case) -> None:
    from sve_carddb.domains.catalog.loader import prepare  # ruff: ignore[import-outside-top-level] -- initialize the shared text interner before the catalog composer

    row = record(
        "search_alias_adoption",
        {"kind": "type", "code": "follower", "lang": "ja", "text": "Synthetic"},
        {
            "normalized": "synthetic",
            "normalizer": {"version": "nfkc-casefold-v1", "config": {}},
        },
    )
    write(case.root, "catalog/adoptions/aliases/001.yaml", envelope([row]))
    with pytest.raises(
        ValueError, match="Offline catalog supports vocabulary and language values"
    ):
        prepare(case.inputs().load(), case.repository, case.build(), {})
