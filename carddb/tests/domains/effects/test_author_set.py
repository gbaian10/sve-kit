"""File inventories reject omitted records without changing prototype inputs."""

from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical
from sve_carddb.core.provenance import BuildContext
from sve_carddb.domains.effects.author_current import CardRejected, CurrentAuthorSources
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas
from sve_carddb.domains.effects.author_set import load_author_set, validate_author_set
from sve_carddb.domains.text_observations import Vocabulary
from sve_carddb.domains.translations.inputs import Snapshot
from sve_carddb.domains.translations.recognition.rules import load
from sve_carddb.domains.translations.sources import Sources

from .author_entry_cases import meta

if TYPE_CHECKING:
    from pydantic import JsonValue

    from ...support.database_template import DatabaseTemplate

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
    write(
        root,
        AUTHOR,
        {
            "format": 2,
            "kind": "author_effect_set",
            "scope": scope,
            "cards": {
                key: {
                    "meta": {"written_by": meta()["written_by"]},
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
                    "written_by": meta()["written_by"],
                }
            ],
        },
    )


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
        current = CurrentAuthorSources(
            db,
            Sources({}, REPO, BuildContext.from_inputs("0" * 40, {})),
            Snapshot((), ()),
            Vocabulary(bindings=()),
            load(REPO),
            "sha256:" + "0" * 64,
        )
        prototype(tmp_path, "FIX-001")
        index(tmp_path, selection(path=PROTOTYPE))
        result = current.validate(tmp_path, expander)
        assert len(result) == 1
        assert isinstance(result[0], CardRejected)
        assert result[0].diagnostic.reason == "unsupported_prototype"
        index(tmp_path, selection(path=PROTOTYPE, identity="2"))
        with pytest.raises(AuthorInputError, match="prototype_index_identity_mismatch"):
            current.validate(tmp_path, expander)
        index(tmp_path)
        prototype(tmp_path, "MISSING")
        with pytest.raises(AuthorInputError, match="prototype_identity_unresolved"):
            current.validate(tmp_path, expander)
        prototype(tmp_path, "FIX-001")
        for scope in ("shared", "en_override"):
            author(tmp_path, "FIX-001", scope=scope)
            index(tmp_path, selection(scope=scope))
            if scope == "shared":
                with pytest.raises(AuthorInputError, match="prototype_author_overlap"):
                    current.validate(tmp_path, expander)
            else:
                with pytest.raises(AuthorInputError, match="schema/const"):
                    current.validate(tmp_path, expander)


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


@pytest.mark.parametrize("component", ["rules", "rules/effects"])
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
