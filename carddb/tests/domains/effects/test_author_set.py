"""File inventories reject omitted records without changing prototype inputs."""

import os
from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical, object_value
from sve_carddb.core.yaml import MAX_BYTES
from sve_carddb.domains.effects.author_expansion import ExpansionError, body_version
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas
from sve_carddb.domains.effects.author_set import load_author_set, validate_author_set

from .author_entry_cases import meta, source_use

if TYPE_CHECKING:
    from pydantic import JsonValue

REPO = Path(__file__).resolve().parents[4]
AUTHOR = "rules/author-effects/FIX/001.yaml"
BODY = "rules/effect-bodies/FIX/001.yaml"
PROTOTYPE = "rules/effects/FIX.yaml"
INDEX = "rules/effects-index.yaml"


@pytest.fixture(scope="module")
def schemas() -> AuthorSchemas:
    return AuthorSchemas(REPO / "dsl")


def write(root: Path, path: str, value: JsonValue) -> Path:
    target = root / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(canonical(value))
    return target


def selection(
    key: str = "FIX-001",
    *,
    path: str = AUTHOR,
    scope: str = "shared",
    identity: str = "1",
) -> dict[str, JsonValue]:
    return {
        "card_id": "c:" + identity * 32,
        "scope": scope,
        "file": path,
        "kind": "effect_set" if path == PROTOTYPE else "author_effect_set",
        "record_key": key,
    }


def index(root: Path, *records: dict[str, JsonValue]) -> None:
    write(root, INDEX, {"format": 1, "kind": "effects_index", "records": list(records)})


def author(root: Path, *keys: str, scope: str = "shared") -> None:
    card: JsonValue = {
        "meta": meta(),
        "abilities": [
            {
                "line": 1,
                "sources": [source_use()],
                "ports": {"imports": {}, "exports": {}},
                "evolve": {"cost": {"pp": 1}},
            }
        ],
    }
    write(
        root,
        AUTHOR,
        {
            "format": 1,
            "kind": "author_effect_set",
            "scope": scope,
            "cards": dict.fromkeys(keys, card),
        },
    )


def prototype(root: Path, *keys: str) -> None:
    write(
        root,
        PROTOTYPE,
        {
            "format": 1,
            "kind": "effect_set",
            "cards": {key: {"status": "unsupported", "abilities": []} for key in keys},
        },
    )


def companion(root: Path, path: str = BODY) -> None:
    record: dict[str, JsonValue] = {
        "frame_id": "frame:" + "1" * 64,
        "ability_scope": "ability_0",
        "body_version": "",
        "dsl": "1.0",
        "params": {},
        "ports": {"imports": {}, "exports": {}},
        "body": {"evolve": {"cost": {"pp": 1}}},
        "written_by": meta()["written_by"],
        "registrations": [],
    }
    record["body_version"] = body_version(record)
    write(
        root,
        path,
        {
            "format": 1,
            "kind": "effect_body_shard",
            "scope": "shared",
            "records": [record],
        },
    )


def test_existing_authored_tree_without_index(schemas: AuthorSchemas) -> None:
    inputs = load_author_set(REPO / "authored", schemas)
    assert inputs.index_file is None
    assert inputs.index.records == ()
    assert inputs.author_files == inputs.body_files == ()
    assert len(inputs.prototype_files) == 47
    assert (
        sum(
            len(object_value(object_value(schemas.prototype_tree(item.raw))["cards"]))
            for item in inputs.prototype_files
        )
        == 422
    )


def test_complete_inventory_retains_bytes_and_unused_bodies(
    tmp_path: Path, schemas: AuthorSchemas
) -> None:
    index(tmp_path, selection(), selection("FIX-002", identity="2"))
    author(tmp_path, "FIX-001", "FIX-002")
    companion(tmp_path)
    prototype(tmp_path, "OLD-001")
    inputs = load_author_set(tmp_path, schemas)
    assert inputs.author_files[0].raw == (tmp_path / AUTHOR).read_bytes()
    assert inputs.body_files[0].path == BODY
    assert inputs.prototype_files[0].path == PROTOTYPE
    assert "written_by" not in repr(inputs)
    validate_author_set(inputs, schemas)


@pytest.mark.parametrize("area", ["rules/author-effects", "rules/effect-bodies"])
def test_missing_index_with_declared_area(
    tmp_path: Path, schemas: AuthorSchemas, area: str
) -> None:
    (tmp_path / area).mkdir(parents=True)
    with pytest.raises(AuthorInputError, match="index_missing"):
        load_author_set(tmp_path, schemas)


def test_empty_set_has_no_index_requirement(
    tmp_path: Path, schemas: AuthorSchemas
) -> None:
    assert load_author_set(tmp_path, schemas).index.records == ()


def test_rules_parent_must_be_directory(tmp_path: Path, schemas: AuthorSchemas) -> None:
    (tmp_path / "rules").write_bytes(b"not a directory")
    with pytest.raises(AuthorInputError, match="unsafe_or_missing_directory"):
        load_author_set(tmp_path, schemas)


@pytest.mark.parametrize("has_index", [False, True])
@pytest.mark.parametrize("version", [True, 1.0, 99])
def test_unchecked_index_object_cannot_bypass_schema(
    tmp_path: Path, schemas: AuthorSchemas, has_index: bool, version: object
) -> None:
    if has_index:
        index(tmp_path)
    inputs = load_author_set(tmp_path, schemas)
    forged = replace(inputs, index=inputs.index.model_copy(update={"format": version}))
    with pytest.raises(AuthorInputError):
        validate_author_set(forged, schemas)


@pytest.mark.parametrize(
    ("keys", "selected", "reason"),
    [
        (("FIX-001",), (), "orphan_author_file"),
        (("FIX-001", "FIX-002"), ("FIX-001",), "index_record_closure"),
        (("FIX-001",), ("FIX-002",), "index_record_closure"),
    ],
)
def test_two_way_record_closure(
    tmp_path: Path,
    schemas: AuthorSchemas,
    keys: tuple[str, ...],
    selected: tuple[str, ...],
    reason: str,
) -> None:
    index(tmp_path, *(selection(key) for key in selected))
    author(tmp_path, *keys)
    with pytest.raises(AuthorInputError, match=reason):
        load_author_set(tmp_path, schemas)


def test_duplicate_selection_cannot_claim_another_identity(
    tmp_path: Path, schemas: AuthorSchemas
) -> None:
    index(tmp_path, selection(), selection(identity="2"))
    author(tmp_path, "FIX-001")
    with pytest.raises(AuthorInputError, match="duplicate_record_selection"):
        load_author_set(tmp_path, schemas)


def test_selected_prototype_requires_its_whole_file(
    tmp_path: Path, schemas: AuthorSchemas
) -> None:
    prototype(tmp_path, "FIX-001", "FIX-002")
    assert len(load_author_set(tmp_path, schemas).prototype_files) == 1
    index(tmp_path, selection(path=PROTOTYPE))
    with pytest.raises(AuthorInputError, match="index_record_closure"):
        load_author_set(tmp_path, schemas)
    index(
        tmp_path,
        selection(path=PROTOTYPE),
        selection("FIX-002", path=PROTOTYPE, identity="2"),
    )
    assert len(load_author_set(tmp_path, schemas).index.records) == 2


def test_missing_index_target_is_located(
    tmp_path: Path, schemas: AuthorSchemas
) -> None:
    index(tmp_path, selection())
    with pytest.raises(AuthorInputError, match="index_file_missing") as caught:
        load_author_set(tmp_path, schemas)
    assert caught.value.diagnostic.card_id == "c:" + "1" * 32


@pytest.mark.parametrize(
    "path",
    [
        "rules/author-effects/FIX/1.yaml",
        "rules/author-effects/FIX/001.json",
        "rules/author-effects/.hidden",
        "rules/effects/FIX/001.yaml",
        "rules/effects/FIX.json",
        "rules/effect-bodies/FIX/extra/001.yaml",
    ],
)
def test_unlisted_bad_paths_are_not_skipped(
    tmp_path: Path, schemas: AuthorSchemas, path: str
) -> None:
    index(tmp_path)
    write(tmp_path, path, {})
    with pytest.raises(AuthorInputError, match="invalid_input_path"):
        load_author_set(tmp_path, schemas)


@pytest.mark.parametrize("component", ["rules", "rules/effects", PROTOTYPE])
def test_symlinks_are_rejected_before_reading(
    tmp_path: Path, schemas: AuthorSchemas, component: str
) -> None:
    root = tmp_path / "authored"
    root.mkdir()
    external = tmp_path / "external"
    external.mkdir()
    target = root / component
    target.parent.mkdir(parents=True, exist_ok=True)
    target.symlink_to(external, target_is_directory=True)
    with pytest.raises(AuthorInputError, match="unsafe_or_missing"):
        load_author_set(root, schemas)


def test_nonregular_file_does_not_block(tmp_path: Path, schemas: AuthorSchemas) -> None:
    target = tmp_path / PROTOTYPE
    target.parent.mkdir(parents=True)
    os.mkfifo(target)
    with pytest.raises(AuthorInputError, match="nonregular_input"):
        load_author_set(tmp_path, schemas)


def test_size_bound_applies_before_yaml(tmp_path: Path, schemas: AuthorSchemas) -> None:
    target = write(tmp_path, PROTOTYPE, {})
    target.write_bytes(b"x" * MAX_BYTES)
    with pytest.raises(AuthorInputError, match="size_limit"):
        load_author_set(tmp_path, schemas)


def test_en_inventory_is_closed_but_never_decoded(
    tmp_path: Path, schemas: AuthorSchemas
) -> None:
    index(tmp_path, selection(scope="en_override"))
    write(
        tmp_path,
        AUTHOR,
        {
            "format": 1,
            "kind": "author_effect_set",
            "scope": "en_override",
            "cards": {"FIX-001": {"future_en_content": True}},
        },
    )
    inputs = load_author_set(tmp_path, schemas)
    assert inputs.index.records[0].scope == "en_override"
    with pytest.raises(AuthorInputError, match="en_source_contract_unavailable"):
        schemas.author_effect_set(inputs.author_files[0].raw)
    index(tmp_path, selection(scope="shared"))
    with pytest.raises(AuthorInputError, match="index_envelope_mismatch"):
        load_author_set(tmp_path, schemas)


def test_companion_keys_are_global(tmp_path: Path, schemas: AuthorSchemas) -> None:
    index(tmp_path)
    companion(tmp_path)
    companion(tmp_path, "rules/effect-bodies/OTHER/999.yaml")
    with pytest.raises(ExpansionError, match="duplicate_body"):
        load_author_set(tmp_path, schemas)


def test_input_objects_are_revalidated(tmp_path: Path, schemas: AuthorSchemas) -> None:
    index(tmp_path, selection())
    author(tmp_path, "FIX-001")
    inputs = load_author_set(tmp_path, schemas)
    forged = replace(inputs.author_files[0], raw=b"{}")
    with pytest.raises(AuthorInputError, match="input_hash_mismatch"):
        validate_author_set(replace(inputs, author_files=(forged,)), schemas)
    with pytest.raises(AuthorInputError, match="index_file_missing"):
        validate_author_set(replace(inputs, author_files=()), schemas)
    with pytest.raises(AuthorInputError, match="duplicate_input_file"):
        validate_author_set(
            replace(inputs, author_files=inputs.author_files * 2), schemas
        )
    with pytest.raises(AuthorInputError, match="index_missing"):
        validate_author_set(replace(inputs, index_file=None), schemas)


def test_invalid_unselected_prototype_is_not_ignored(
    tmp_path: Path, schemas: AuthorSchemas
) -> None:
    write(
        tmp_path,
        PROTOTYPE,
        {"format": 1, "kind": "effect_set", "cards": {"FIX-001": {}}},
    )
    with pytest.raises(AuthorInputError):
        load_author_set(tmp_path, schemas)
