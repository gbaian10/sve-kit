"""Exact flavor recipes pin the complete installed runtime independently of effect rules."""

from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import BuildContext
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameter_rules.repository import git, revision
from sve_carddb.template_sources.flavor import CODE_PATH, VERSION
from sve_carddb.template_sources.models import Recipe
from sve_carddb.template_sources.pins import PARSER
from sve_carddb.translations.sources import CODE_PATH as PARSER_PATH

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_sources import PinnedRepository

RUNTIME = Path(__file__).resolve().parents[4]


def runtime(repository: PinnedRepository, commit: str) -> dict[str, bytes]:
    """Deleted, extra and changed modules cannot disappear from the dependency closure."""
    revision(repository, commit)
    installed = {
        p.relative_to(RUNTIME).as_posix()
        for p in (RUNTIME / "carddb/src/sve_carddb").rglob("*.py")
        if p.name != "_version.py"
    }
    pinned = set()
    for row in (
        git(
            repository,
            "ls-tree",
            "-r",
            "--format=%(objectmode)%x09%(objecttype)%x09%(path)",
            commit,
            "--",
            "carddb/src/sve_carddb",
        )
        .decode()
        .splitlines()
    ):
        mode, kind, name = row.split("\t", 2)
        if name.endswith(".py") and not name.endswith("/_version.py"):
            if mode not in {"100644", "100755"} or kind != "blob":
                raise ValueError("Flavor runtime modules must be regular Git files")
            pinned.add(name)
    if installed != pinned:
        raise ValueError("Flavor runtime module closure differs from pinned Git")
    names = tuple(sorted(installed | {"carddb/uv.lock", "carddb/pyproject.toml"}))
    content = repository.read_many(commit, names)
    if any(
        (RUNTIME / name).is_symlink() or (RUNTIME / name).read_bytes() != raw
        for name, raw in content.items()
    ):
        raise ValueError("Flavor historical runtime cannot be replayed")
    return content


def recipes(repository: PinnedRepository, commit: str) -> tuple[Recipe, ...]:
    """Emit the two closed recipe pins without a borrowed recognition config."""
    content = runtime(repository, commit)
    specifications: tuple[tuple[str, str, dict[str, JsonValue]], ...] = (
        (VERSION, CODE_PATH, {}),
        (PARSER, PARSER_PATH, {"provider": "jp"}),
    )
    return tuple(
        Recipe(
            id=identifier,
            code_revision=commit,
            code_path=path,
            code_hash=digest(content[path]),
            config=config,
            config_hash=digest(canonical(config)),
        )
        for identifier, path, config in specifications
    )


def verify(repository: PinnedRepository, pins: tuple[Recipe, ...]) -> BuildContext:
    """Config is exactly empty; batch and identity are explicit F1 inputs, not recipe options."""
    if not pins or pins != recipes(repository, pins[0].code_revision):
        raise ValueError("Flavor requires its exact independent recipe and parser pins")
    parser = pins[1]
    return BuildContext.from_inputs(
        parser.code_revision,
        runtime(repository, parser.code_revision),
        {
            "translation_recipes": {
                "translation-" + provider + "-v1": {
                    "version": "translation-" + provider + "-v1",
                    "program_revision": parser.code_revision,
                    "code_path": parser.code_path,
                    "code_hash": parser.code_hash,
                    "config": {"provider": provider},
                    "config_hash": digest(canonical({"provider": provider})),
                }
                for provider in ("jp", "en")
            }
        },
    )
