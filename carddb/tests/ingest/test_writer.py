import errno
import os
from pathlib import Path, PurePosixPath

import pytest

from sve_carddb.core.paths import UnsafePathError
from sve_carddb.core.regions import SourceRegion
from sve_carddb.ingest.archive.manifest import (
    Kind,
    Manifest,
    Outcome,
    RequestStart,
    Resource,
)
from sve_carddb.ingest.archive.store import decompress
from sve_carddb.ingest.http.writer import (
    DiskFullError,
    Fetched,
    LocalState,
    PathConflictError,
    RefreshProtectionError,
    Writer,
    WriteResult,
    remove_temp_files,
)

URL = "https://shadowverse-evolve.com/cardlist/?cardno=BP01-001"
PATH = PurePosixPath("raw/jp/card/BP01-001.html.zst")


def fetched(
    body: bytes = b"<html>v1</html>",
    *,
    url: str = URL,
    path: PurePosixPath = PATH,
    compressed: bool = True,
) -> Fetched:
    return Fetched(
        url=url,
        region=SourceRegion.JP,
        kind=Kind.CARD,
        path=path,
        body=body,
        content_type="text/html",
        etag=None,
        last_modified=None,
        compressed=compressed,
    )


def start(manifest: Manifest, url: str = URL) -> int:
    return manifest.requests.start(
        RequestStart("run", "fetch", 1, 0, url, url, sent_if_none_match=None)
    )


@pytest.fixture
def root(tmp_path: Path) -> Path:
    return tmp_path / "data"


@pytest.fixture
def writer(root: Path, manifest: Manifest) -> Writer:
    return Writer(root, manifest)


def write(
    writer: Writer, manifest: Manifest, item: Fetched, *, rewrite: bool = False
) -> WriteResult:
    return writer.write(item, request_id=start(manifest, item.url), rewrite=rewrite)


def test_new_page_is_compressed_and_recorded(
    writer: Writer, manifest: Manifest, root: Path
) -> None:
    assert writer.local_state(URL) is LocalState.MISSING
    assert write(writer, manifest, fetched()).changed
    assert decompress((root / PATH).read_bytes()) == b"<html>v1</html>"
    assert writer.local_state(URL) is LocalState.TRUSTED
    assert manifest.requests.outcomes(URL) == [Outcome.CHANGED]


def test_same_content_leaves_the_file_alone(
    writer: Writer, manifest: Manifest, root: Path
) -> None:
    write(writer, manifest, fetched())
    before = (root / PATH).stat().st_mtime_ns
    first = manifest.resources.get(URL)
    result = write(writer, manifest, fetched())
    assert not result.changed
    assert not result.rewritten
    after = manifest.resources.get(URL)
    assert (root / PATH).stat().st_mtime_ns == before
    assert first is not None
    assert after is not None
    assert after.last_changed_at == first.last_changed_at
    assert after.last_checked_at >= first.last_checked_at
    assert manifest.requests.outcomes(URL) == [Outcome.CHANGED, Outcome.UNCHANGED]


def test_changed_content_is_rewritten(
    writer: Writer, manifest: Manifest, root: Path
) -> None:
    write(writer, manifest, fetched())
    assert write(writer, manifest, fetched(b"<html>v2</html>")).changed
    assert decompress((root / PATH).read_bytes()) == b"<html>v2</html>"


def test_protected_writer_stops_changed_raw_before_replace(
    writer: Writer, manifest: Manifest, root: Path
) -> None:
    write(writer, manifest, fetched())
    original = (root / PATH).read_bytes()
    protected = Writer(root, manifest, protect_history=True)
    with pytest.raises(RefreshProtectionError):
        write(protected, manifest, fetched(b"<html>v2</html>"))
    assert (root / PATH).read_bytes() == original
    assert manifest.resources.get(URL) is not None


def test_uncompressed_content_is_stored_as_is(
    writer: Writer, manifest: Manifest, root: Path
) -> None:
    png = PurePosixPath("media/images/jp/BP01/bp01_001.png")
    write(writer, manifest, fetched(b"\x89PNG...", path=png, compressed=False))
    assert (root / png).read_bytes() == b"\x89PNG..."
    assert writer.local_state(URL) is LocalState.TRUSTED


@pytest.mark.parametrize("damage", ["truncate", "delete", "garbage"])
def test_damaged_file_is_untrusted_and_repaired_even_with_same_content(
    writer: Writer, manifest: Manifest, root: Path, damage: str
) -> None:
    write(writer, manifest, fetched())
    target = root / PATH
    if damage == "truncate":
        target.write_bytes(target.read_bytes()[:5])
    elif damage == "delete":
        target.unlink()
    else:
        target.write_bytes(b"not zstd at all")
    assert writer.local_state(URL) is LocalState.UNTRUSTED
    assert write(writer, manifest, fetched()).changed
    assert writer.local_state(URL) is LocalState.TRUSTED


def test_rewrite_mode_writes_even_a_trusted_identical_copy(
    writer: Writer, manifest: Manifest
) -> None:
    write(writer, manifest, fetched())
    result = write(writer, manifest, fetched(), rewrite=True)
    assert result.rewritten
    assert not result.changed
    assert writer.local_state(URL) is LocalState.TRUSTED


def test_path_owned_by_another_url_is_refused_before_writing(
    writer: Writer, manifest: Manifest, root: Path
) -> None:
    write(writer, manifest, fetched())
    other = fetched(b"<html>other</html>", url=f"{URL}-OTHER")
    with pytest.raises(PathConflictError, match="already belongs"):
        write(writer, manifest, other)
    assert decompress((root / PATH).read_bytes()) == b"<html>v1</html>"


def test_path_outside_the_root_is_refused(writer: Writer, manifest: Manifest) -> None:
    escaping = fetched(path=PurePosixPath("raw/../../escape.html.zst"))
    with pytest.raises(UnsafePathError):
        write(writer, manifest, escaping)


def test_read_roots_allow_reads_but_not_writes_through_a_symlink(
    writer: Writer, manifest: Manifest, root: Path, tmp_path: Path
) -> None:
    write(writer, manifest, fetched())
    disk = tmp_path / "disk"
    disk.mkdir()
    (root / "raw").rename(disk / "raw")
    (root / "raw").symlink_to(disk / "raw")
    reader = Writer(root, manifest, read_roots=[disk])
    assert reader.local_state(URL) is LocalState.TRUSTED
    assert reader.read(URL) == b"<html>v1</html>"
    with pytest.raises(UnsafePathError):
        writer.local_state(URL)
    with pytest.raises(UnsafePathError):
        write(reader, manifest, fetched(), rewrite=True)


def test_disk_full_is_fatal_and_leaves_no_temp_file(
    writer: Writer,
    manifest: Manifest,
    root: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    write(writer, manifest, fetched())

    def no_space(_fd: int) -> None:
        raise OSError(errno.ENOSPC, os.strerror(errno.ENOSPC))

    monkeypatch.setattr(os, "fsync", no_space)
    with pytest.raises(DiskFullError):
        write(writer, manifest, fetched(b"<html>v2</html>"))
    monkeypatch.undo()
    assert decompress((root / PATH).read_bytes()) == b"<html>v1</html>"
    assert remove_temp_files(root) == []


def test_crash_before_commit_is_detected_as_untrusted(
    writer: Writer, manifest: Manifest
) -> None:
    write(writer, manifest, fetched())

    def crash(_resource: Resource) -> None:
        raise KeyboardInterrupt

    with pytest.raises(KeyboardInterrupt):
        writer.write(
            fetched(b"<html>v2</html>"),
            request_id=start(manifest),
            in_transaction=crash,
        )
    # The file already holds v2 but the manifest still says v1.
    assert writer.local_state(URL) is LocalState.UNTRUSTED
    assert write(writer, manifest, fetched(b"<html>v2</html>")).changed
    assert writer.local_state(URL) is LocalState.TRUSTED


def test_in_transaction_callback_commits_with_the_resource(
    writer: Writer, manifest: Manifest
) -> None:
    seen: list[str] = []
    writer.write(
        fetched(),
        request_id=start(manifest),
        in_transaction=lambda resource: seen.append(resource.sha256),
    )
    resource = manifest.resources.get(URL)
    assert resource is not None
    assert seen == [resource.sha256]


def test_not_modified_needs_a_trusted_copy(
    writer: Writer, manifest: Manifest, root: Path
) -> None:
    write(writer, manifest, fetched())
    writer.mark_not_modified(URL, request_id=start(manifest))
    assert manifest.requests.outcomes(URL)[-1] is Outcome.NOT_MODIFIED
    (root / PATH).unlink()
    with pytest.raises(RuntimeError, match="trusted"):
        writer.mark_not_modified(URL, request_id=start(manifest))


def test_remove_temp_files(root: Path) -> None:
    leftover = root / "raw" / "jp" / "card" / "BP01-001.html.zst.tmp-abcd1234"
    leftover.parent.mkdir(parents=True)
    leftover.write_bytes(b"partial")
    keep = leftover.with_name("BP01-001.html.zst")
    keep.write_bytes(b"done")
    assert remove_temp_files(root) == [leftover]
    assert keep.exists()


def test_read_returns_the_trusted_local_copy(
    writer: Writer, manifest: Manifest, root: Path
) -> None:
    write(writer, manifest, fetched())
    assert writer.read(URL) == b"<html>v1</html>"
    (root / PATH).write_bytes(b"damaged")
    with pytest.raises(RuntimeError, match="trusted"):
        writer.read(URL)
