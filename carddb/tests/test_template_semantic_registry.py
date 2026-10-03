"""Finite semantic versions verify immutable Git data and fixed installed code separately."""

import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path

import pytest

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import canonical
from sve_carddb.template_semantics import registry, versions
from sve_carddb.template_semantics.environment import capture
from sve_carddb.template_translations.replay_models import SemanticBinding

from .adoption_fixtures import commit, git


@pytest.fixture(scope="module")
def semantic_repository(tmp_path_factory: pytest.TempPathFactory) -> tuple[Path, str]:
    root = tmp_path_factory.mktemp("semantic-git")
    names = {"carddb/uv.lock", "carddb/pyproject.toml"}
    for identifier in versions.REGISTERED:
        path, data = registry.manifest(identifier)
        names.add(path)
        names.update(f.path for f in data.files)
    for name in names:
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(registry.installed(name))
    git(root, "init")
    revision = commit(root)
    return root, revision


@pytest.mark.parametrize("flavor", [False, True])
def test_exact_version_closure_replays_without_a_matching_host_commit(
    semantic_repository: tuple[Path, str], flavor: bool
) -> None:
    root, producer = semantic_repository
    pins = registry.bindings(PinnedRepository(root), producer, flavor=flavor)
    env = capture(registry.ROOT, producer=True)
    checked = registry.verify(
        PinnedRepository(root), pins, flavor=flavor, environment=env
    )
    assert tuple(b.id for b in checked.bindings) == tuple(
        sorted(
            (
                registry.PARSER_ID,
                registry.FLAVOR_ID if flavor else registry.EFFECT_ID,
                registry.SUMMARY_ID,
            )
        )
    )
    assert checked.exact_pins
    assert {m.entrypoint for m in checked.manifests} <= {
        "physical_parser_v1",
        "template_effect_v1",
        "flavor_exact_v1",
        "semantic_output_v1",
    }


def test_unknown_and_denied_versions_have_distinct_precise_refusals(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with pytest.raises(ValueError, match=r"^Unsupported template semantic version$"):
        registry.manifest("unregistered-v1")
    monkeypatch.setattr(
        registry, "DENIED", ((registry.PARSER_ID, "unsafe_test", "test-fix"),)
    )
    with pytest.raises(
        ValueError, match=r"^Template semantic version is explicitly denied$"
    ):
        registry.manifest(registry.PARSER_ID)


@pytest.mark.parametrize("change", ["missing", "hash", "path", "revision"])
def test_binding_cannot_shrink_rebind_or_borrow_latest(
    semantic_repository: tuple[Path, str], change: str
) -> None:
    root, revision = semantic_repository
    pins = registry.bindings(PinnedRepository(root), revision, flavor=False)
    altered = list(pins)
    expected = "Semantic version manifest differs from its registered binding"
    if change == "missing":
        altered.pop()
        expected = "Template semantic binding closure must be exact"
    else:
        update = {
            change: (
                "sha256:" + "f" * 64
                if change == "hash"
                else "a" * 40
                if change == "revision"
                else "carddb/other.json"
            )
        }
        altered[0] = SemanticBinding.model_validate_json(
            canonical({**altered[0].model_dump(mode="json"), **update})
        )
        if change == "revision":
            expected = "Recognition immutable Git history is unavailable"
    with pytest.raises(ValueError, match="^" + re.escape(expected) + "$"):
        registry.verify(PinnedRepository(root), tuple(altered), flavor=False)


def test_installed_one_byte_change_fails_before_any_output(
    monkeypatch: pytest.MonkeyPatch, semantic_repository: tuple[Path, str]
) -> None:
    root, revision = semantic_repository
    pins = registry.bindings(PinnedRepository(root), revision, flavor=False)
    original = registry.installed
    target = registry.manifest(registry.PARSER_ID)[1].files[0].path
    monkeypatch.setattr(
        registry,
        "installed",
        lambda name: original(name) + (b"# changed\n" if name == target else b""),
    )
    with pytest.raises(
        ValueError, match=r"^Historical semantic implementation exact bytes differ$"
    ):
        registry.verify(PinnedRepository(root), pins, flavor=False)


def test_producer_git_symlink_is_refused_even_when_it_targets_matching_bytes(
    semantic_repository: tuple[Path, str], tmp_path: Path
) -> None:
    import shutil  # ruff: ignore[import-outside-top-level] -- fork the immutable shared Git fixture before changing file mode

    original, _ = semantic_repository
    root = tmp_path / "git-symlink"
    shutil.copytree(original, root)
    name = registry.manifest(registry.PARSER_ID)[1].files[0].path
    target = root / name
    data = target.read_bytes()
    target.unlink()
    backing = root / "same-bytes.txt"
    backing.write_bytes(data)
    target.symlink_to(backing)
    producer = commit(root)
    with pytest.raises(
        ValueError, match=r"^Semantic producer inputs must be regular Git files$"
    ):
        registry.bindings(PinnedRepository(root), producer, flavor=False)


@pytest.mark.parametrize("parent", [False, True])
def test_installed_symlink_and_symlink_parent_are_both_refused(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, parent: bool
) -> None:
    root = tmp_path / "installed"
    root.mkdir()
    backing = tmp_path / "matching"
    backing.mkdir()
    (backing / "value.py").write_bytes(b"# synthetic identical content\n")
    if parent:
        (root / "package").symlink_to(backing, target_is_directory=True)
        name = "package/value.py"
    else:
        (root / "value.py").symlink_to(backing / "value.py")
        name = "value.py"
    monkeypatch.setattr(registry, "ROOT", root)
    with pytest.raises(
        ValueError, match=r"^Installed semantic input must be a regular file$"
    ):
        registry.installed(name)


def test_shallow_clone_cannot_borrow_missing_producer_history(
    semantic_repository: tuple[Path, str], tmp_path: Path
) -> None:
    import shutil  # ruff: ignore[import-outside-top-level] -- only a synthetic local Git repository is cloned

    original, old = semantic_repository
    source = tmp_path / "full-history"
    shutil.copytree(original, source)
    (source / "later.txt").write_text("Synthetic unrelated later commit\n")
    commit(source)
    shallow = tmp_path / "shallow"
    git(tmp_path, "clone", "--depth=1", source.as_uri(), str(shallow))
    assert git(shallow, "rev-parse", "--is-shallow-repository") == "true"
    with pytest.raises(
        ValueError, match=r"^Recognition immutable Git history is unavailable$"
    ):
        registry.bindings(PinnedRepository(shallow), old, flavor=False)


def test_two_explicit_installed_parser_versions_dispatch_independently(
    semantic_repository: tuple[Path, str],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    import shutil  # ruff: ignore[import-outside-top-level] -- immutable synthetic producer is cloned before a new binding is added

    from sve_carddb.snapshot.values import digest  # ruff: ignore[import-outside-top-level] -- test-only binding construction
    from sve_carddb.template_semantics.v1.parser import Parser  # ruff: ignore[import-outside-top-level] -- the test registers an explicit installed subclass

    root, old = semantic_repository
    new_root = tmp_path / "retained-parser"
    shutil.copytree(root, new_root)
    path, data = registry.manifest(registry.PARSER_ID)
    second = "physical-parser-test-v2"
    new_path = path.replace("physical-parser-v1", second)
    raw = canonical({**data.model_dump(mode="json"), "id": second}) + b"\n"
    (new_root / new_path).write_bytes(raw)
    producer = commit(new_root)
    original = registry.installed
    monkeypatch.setattr(
        registry, "installed", lambda name: raw if name == new_path else original(name)
    )
    monkeypatch.setitem(
        versions.REGISTERED,
        second,
        (new_path, digest(canonical({**data.model_dump(mode="json"), "id": second}))),
    )

    class SecondParser(Parser):
        pass

    monkeypatch.setitem(registry.PARSERS, second, SecondParser)
    repository = PinnedRepository(new_root)
    old_pins = registry.bindings(repository, old, flavor=False)
    new_pins = tuple(
        sorted(
            (
                SemanticBinding(
                    id=second,
                    revision=producer,
                    path=new_path,
                    hash=versions.REGISTERED[second][1],
                )
                if b.id == registry.PARSER_ID
                else b
                for b in old_pins
            ),
            key=lambda b: b.id,
        )
    )
    assert type(registry.verify(repository, old_pins, flavor=False).parser) is Parser
    assert (
        type(registry.verify(repository, new_pins, flavor=False).parser) is SecondParser
    )


@pytest.mark.parametrize("change", ["package", "native", "python_file", "version"])
def test_producer_environment_cannot_shrink_or_forge_artifact_evidence(
    semantic_repository: tuple[Path, str], change: str
) -> None:
    from sve_carddb.template_translations.replay_models import ProducerEnvironment  # ruff: ignore[import-outside-top-level] -- validate the tampered closed wire shape

    root, revision = semantic_repository
    repository = PinnedRepository(root)
    bindings = registry.bindings(repository, revision, flavor=False)
    data = capture(registry.ROOT, producer=True).model_dump(mode="json")
    expected = "Semantic environment must contain its exact necessary packages"
    if change == "package":
        data["packages"].pop()
    elif change == "native":
        data["packages"][1]["artifacts"] = [
            a for a in data["packages"][1]["artifacts"] if a["path"].endswith(".py")
        ]
        expected = "Semantic environment omits necessary Python or native artifacts"
    elif change == "python_file":
        data["packages"][0]["artifacts"].pop()
        expected = "Semantic producer Python artifact closure must be exact"
    else:
        data["packages"][0]["version"] = "0.0.1"
        expected = "Semantic producer package version differs from fixed lock evidence"
    env = ProducerEnvironment.model_validate_json(canonical(data))
    with pytest.raises(ValueError, match="^" + re.escape(expected) + "$"):
        registry.verify(repository, bindings, flavor=False, environment=env)
