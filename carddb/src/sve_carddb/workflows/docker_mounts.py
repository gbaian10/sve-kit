"""Derive Docker's immutable mounts from an offline export recipe."""

import json
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- resolve the checkout's Git common directory
import sys
from pathlib import Path

from sve_carddb.workflows.offline import Inputs


def git_common_dir(repo: Path) -> Path:
    """Locate the object store that `git show` needs; a worktree keeps it outside the checkout."""
    output = subprocess.check_output(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed Git metadata query; repo path is recipe input
        [
            shutil.which("git") or "git",
            "-C",
            str(repo),
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        ],
        text=True,
    )
    return Path(output.strip())


def readonly_roots(recipe: Inputs) -> tuple[Path, ...]:
    """Return the recipe's protected inputs and the Git common dir, without duplicates."""
    protected = recipe.protected_inputs()
    # Checked before Git runs, so a relative or missing repo fails with the path rather than a Git error.
    _require_canonical(protected)
    common = git_common_dir(recipe.repo)
    _require_canonical((common,))
    return tuple(dict.fromkeys((*protected, common)))


def _require_canonical(roots: tuple[Path, ...]) -> None:
    for root in roots:
        if not root.is_absolute() or root.resolve(strict=True) != root:
            raise ValueError(
                f"Protected mount root must be absolute and symlink-free: {root}"
            )


def compose_override(roots: tuple[Path, ...]) -> str:
    """Render same-path, read-only bind mounts as a Compose override."""
    mounts = "\n".join(
        f"      - type: bind\n        source: {json.dumps(str(root))}\n        target: {json.dumps(str(root))}\n        read_only: true"
        for root in roots
    )
    return f"services:\n  export:\n    volumes:\n{mounts}\n"


def main() -> None:
    """Write the override file and print the recipe's repository, the container's working directory."""
    recipe = Inputs.model_validate_json(Path(sys.argv[1]).read_bytes())
    Path(sys.argv[2]).write_text(
        compose_override(readonly_roots(recipe)), encoding="utf-8"
    )
    sys.stdout.write(f"{recipe.repo}\n")


if __name__ == "__main__":
    main()
