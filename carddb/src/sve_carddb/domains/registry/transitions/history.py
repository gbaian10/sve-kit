"""Exact historical Git registry inputs for mapping-review renewal."""

import io
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- Only immutable local Git blobs are read.
import tarfile
from pathlib import Path, PurePosixPath
from tempfile import TemporaryDirectory
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.provenance import FilePin
from sve_carddb.domains.registry.storage import RegistryFiles, read_base_files
from sve_carddb.domains.registry.validate import check_cursors, validate_structure

if TYPE_CHECKING:
    from sve_carddb.domains.registry.transitions.models import RegistryBasis


def git(repo: Path, *args: str) -> bytes:
    """Use argument arrays and sanitize failures instead of printing imported content."""
    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- Fixed local Git operations use argument arrays without a shell.
        [shutil.which("git") or "/usr/bin/git", "-C", str(repo), *args],
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise ValueError("Cannot reconstruct pinned registry Git input")
    return result.stdout


class History:
    def __init__(self, repo: Path) -> None:
        self.repo = repo
        self._registries: dict[str, RegistryFiles] = {}

    def dependency(self, revision: str) -> FilePin:
        """Pin the actual dependency lock bytes at the recorded program revision."""
        return FilePin(
            name="carddb/uv.lock",
            sha256=digest(git(self.repo, "show", revision + ":carddb/uv.lock")),
        )

    def registry(self, basis: RegistryBasis) -> RegistryFiles:
        """Rebuild the complete original registry from an exact immutable Git tree."""
        revision = basis.authored_revision
        if revision not in self._registries:
            exact = git(self.repo, "rev-parse", "--verify", revision + "^{commit}")
            if exact.decode().strip() != revision:
                raise ValueError("Registry basis must be an exact Git commit")
            raw = git(
                self.repo,
                "archive",
                "--format=tar",
                revision,
                "authored/registry",
                "authored/ids",
            )
            with TemporaryDirectory(prefix="registry-basis-") as temporary:
                root = Path(temporary)
                with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
                    for member in archive.getmembers():
                        path = PurePosixPath(member.name)
                        if member.isdir() and path.as_posix() == "authored":
                            continue
                        if (
                            path.is_absolute()
                            or ".." in path.parts
                            or path.parts[:2]
                            not in {("authored", "registry"), ("authored", "ids")}
                        ):
                            raise ValueError("Unsafe historical registry Git path")
                        if member.isdir():
                            continue
                        if not member.isfile() or path.suffix != ".yaml":
                            raise ValueError(
                                "Unsupported historical registry Git entry"
                            )
                        content = archive.extractfile(member)
                        if content is None:
                            raise ValueError("Missing historical registry Git blob")
                        destination = root / path
                        destination.parent.mkdir(parents=True, exist_ok=True)
                        destination.write_bytes(content.read())
                self._registries[revision] = read_base_files(root / "authored")
        files = self._registries[revision]
        if digest(canonical(files.index().model_dump(mode="json"))) != basis.index_hash:
            raise ValueError("Historical registry basis index hash mismatch")
        return files


def check_append(previous: RegistryFiles | None, current: RegistryFiles) -> None:
    """Keep historic shard bytes and allocation cursors while allowing new shards."""
    entries = [entry for shard in current.shards for entry in shard.envelope().records]
    validate_structure(entries)
    check_cursors(current.index().next_int_id, entries)
    if previous is None:
        return
    shards = {shard.path: shard for shard in current.shards}
    if any(
        old.path not in shards
        or (old.content, old.exact_content)
        != (shards[old.path].content, shards[old.path].exact_content)
        for old in previous.shards
    ):
        raise ValueError("Registry renewal must preserve exact historical shard bytes")
    before, after = previous.index(), current.index()
    if before.allocation_policy != after.allocation_policy or any(
        before.next_int_id[region] > after.next_int_id[region]
        for region in ("en", "jp")
    ):
        raise ValueError("Registry renewal allocation cursors cannot move backwards")
