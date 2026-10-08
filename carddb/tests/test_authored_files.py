"""Authored boundaries allow linked ancestors but reject links within declared data."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.authored_files import read, shards
from sve_carddb.translations.loader import load_glossary

if TYPE_CHECKING:
    from pathlib import Path


def test_repository_below_symlink_is_readable(tmp_path: Path) -> None:
    real = tmp_path / "real"
    root = real / "repo/authored"
    path = root / "translations/glossary/example/001.yaml"
    path.parent.mkdir(parents=True)
    path.write_text("example: true\n")
    linked = tmp_path / "linked"
    linked.symlink_to(real, target_is_directory=True)
    authored = linked / "repo/authored"
    assert shards(authored, ("translations/glossary",)) == (
        (
            "translations/glossary/example/001.yaml",
            b"example: true\n",
            b'{"example":true}',
        ),
    )
    assert (
        read(authored / "translations/glossary/example/001.yaml", root=authored)[0]
        == b"example: true\n"
    )


@pytest.mark.parametrize(
    "part", ["authored", "translations", "glossary", "example", "001.yaml"]
)
def test_internal_symlink_is_rejected(tmp_path: Path, part: str) -> None:
    root = tmp_path / "repo/authored"
    path = root / "translations/glossary/example/001.yaml"
    path.parent.mkdir(parents=True)
    path.write_text("example: true\n")
    target = next(p for p in (path, *path.parents) if p.name == part)
    moved = tmp_path / "moved"
    target.rename(moved)
    target.symlink_to(moved, target_is_directory=moved.is_dir())
    with pytest.raises(ValueError, match=r"[Ss]ymlink"):
        shards(root, ("translations/glossary",))
    with pytest.raises(ValueError, match=r"[Ss]ymlink"):
        read(path, root=root)


def test_missing_required_area_is_not_empty(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="Missing authored data area"):
        shards(tmp_path, ("translations/glossary",))
    with pytest.raises(ValueError, match="Missing authored data area"):
        load_glossary(tmp_path)
    (tmp_path / "translations/templates").mkdir(parents=True)
    with pytest.raises(ValueError, match="Missing authored data area"):
        load_glossary(tmp_path)
    assert (
        shards(
            tmp_path, ("translations/overrides",), optional=("translations/overrides",)
        )
        == ()
    )


def test_optional_area_cannot_be_file_or_dangling_link(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="Missing authored data area"):
        shards(tmp_path / "missing-root", ("area",), optional=("area",))
    area = tmp_path / "area"
    area.write_text("example: true\n")
    with pytest.raises(ValueError, match="Missing authored data area"):
        shards(tmp_path, ("area",), optional=("area",))
    area.unlink()
    area.symlink_to("missing")
    with pytest.raises(ValueError, match="Symlink"):
        shards(tmp_path, ("area",), optional=("area",))


def test_read_cannot_traverse_above_authored(tmp_path: Path) -> None:
    root = tmp_path / "authored"
    root.mkdir()
    (tmp_path / "outside.yaml").write_text("example: true\n")
    with pytest.raises(ValueError, match="outside its data root"):
        read(root / "../outside.yaml", root=root)
