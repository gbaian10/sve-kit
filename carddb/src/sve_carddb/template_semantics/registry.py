"""Finite immutable semantic bindings; producer Git bytes are data, never executed."""

import ast
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.template_parameter_rules.repository import git, revision
from sve_carddb.template_semantics import versions
from sve_carddb.template_semantics.environment import PACKAGES, validate
from sve_carddb.template_semantics.v1.parser import Parser
from sve_carddb.template_translations.replay_models import (
    FlavorReplayInputs,
    SemanticBinding,
    SemanticManifest,
)

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_sources import PinnedRepository
    from sve_carddb.template_sources.models import Recipe
    from sve_carddb.template_translations.replay_models import (
        ProducerEnvironment,
        ReplayContext,
    )

ROOT = Path(__file__).resolve().parents[4]
# Each future refusal must name the version, reason code and reviewed fix reference.
DENIED: tuple[tuple[str, str, str], ...] = ()
PARSER_ID = "physical-parser-v1"
EFFECT_ID = "template-effect-v1"
FLAVOR_ID = "template-flavor-v1"
SUMMARY_ID = "semantic-output-v1"

# IDs select already installed implementations; Git content never provides executable code.
PARSERS: dict[str, type[Parser]] = {PARSER_ID: Parser}
EFFECTS = {EFFECT_ID: "template_effect_v1"}
FLAVORS = {FLAVOR_ID: "flavor_exact_v1"}
SUMMARIES = {SUMMARY_ID: "semantic_output_v1"}


def closure(declared: tuple[SemanticBinding, ...], *, flavor: bool) -> None:
    """Exactly one explicitly supported implementation of each necessary role is required."""
    ids = tuple(b.id for b in declared)
    roles = (PARSERS, FLAVORS if flavor else EFFECTS, SUMMARIES)
    if (
        len(ids) != len(roles)
        or ids != tuple(sorted(set(ids)))
        or any(sum(identifier in role for identifier in ids) != 1 for role in roles)
    ):
        raise ValueError("Template semantic binding closure must be exact")


@dataclass(frozen=True)
class Checked:
    bindings: tuple[SemanticBinding, ...]
    manifests: tuple[SemanticManifest, ...]
    exact_pins: tuple[tuple[str, str, str], ...]
    parser: Parser


def regular(
    repository: PinnedRepository, commit: str, names: tuple[str, ...]
) -> dict[str, bytes]:
    """Reject modes and filesystem links independently of matching blob content."""
    revision(repository, commit)
    rows = (
        git(
            repository,
            "ls-tree",
            "-r",
            "--format=%(objectmode)%x09%(objecttype)%x09%(path)",
            commit,
            "--",
            *names,
        )
        .decode()
        .splitlines()
    )
    actual = {}
    for row in rows:
        mode, kind, name = row.split("\t", 2)
        if mode not in {"100644", "100755"} or kind != "blob":
            raise ValueError("Semantic producer inputs must be regular Git files")
        actual[name] = mode
    if set(actual) != set(names):
        raise ValueError("Semantic producer dependency closure is unavailable")
    return repository.read_many(commit, names)


def installed(name: str) -> bytes:
    """Do not follow symlinked parent directories into another implementation."""
    path = ROOT / name
    if any(p.is_symlink() for p in (path, *path.parents)) or not path.is_file():
        raise ValueError("Installed semantic input must be a regular file")
    return path.read_bytes()


def manifest(identifier: str) -> tuple[str, SemanticManifest]:
    """Only the code-owned append-only mapping selects path, hash and entrypoint."""
    if identifier not in versions.REGISTERED:
        raise ValueError("Unsupported template semantic version")
    if any(row[0] == identifier for row in DENIED):
        raise ValueError("Template semantic version is explicitly denied")
    path, expected = versions.REGISTERED[identifier]
    data = SemanticManifest.model_validate_json(installed(path))
    if (
        data.id != identifier
        or digest(canonical(data.model_dump(mode="json"))) != expected
    ):
        raise ValueError(
            "Semantic version manifest differs from its registered binding"
        )
    return path, data


def bindings(
    repository: PinnedRepository, producer: str, *, flavor: bool
) -> tuple[SemanticBinding, ...]:
    """Generation validates exactly the same fixed closure as historical consumption."""
    result = tuple(
        SemanticBinding(
            id=name,
            revision=producer,
            path=manifest(name)[0],
            hash=versions.REGISTERED[name][1],
        )
        for name in sorted((PARSER_ID, FLAVOR_ID if flavor else EFFECT_ID, SUMMARY_ID))
    )
    verify(repository, result, flavor=flavor)
    return result


def verify(  # ruff: ignore[complex-structure] -- each fixed closure and entrypoint is checked before constructing a dispatcher
    repository: PinnedRepository,
    declared: tuple[SemanticBinding, ...],
    *,
    flavor: bool,
    environment: ProducerEnvironment | None = None,
) -> Checked:
    """No latest dispatch or whole-host-runtime equality participates in this gate."""
    closure(declared, flavor=flavor)
    manifests: list[SemanticManifest] = []
    exact: list[tuple[str, str, str]] = []
    for binding in declared:
        path, data = manifest(binding.id)
        if (binding.path, binding.hash) != (path, versions.REGISTERED[binding.id][1]):
            raise ValueError(
                "Semantic version manifest differs from its registered binding"
            )
        names = tuple(sorted({path, *(f.path for f in data.files)}))
        raw = regular(repository, binding.revision, names)
        if raw[path] != installed(path):
            raise ValueError("Historical semantic manifest exact bytes differ")
        for item in data.files:
            if (
                digest(raw[item.path]) != item.hash
                or installed(item.path) != raw[item.path]
            ):
                raise ValueError(
                    "Historical semantic implementation exact bytes differ"
                )
        exact.extend((binding.revision, name, digest(raw[name])) for name in names)
        manifests.append(data)
    if {p for m in manifests for p in m.environment_packages} != set(PACKAGES):
        raise ValueError("Semantic manifest package closure must be exact")
    if environment is not None:
        verify_environment(repository, declared, environment)
    import_boundary(tuple(manifests))
    parser_id = next(b.id for b in declared if b.id in PARSERS)
    entrypoints = {PARSER_ID: "physical_parser_v1", **EFFECTS, **FLAVORS, **SUMMARIES}
    for data in manifests:
        if data.id in PARSERS:
            if data.entrypoint != "physical_parser_v1":
                raise ValueError("Semantic registered entrypoint differs")
        elif data.entrypoint != entrypoints[data.id]:
            raise ValueError("Semantic registered entrypoint differs")
    return Checked(
        declared, tuple(manifests), tuple(sorted(set(exact))), PARSERS[parser_id]()
    )


def verify_recipes(
    repository: PinnedRepository, pins: tuple[Recipe, ...], checked: Checked
) -> None:
    """Recipe entrypoints are fixed calculation modules, not mutable orchestration."""
    permitted = versions.RECIPES
    for pin in pins:
        if pin.id not in permitted or pin.code_path != permitted[pin.id]:
            raise ValueError("Unsupported fixed template recipe entrypoint")
        frozen = {f.path: f.hash for m in checked.manifests for f in m.files}
        if frozen.get(pin.code_path) != pin.code_hash:
            raise ValueError("Template recipe code is absent from its semantic closure")
        raw = regular(repository, pin.code_revision, (pin.code_path,))[pin.code_path]
        if (
            digest(raw) != pin.code_hash
            or digest(canonical(pin.config)) != pin.config_hash
        ):
            raise ValueError("Fixed template recipe program or config hash mismatch")


def import_boundary(manifests: tuple[SemanticManifest, ...]) -> None:
    """Versioned calculations may reach only listed modules or explicit model/I/O boundaries."""
    files = {f.path for m in manifests for f in m.files}
    modules = {
        p.removeprefix("carddb/src/").removesuffix(".py").replace("/", ".")
        for p in files
        if p.endswith(".py")
    }
    for name in sorted(files):
        if not name.endswith(".py") or name in versions.METADATA_ONLY:
            continue
        for node in ast.walk(ast.parse(installed(name))):
            if (
                isinstance(node, ast.ImportFrom)
                and node.module
                and node.module.startswith("sve_carddb")
            ):
                children = {node.module + "." + alias.name for alias in node.names}
                if (
                    node.module not in modules
                    and node.module not in versions.BOUNDARIES
                    and not children <= modules
                ):
                    raise ValueError(
                        "Frozen semantic import crosses an undeclared boundary"
                    )


def verify_environment(
    repository: PinnedRepository,
    bindings: tuple[SemanticBinding, ...],
    environment: ProducerEnvironment,
) -> None:
    """Producer lock provenance is checked; host values never form an equality gate."""
    validate(environment)
    raw: dict[str, bytes] = {}
    for producer in sorted({b.revision for b in bindings}):
        raw = regular(repository, producer, ("carddb/pyproject.toml", "carddb/uv.lock"))
        if (digest(raw["carddb/uv.lock"]), digest(raw["carddb/pyproject.toml"])) != (
            environment.uv_lock_hash,
            environment.pyproject_hash,
        ):
            raise ValueError("Semantic producer environment lock evidence differs")
    required = object_value(
        parse(
            installed(
                "carddb/src/sve_carddb/template_semantics/v1/environment-artifacts.json"
            )
        )
    )
    locked = {
        p["name"]: p.get("version")
        for p in tomllib.loads(raw["carddb/uv.lock"].decode())["package"]
    }
    for package in environment.packages:
        spec = object_value(required[package.name])
        if (
            package.version != locked.get(package.name)
            or package.version != spec["version"]
        ):
            raise ValueError(
                "Semantic producer package version differs from fixed lock evidence"
            )
        if [a.path for a in package.artifacts if a.path.endswith(".py")] != array(
            spec["python_files"]
        ):
            raise ValueError("Semantic producer Python artifact closure must be exact")
        if sorted(
            a.path.split(".")[0]
            for a in package.artifacts
            if a.path.endswith((".so", ".pyd"))
        ) != array(spec["native_modules"]):
            raise ValueError("Semantic producer native artifact closure must be exact")


def foreign(context: ReplayContext) -> None:
    """Foreign readers validate support and closed evidence without recursing into sources."""
    flavor = isinstance(context.inputs, FlavorReplayInputs)
    closure(context.semantic_bindings, flavor=flavor)
    for item in context.semantic_bindings:
        path, _data = manifest(item.id)
        if (item.path, item.hash) != (path, versions.REGISTERED[item.id][1]):
            raise ValueError(
                "Semantic version manifest differs from its registered binding"
            )
    validate(context.environment)
