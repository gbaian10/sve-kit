"""Single-gate synthetic counterexamples for immutable semantic registry evidence."""

import json
import shutil
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_semantics import registry, versions
from sve_carddb.template_semantics.environment import capture
from sve_carddb.template_sources.models import Recipe
from sve_carddb.template_translations.replay_models import (
    SemanticBinding,
    SemanticManifest,
)

from .adoption_fixtures import commit
from .test_template_semantic_registry import semantic_repository

if TYPE_CHECKING:
    from pathlib import Path

__all__ = ("semantic_repository",)


def declared(producer: str) -> tuple[SemanticBinding, ...]:
    return tuple(
        SemanticBinding(id=name, revision=producer, path=path, hash=checksum)
        for name, (path, checksum) in sorted(versions.REGISTERED.items())
        if name != registry.FLAVOR_ID
    )


def fork(original: Path, tmp_path: Path) -> Path:
    root = tmp_path / "synthetic-git"
    shutil.copytree(original, root)
    return root


def repin_manifests(
    root: Path, monkeypatch: pytest.MonkeyPatch, *, omit_package: bool = False
) -> None:
    """Synthetic registered metadata lets later guards receive valid upstream evidence."""
    for name, (path, _) in tuple(versions.REGISTERED.items()):
        data = SemanticManifest.model_validate_json((root / path).read_bytes())
        data = data.model_copy(
            update={
                "files": tuple(
                    f.model_copy(update={"hash": digest((root / f.path).read_bytes())})
                    for f in data.files
                ),
                "environment_packages": tuple(
                    p
                    for p in data.environment_packages
                    if not omit_package or p != "selectolax"
                ),
            }
        )
        raw = canonical(data.model_dump(mode="json"))
        (root / path).write_bytes(raw)
        monkeypatch.setitem(versions.REGISTERED, name, (path, digest(raw)))
    monkeypatch.setattr(registry, "ROOT", root)


def test_manifest_content_cannot_change_under_the_same_registered_id(
    semantic_repository: tuple[Path, str],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    original, _ = semantic_repository
    root = fork(original, tmp_path)
    path, _ = versions.REGISTERED[registry.PARSER_ID]
    data = SemanticManifest.model_validate_json((root / path).read_bytes())
    (root / path).write_bytes(
        canonical(
            data.model_copy(update={"entrypoint": "different_parser_v1"}).model_dump(
                mode="json"
            )
        )
    )
    monkeypatch.setattr(registry, "ROOT", root)
    with pytest.raises(
        ValueError,
        match=r"^Semantic version manifest differs from its registered binding$",
    ):
        registry.manifest(registry.PARSER_ID)


def test_matching_producer_and_installed_change_still_fails_declared_file_hash(
    semantic_repository: tuple[Path, str],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    original, _ = semantic_repository
    root = fork(original, tmp_path)
    target = registry.manifest(registry.PARSER_ID)[1].files[0].path
    with (root / target).open("ab") as stream:
        stream.write(b"\n# synthetic matching change on both sides\n")
    producer = commit(root)
    monkeypatch.setattr(registry, "ROOT", root)
    with pytest.raises(
        ValueError, match=r"^Historical semantic implementation exact bytes differ$"
    ):
        registry.verify(PinnedRepository(root), declared(producer), flavor=False)


def test_canonical_equivalent_manifest_requires_identical_exact_bytes(
    semantic_repository: tuple[Path, str],
    tmp_path: Path,
) -> None:
    original, _ = semantic_repository
    root = fork(original, tmp_path)
    path, _ = versions.REGISTERED[registry.PARSER_ID]
    data = json.loads((root / path).read_bytes())
    altered = json.dumps(data, indent=4).encode()
    assert digest(canonical(data)) == versions.REGISTERED[registry.PARSER_ID][1]
    assert altered != registry.installed(path)
    (root / path).write_bytes(altered)
    producer = commit(root)
    with pytest.raises(
        ValueError, match=r"^Historical semantic manifest exact bytes differ$"
    ):
        registry.verify(PinnedRepository(root), declared(producer), flavor=False)


def test_missing_git_file_is_refused_at_dependency_closure_gate(
    semantic_repository: tuple[Path, str],
) -> None:
    root, producer = semantic_repository
    with pytest.raises(
        ValueError, match=r"^Semantic producer dependency closure is unavailable$"
    ):
        registry.regular(
            PinnedRepository(root), producer, ("carddb/uv.lock", "missing.py")
        )


@pytest.mark.parametrize("change", ["entrypoint", "frozen_hash"])
def test_recipe_cannot_escape_the_registered_calculation_closure(
    semantic_repository: tuple[Path, str],
    change: str,
) -> None:
    root, producer = semantic_repository
    repository = PinnedRepository(root)
    checked = registry.verify(repository, declared(producer), flavor=False)
    identifier = "translation-jp-v1"
    path = versions.RECIPES[identifier]
    checksum = digest(registry.installed(path))
    expected = r"^Template recipe code is absent from its semantic closure$"
    if change == "entrypoint":
        path = "carddb/src/sve_carddb/template_translations/semantic_replay.py"
        expected = r"^Unsupported fixed template recipe entrypoint$"
    else:
        checksum = "sha256:" + "f" * 64
    pin = Recipe(
        id=identifier,
        code_revision=producer,
        code_path=path,
        code_hash=checksum,
        config={},
        config_hash=digest(canonical({})),
    )
    with pytest.raises(ValueError, match=expected):
        registry.verify_recipes(repository, (pin,), checked)


def test_manifest_package_union_cannot_omit_a_necessary_package(
    semantic_repository: tuple[Path, str],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    original, _ = semantic_repository
    root = fork(original, tmp_path)
    repin_manifests(root, monkeypatch, omit_package=True)
    producer = commit(root)
    with pytest.raises(
        ValueError, match=r"^Semantic manifest package closure must be exact$"
    ):
        registry.verify(PinnedRepository(root), declared(producer), flavor=False)


@pytest.mark.parametrize("field", ["uv_lock_hash", "pyproject_hash"])
def test_producer_lock_hashes_are_checked_against_git_not_claims(
    semantic_repository: tuple[Path, str],
    field: str,
) -> None:
    root, producer = semantic_repository
    env = capture(registry.ROOT, producer=True).model_copy(
        update={field: "sha256:" + "f" * 64}
    )
    with pytest.raises(
        ValueError, match=r"^Semantic producer environment lock evidence differs$"
    ):
        registry.verify_environment(PinnedRepository(root), declared(producer), env)


def test_valid_byte_and_hash_pins_do_not_permit_a_mutable_import(
    semantic_repository: tuple[Path, str],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    original, _ = semantic_repository
    root = fork(original, tmp_path)
    target = "carddb/src/sve_carddb/template_semantics/v1/html.py"
    with (root / target).open("ab") as stream:
        stream.write(
            b"\nfrom sve_carddb.template_translations.semantic_replay import produce\n"
        )
    repin_manifests(root, monkeypatch)
    producer = commit(root)
    with pytest.raises(
        ValueError, match=r"^Frozen semantic import crosses an undeclared boundary$"
    ):
        registry.verify(PinnedRepository(root), declared(producer), flavor=False)
