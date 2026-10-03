"""Immutable Git recipes reuse the existing canonical hash and Git-blob boundary."""

import sys
import unicodedata
from pathlib import Path

from pydantic import JsonValue

from sve_carddb.build_inputs import BuildContext
from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_sources import normalizer
from sve_carddb.template_sources.models import Recipe
from sve_carddb.translations.sources import CODE_PATH as PARSER_PATH

PARSER = "translation-jp-v1"
RUNTIME = Path(__file__).resolve().parents[4]


def recipes(repository: Path, revision: str) -> tuple[Recipe, ...]:
    """Pin the entire installed first-party runtime plus the dependency lock."""
    names = {"carddb/uv.lock", "carddb/pyproject.toml"}
    for path in (RUNTIME / "carddb/src/sve_carddb").rglob("*.py"):
        if path.name == "_version.py":
            # Hatch generates this metadata; it is not a recipe dependency.
            continue
        if path.is_symlink():
            raise ValueError("Template runtime cannot contain symlinked modules")
        names.add(path.relative_to(RUNTIME).as_posix())
    content = PinnedRepository(repository).read_many(revision, tuple(sorted(names)))
    if any((RUNTIME / name).read_bytes() != raw for name, raw in content.items()):
        raise ValueError("Historical template runtime cannot be replayed")
    context = BuildContext.from_inputs(revision, content, {})
    result = []
    specifications: tuple[tuple[str, str, dict[str, JsonValue]], ...] = (
        (PARSER, PARSER_PATH, {"provider": "jp"}),
        (
            normalizer.VERSION,
            normalizer.CODE_PATH,
            {
                "recognition_policy": None,
                "python_version": sys.version.split()[0],
                "unicode_version": unicodedata.unidata_version,
                "dependencies": {pin.name: pin.sha256 for pin in context.dependencies},
            },
        ),
    )
    for identifier, code_path, config in specifications:
        result.append(
            Recipe.model_validate(
                {
                    "id": identifier,
                    "code_revision": revision,
                    "code_path": code_path,
                    "code_hash": digest(content[code_path]),
                    "config": config,
                    "config_hash": digest(canonical(config)),
                }
            )
        )
    return tuple(sorted(result, key=lambda pin: pin.id))


def verify_recipes(repository: Path, pins: tuple[Recipe, ...]) -> None:
    """Reject unknown versions, changed config/runtime and mixed producing revisions."""
    if not pins or pins != recipes(repository, pins[0].code_revision):
        raise ValueError("Template recipe pins or Python/Unicode runtime do not match")
