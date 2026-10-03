"""Producer environment is evidence, never an equality gate on historical output."""

import importlib.metadata
import platform
import sys
import tomllib
import unicodedata
from pathlib import Path
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_translations.replay_models import (
    Artifact,
    PackageEnvironment,
    ProducerEnvironment,
    PythonEnvironment,
)

if TYPE_CHECKING:
    from pydantic import JsonValue

PACKAGES = ("pydantic", "pydantic-core", "pyyaml", "ruamel-yaml", "selectolax")
NATIVE = frozenset({"pydantic-core", "pyyaml", "selectolax"})


def capture(root: Path, *, producer: bool) -> ProducerEnvironment:
    """Hash installed package artifacts including the actual native filenames."""
    lock_raw = (root / "carddb/uv.lock").read_bytes()
    locked = {
        row["name"]: row["version"]
        for row in tomllib.loads(lock_raw.decode())["package"]
        if "version" in row
    }
    packages = []
    for name in PACKAGES:
        dist = importlib.metadata.distribution(name)
        files = []
        for path in dist.files or ():
            relative = str(path)
            if (
                ".." in path.parts
                or path.is_absolute()
                or path.suffix not in {".py", ".so", ".pyd"}
            ):
                continue
            installed = Path(str(dist.locate_file(path)))
            if installed.is_symlink() or not installed.is_file():
                raise ValueError("Semantic package artifact must be a regular file")
            files.append(Artifact(path=relative, hash=digest(installed.read_bytes())))
        if producer and locked.get(name) != dist.version:
            raise ValueError("Semantic producer package differs from its exact lock")
        packages.append(
            PackageEnvironment(
                name=name,
                version=dist.version,
                artifacts=tuple(sorted(files, key=lambda a: a.path)),
            )
        )
    result = ProducerEnvironment(
        python=PythonEnvironment(
            version=sys.version.split()[0],
            implementation=platform.python_implementation(),
            unicode_version=unicodedata.unidata_version,
        ),
        packages=tuple(packages),
        uv_lock_hash=digest(lock_raw),
        pyproject_hash=digest((root / "carddb/pyproject.toml").read_bytes()),
    )
    validate(result)
    return result


def validate(environment: ProducerEnvironment) -> None:
    """A caller cannot shrink the necessary package/native artifact evidence."""
    if tuple(p.name for p in environment.packages) != PACKAGES:
        raise ValueError(
            "Semantic environment must contain its exact necessary packages"
        )
    for package in environment.packages:
        if not any(a.path.endswith(".py") for a in package.artifacts) or (
            package.name in NATIVE
            and not any(a.path.endswith((".so", ".pyd")) for a in package.artifacts)
        ):
            raise ValueError(
                "Semantic environment omits necessary Python or native artifacts"
            )


def differences(
    producer: ProducerEnvironment, actual: ProducerEnvironment
) -> dict[str, JsonValue]:
    """Expose both values without guessing that a difference caused output drift."""
    left, right = producer.model_dump(mode="json"), actual.model_dump(mode="json")
    return {
        name: {"producer": left[name], "actual": right[name]}
        for name in sorted(left)
        if canonical(left[name]) != canonical(right[name])
    }
