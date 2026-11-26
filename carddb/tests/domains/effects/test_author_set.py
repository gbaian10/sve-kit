"""File inventories reject omitted records without changing prototype inputs."""

from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.yaml import MAX_BYTES
from sve_carddb.domains.effects.author_current import CardRejected, CurrentAuthorSources
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_handwritten import read_files
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas
from sve_carddb.domains.effects.author_set import (
    InputFile,
    load_author_set,
    merge_index_files,
    validate_author_set,
)
from sve_carddb.domains.mechanics import load as load_keywords

from .author_entry_cases import meta, source_use

if TYPE_CHECKING:
    from pydantic import JsonValue

    from ...support.database_template import DatabaseTemplate

REPO = Path(__file__).resolve().parents[4]
AUTHOR = "rules/author-effects/FIX/001.yaml"
BODY = "rules/effect-bodies/FIX/001.yaml"
PROTOTYPE = "rules/effects/FIX.yaml"
INDEX = "rules/effects-index/FIX.yaml"
ABILITY_FILTERS = frozenset(
    {"earth_rite", "fanfare", "lastword", "quick_activation", "quick_spell"}
)


def _keyword_codes(node: JsonValue) -> set[str]:
    codes: set[str] = set()
    if isinstance(node, dict):
        keywords = node.get("keywords")
        if isinstance(keywords, dict):
            keywords = keywords.get("list")
        if isinstance(keywords, list):
            codes.update(value for value in keywords if isinstance(value, str))
        keyword = node.get("keyword")
        if isinstance(keyword, str) and keyword not in ABILITY_FILTERS:
            codes.add(keyword)
        for item in node.values():
            codes.update(_keyword_codes(item))
    elif isinstance(node, list):
        for item in node:
            codes.update(_keyword_codes(item))
    return codes


def test_repository_author_keywords_are_registered() -> None:
    authored = REPO / "authored"
    registered = {record.code for record in load_keywords(authored).registry.records}
    used: set[str] = set()
    for directory, main in (("author-effects", True), ("effect-bodies", False)):
        for _, document in read_files(authored / "rules" / directory, main=main):
            used.update(_keyword_codes(document.value))
    assert used
    assert not (used - registered)


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
    write(
        root,
        AUTHOR,
        {
            "format": 2,
            "kind": "author_effect_set",
            "scope": scope,
            "cards": {
                key: {
                    "abilities": [{"line": 1, "evolve": {"cost": {"pp": 1}}}],
                }
                for key in keys
            },
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
    write(
        root,
        path,
        {
            "format": 2,
            "kind": "effect_body_shard",
            "records": [
                {
                    "id": "synthetic",
                    "body": {"evolve": {"cost": {"pp": 1}}},
                    "written_by": [
                        {
                            "actor": {"who": "synthetic author", "model": None},
                            "date": "2026-10-21",
                        }
                    ],
                }
            ],
        },
    )


@pytest.mark.parametrize("path", [AUTHOR, BODY])
def test_disk_loader_rejects_legacy_storage(
    tmp_path: Path, schemas: AuthorSchemas, path: str
) -> None:
    author(tmp_path, "FIX-001")
    index(tmp_path, selection())
    if path == AUTHOR:
        document: JsonValue = {
            "format": 1,
            "kind": "author_effect_set",
            "scope": "shared",
            "cards": {
                "FIX-001": {
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
            },
        }
    else:
        document = {
            "format": 1,
            "kind": "effect_body_shard",
            "scope": "shared",
            "records": [],
        }
    write(tmp_path, path, document)
    with pytest.raises(AuthorInputError) as error:
        load_author_set(tmp_path, schemas)
    assert error.value.diagnostic.reason == "unsupported_storage_format"
    assert error.value.diagnostic.path == (path,)


def test_prototype_inventory_uses_database_identity(
    tmp_path: Path, t0_database_template: DatabaseTemplate
) -> None:
    expander = AuthorExpander(REPO / "dsl")
    with t0_database_template.copy() as db:
        with db.transaction():
            db.insert("card", dict(db.rows("card")[0].values) | {"id": "c:" + "1" * 32})
            db.insert(
                "printing",
                dict(db.rows("printing")[0].values)
                | {"id": "prototype", "card_no": "FIX-001", "card_id": "c:" + "1" * 32},
            )
        current = CurrentAuthorSources(db)
        prototype(tmp_path, "FIX-001")
        index(tmp_path, selection(path=PROTOTYPE))
        result = current.validate_inputs(
            load_author_set(tmp_path, expander.schemas), expander
        )
        assert len(result) == 1
        assert isinstance(result[0], CardRejected)
        assert result[0].diagnostic.reason == "unsupported_prototype"
        index(tmp_path, selection(path=PROTOTYPE, identity="2"))
        with pytest.raises(AuthorInputError, match="prototype_index_identity_mismatch"):
            current.validate_inputs(
                load_author_set(tmp_path, expander.schemas), expander
            )
        index(tmp_path)
        prototype(tmp_path, "MISSING")
        with pytest.raises(AuthorInputError, match="prototype_identity_unresolved"):
            current.validate_inputs(
                load_author_set(tmp_path, expander.schemas), expander
            )
        prototype(tmp_path, "FIX-001")
        author(tmp_path, "FIX-001")
        index(tmp_path, selection())
        with pytest.raises(AuthorInputError, match="prototype_author_overlap"):
            current.validate_inputs(
                load_author_set(tmp_path, expander.schemas), expander
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


@pytest.mark.parametrize(
    "path",
    [
        "rules/author-effects/.hidden",
        "rules/effects-index/FIX/001.yaml",
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


@pytest.mark.parametrize("component", ["rules", "rules/effects", "rules/effects-index"])
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


def test_pack_indexes_merge_and_retain_exact_bytes(
    tmp_path: Path, schemas: AuthorSchemas
) -> None:
    prototype(tmp_path, "FIX-001", "OLD-001")
    index(tmp_path, selection(path=PROTOTYPE))
    other = selection("OLD-001", path=PROTOTYPE, identity="2")
    write(
        tmp_path,
        "rules/effects-index/OLD.yaml",
        {
            "format": 1,
            "kind": "effects_index",
            "records": [other],
        },
    )
    inputs = load_author_set(tmp_path, schemas)
    assert [record.record_key for record in inputs.index.records] == [
        "FIX-001",
        "OLD-001",
    ]
    assert len(inputs.index_files) == 2
    assert all(
        item.raw == (tmp_path / item.path).read_bytes() for item in inputs.index_files
    )
    with pytest.raises(AuthorInputError, match="index_input_mismatch"):
        validate_author_set(
            replace(inputs, index_files=inputs.index_files[:1]), schemas
        )
    forged = replace(inputs.index_files[0], raw=b"{}")
    with pytest.raises(AuthorInputError, match="input_hash_mismatch"):
        validate_author_set(
            replace(inputs, index_files=(forged, *inputs.index_files[1:])), schemas
        )


def test_pack_indexes_can_merge_beyond_one_mib(schemas: AuthorSchemas) -> None:
    header: dict[str, JsonValue] = {"format": 1, "kind": "effects_index"}
    all_records: list[JsonValue] = []
    files: list[InputFile] = []
    for pack_number in range(8):
        owner = f"PACK{pack_number:02d}"
        rows: list[JsonValue] = [
            selection(
                f"{owner}-{number:03d}", path=f"rules/author-effects/{owner}/001.yaml"
            )
            | {"card_id": f"c:{pack_number * 900 + number:032x}"}
            for number in range(900)
        ]
        raw = canonical({**header, "records": rows})
        assert len(raw) < MAX_BYTES
        files.append(InputFile(f"rules/effects-index/{owner}.yaml", raw, digest(raw)))
        all_records.extend(rows)
    assert len(canonical({**header, "records": all_records})) > MAX_BYTES

    merged = merge_index_files(tuple(files), schemas)
    assert len(merged.records) == len(all_records)


@pytest.mark.parametrize("failure", ["owner", "identity", "order"])
def test_pack_indexes_reject_wrong_owner_global_duplicates_and_unsorted_rows(
    tmp_path: Path, schemas: AuthorSchemas, failure: str
) -> None:
    prototype(tmp_path, "FIX-001", "OLD-001")
    index(tmp_path, selection(path=PROTOTYPE))
    rows = [selection("OLD-001", path=PROTOTYPE, identity="2")]
    if failure == "owner":
        rows[0]["record_key"] = "OTHER-001"
    elif failure == "identity":
        rows[0]["card_id"] = selection()["card_id"]
    else:
        rows.insert(0, selection("OLD-002", path=PROTOTYPE, identity="3"))
    write(
        tmp_path,
        "rules/effects-index/OLD.yaml",
        {
            "format": 1,
            "kind": "effects_index",
            "records": [*rows],
        },
    )
    reason = {
        "owner": "wrong_index_owner",
        "identity": "duplicate_card_scope",
        "order": "records_not_sorted",
    }[failure]
    with pytest.raises(AuthorInputError, match=reason):
        load_author_set(tmp_path, schemas)


@pytest.mark.parametrize("number", ["FIX-001", "FIX-001Ⓢ"])
def test_content_named_author_shard_is_admitted(
    tmp_path: Path, schemas: AuthorSchemas, number: str
) -> None:
    author(tmp_path, number)
    path = f"rules/author-effects/FIX/{number}.yaml"
    (tmp_path / AUTHOR).rename(tmp_path / path)
    index(tmp_path, selection(number, path=path))
    assert load_author_set(tmp_path, schemas).author_files[0].path == path
