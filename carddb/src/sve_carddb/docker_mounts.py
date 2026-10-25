"""Derive Docker's immutable mounts from an offline export recipe."""

import json
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- resolve the checkout's Git common directory
import sys
from pathlib import Path

from sve_carddb.workflows.offline import Inputs


def readonly_roots(recipe_path: Path) -> tuple[Path, ...]:
    """Return canonical recipe inputs and the Git common dir needed by git show."""
    recipe = Inputs.model_validate_json(recipe_path.read_bytes())
    repo = recipe.repo.resolve(strict=True)
    inputs = recipe.protected_inputs()
    roots = [path.resolve(strict=True) for path in inputs]
    common_path = Path(
        subprocess.check_output(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed Git metadata query; repo path is recipe input
            [
                shutil.which("git") or "git",
                "-C",
                str(repo),
                "rev-parse",
                "--path-format=absolute",
                "--git-common-dir",
            ],
            text=True,
        ).strip()
    ).absolute()
    common = common_path.resolve(strict=True)
    unique = dict.fromkeys((*roots, common))
    for original, root in zip(inputs, roots, strict=True):
        if not original.is_absolute() or original != root:
            raise ValueError(
                f"Protected mount root must be absolute and symlink-free: {root}"
            )
    if not common_path.is_absolute() or common_path != common:
        raise ValueError(
            f"Git common dir must be absolute and symlink-free: {common_path}"
        )
    return tuple(unique)


def compose_override(roots: tuple[Path, ...]) -> str:
    """Render same-path, read-only bind mounts as a Compose override."""
    mounts = "\n".join(
        f"      - type: bind\n        source: {json.dumps(str(root))}\n        target: {json.dumps(str(root))}\n        read_only: true"
        for root in roots
    )
    return f"services:\n  export:\n    volumes:\n{mounts}\n"


def main() -> None:
    """Write the override file and print its recipe-owned repository root."""
    roots = readonly_roots(Path(sys.argv[1]))
    Path(sys.argv[2]).write_text(compose_override(roots), encoding="utf-8")
    sys.stdout.write(f"{roots[0]}\n")


if __name__ == "__main__":
    main()
