from pathlib import Path, PurePosixPath

import pytest

from sve_carddb.ingest.archive.store import (
    CorruptDataError,
    UnsafePathError,
    compress,
    decompress,
    relpath,
    resolve_within,
)


def test_relpath_keeps_remote_characters() -> None:
    assert relpath("raw", "jp", "card", "BP03-LDⓈ01.html.zst") == PurePosixPath(
        "raw/jp/card/BP03-LDⓈ01.html.zst"
    )
    assert relpath("ETD01", "etd01-002 .png").name == "etd01-002 .png"


@pytest.mark.parametrize("segment", ["", ".", "..", "a/b", "a\\b", "a\0b"])
def test_relpath_rejects_unsafe_segments(segment: str) -> None:
    with pytest.raises(UnsafePathError, match="segment"):
        relpath("raw", segment)


def test_resolve_within_returns_absolute_path(tmp_path: Path) -> None:
    rel = relpath("raw", "jp", "sets.html.zst")
    assert resolve_within(tmp_path, rel) == tmp_path.resolve() / "raw/jp/sets.html.zst"


def test_resolve_within_rejects_absolute_path(tmp_path: Path) -> None:
    with pytest.raises(UnsafePathError, match="relative"):
        resolve_within(tmp_path, PurePosixPath("/etc/passwd"))


def test_resolve_within_rejects_parent_traversal(tmp_path: Path) -> None:
    with pytest.raises(UnsafePathError, match="outside"):
        resolve_within(tmp_path, PurePosixPath("raw/../../escape"))


def test_resolve_within_rejects_symlink_escape(tmp_path: Path) -> None:
    root = tmp_path / "root"
    outside = tmp_path / "outside"
    root.mkdir()
    outside.mkdir()
    (root / "link").symlink_to(outside)
    with pytest.raises(UnsafePathError, match="outside"):
        resolve_within(root, PurePosixPath("link/file"))


def _moved_away(tmp_path: Path) -> tuple[Path, Path]:
    """A root whose `media` symlinks to a directory on another disk."""
    root = tmp_path / "root"
    disk = tmp_path / "disk"
    (disk / "media").mkdir(parents=True)
    (disk / "media" / "a.png").write_bytes(b"png")
    root.mkdir()
    (root / "media").symlink_to(disk / "media")
    return root, disk


def test_resolve_within_accepts_symlink_into_allowed_root(tmp_path: Path) -> None:
    root, disk = _moved_away(tmp_path)
    target = resolve_within(root, PurePosixPath("media/a.png"), also_allowed=[disk])
    assert target == (disk / "media" / "a.png").resolve()


def test_resolve_within_accepts_any_of_several_allowed_roots(tmp_path: Path) -> None:
    root, disk = _moved_away(tmp_path)
    other = tmp_path / "other"
    other.mkdir()
    rel = PurePosixPath("media/a.png")
    assert resolve_within(root, rel, also_allowed=[other, disk]).exists()


def test_resolve_within_rejects_symlink_outside_allowed_roots(tmp_path: Path) -> None:
    root, _ = _moved_away(tmp_path)
    other = tmp_path / "other"
    other.mkdir()
    with pytest.raises(UnsafePathError, match="outside"):
        resolve_within(root, PurePosixPath("media/a.png"), also_allowed=[other])


def test_resolve_within_rejects_parent_traversal_into_allowed_root(
    tmp_path: Path,
) -> None:
    root, disk = _moved_away(tmp_path)
    with pytest.raises(UnsafePathError, match="outside"):
        resolve_within(root, PurePosixPath("../disk/media/a.png"), also_allowed=[disk])


def test_resolve_within_rejects_absolute_path_even_inside_allowed_root(
    tmp_path: Path,
) -> None:
    root, disk = _moved_away(tmp_path)
    absolute = PurePosixPath(disk / "media" / "a.png")
    with pytest.raises(UnsafePathError, match="relative"):
        resolve_within(root, absolute, also_allowed=[disk])


def test_compress_round_trip() -> None:
    html = "<html><p>合成試験・テスト甲</p></html>".encode()
    stored = compress(html)
    assert stored != html
    assert decompress(stored) == html


def test_decompress_rejects_corrupt_data() -> None:
    stored = compress(b"<html>" * 100)
    with pytest.raises(CorruptDataError):
        decompress(stored[: len(stored) // 2])
