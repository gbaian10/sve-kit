"""Whole-entry refusal tests use exact single messages, never live/private inputs."""

import re
import shutil
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.digital_name_policies.current_models import LinkPolicy
from sve_carddb.digital_name_policies.loader import INDEX, decoded, load, model
from sve_carddb.registry.storage import MAX_BYTES, read_yaml
from sve_carddb.snapshot.values import canonical, digest, object_value

from .adoption_fixtures import commit
from .digital_name_policy_fixtures import LINKS, current, loader_repository, rewrite

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> tuple[Path, str]:
    return loader_repository(tmp_path_factory.mktemp("name-policy-loader"))


def copied(baseline: tuple[Path, str], root: Path) -> Path:
    shutil.copytree(baseline[0], root)
    return root


def reject(root: Path, message: str) -> None:
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        load(root / "authored", root, commit(root))


def test_complete_entry_reads_names_and_links(baseline: tuple[Path, str]) -> None:
    root, revision = baseline
    snapshot = load(root / "authored", root, revision)
    assert len(snapshot.files) == 3
    assert len(snapshot.current_names) == 1
    assert snapshot.links is not None
    assert snapshot.links.content.excluded_names == ()
    assert snapshot.links.content.excluded_targets == ()
    assert snapshot.pins()["authored_revision"] == revision


@pytest.mark.parametrize(
    "raw",
    [
        b"x: &a [*a]",
        b"x: *a",
        b"x: !custom hi",
        b"x: 1\nx: 2",
        b"x: .nan",
        b"\xff",
        b"x: 1\n---\nx: 2",
        b"? [a, b]\n: value",
    ],
    ids=[
        "cycle",
        "alias",
        "custom-tag",
        "duplicate",
        "nan",
        "utf8",
        "documents",
        "complex-key",
    ],
)
def test_yaml_boundary(raw: bytes) -> None:
    with pytest.raises(ValueError, match=r"^Invalid digital-name policy YAML$"):
        decoded(raw)


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        (b"x: &a hi\ny: *a", {"x": "hi", "y": "hi"}),
        (b"x: !!str hi", {"x": "hi"}),
        (b"x: {<<: {k: hi}}", {"x": {"k": "hi"}}),
    ],
    ids=["alias", "standard-tag", "merge"],
)
def test_yaml_expansion_is_allowed(raw: bytes, expected: JsonValue) -> None:
    assert decoded(raw) == expected


def test_size_boundary() -> None:
    with pytest.raises(
        ValueError, match=r"^Digital-name policy file exceeds size limit$"
    ):
        decoded(b" " * MAX_BYTES)


def test_bool_is_not_a_format(baseline: tuple[Path, str]) -> None:
    raw = current(baseline[0], LINKS)
    raw["digital_name_policy_format"] = True
    with pytest.raises(ValueError, match=r"^Policy format must be an integer$"):
        model(LinkPolicy, raw)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("extra", 1),
        ("kind", "unknown"),
        ("purpose", "coverage"),
        ("digital_name_policy_format", 1),
    ],
)
def test_unknown_policy_fields_are_not_ignored(
    baseline: tuple[Path, str], tmp_path: Path, field: str, value: JsonValue
) -> None:
    root = copied(baseline, tmp_path / "repo")
    raw = current(root, LINKS)
    raw[field] = value
    rewrite(root, LINKS, raw)
    reject(root, "Invalid digital-name policy fields")


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("path", "Digital-name policy indexed path mismatch"),
        ("hash", "Digital-name policy indexed hash mismatch"),
        ("missing", "Digital-name policy indexed path mismatch"),
        ("orphan", "Unindexed digital-name policy input"),
        ("identity", "Digital-name policy identity mismatch"),
        ("second", "Digital-name policy purpose must select at most one policy"),
    ],
)
def test_entry_closure(
    baseline: tuple[Path, str], tmp_path: Path, fault: str, message: str
) -> None:
    root = copied(baseline, tmp_path / "repo")
    index_path = root / "authored" / INDEX
    index = object_value(read_yaml(index_path))
    entry = object_value(object_value(index["policies"])[LINKS])
    if fault in {"path", "hash"}:
        entry[fault] = "../outside.yaml" if fault == "path" else digest(b"invalid")
        index_path.write_bytes(canonical(index))
    elif fault == "missing":
        (root / "authored" / str(entry["path"])).unlink()
    elif fault == "orphan":
        (root / "authored/digital-name-policies/orphan.yaml").write_text("{}")
    elif fault == "identity":
        raw = current(root, LINKS)
        raw["policy_id"] = "synthetic-other"
        rewrite(root, LINKS, raw)
    else:
        raw = current(root, LINKS)
        raw["policy_id"] = "synthetic-other-links"
        rewrite(root, "synthetic-other-links", raw)
    reject(root, message)


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("revision", "Policy authored revision must be a full Git SHA"),
        ("absent", "Digital-name policy index is missing"),
        ("disk", "Digital-name policy bytes differ from authored revision"),
        ("extra_disk", "Digital-name policy disk closure differs from immutable tree"),
        ("git_symlink", "Unsafe immutable digital-name policy file"),
        ("disk_symlink", "Symlink digital-name policy input"),
    ],
)
def test_git_and_disk_are_both_immutable(
    baseline: tuple[Path, str], tmp_path: Path, fault: str, message: str
) -> None:
    root = copied(baseline, tmp_path / "repo")
    revision = baseline[1]
    target = root / "authored/digital-name-policies" / LINKS / "current.yaml"
    if fault == "revision":
        revision = "HEAD"
    elif fault == "absent":
        (root / "authored" / INDEX).unlink()
        revision = commit(root)
    elif fault == "disk":
        target.write_bytes(target.read_bytes() + b"\n# different bytes\n")
    elif fault == "extra_disk":
        (target.parent / "orphan.yaml").write_text("{}")
    else:
        copy = tmp_path / "file.yaml"
        copy.write_bytes(target.read_bytes())
        target.unlink()
        target.symlink_to(copy)
        if fault == "git_symlink":
            revision = commit(root)
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        load(root / "authored", root, revision)


@pytest.mark.parametrize(
    "fault",
    [
        "no_batches",
        "unsorted_batches",
        "reason",
        "english_name",
        "target_width",
        "duplicate_name",
        "duplicate_target",
    ],
)
def test_link_conditions_are_closed(
    baseline: tuple[Path, str], tmp_path: Path, fault: str
) -> None:
    root = copied(baseline, tmp_path / "repo")
    raw = current(root, LINKS)
    content = object_value(raw["content"])
    name: dict[str, JsonValue] = {
        "source_lang": "ja",
        "source_name_hash": digest(b"Synthetic card"),
        "reason": "Synthetic exclusion",
    }
    target: dict[str, JsonValue] = {
        "card_id": "c:" + "1" * 32,
        "game": "sv1",
        "official_id": "123456789",
        "reason": "Synthetic exclusion",
    }
    if fault == "no_batches":
        content["source_batches"] = []
    elif fault == "unsorted_batches":
        content["source_batches"] = [
            {"batch_id": batch}
            for batch in sorted((digest(b"a"), digest(b"b")), reverse=True)
        ]
    elif fault == "reason":
        content["excluded_names"] = [name | {"reason": " "}]
    elif fault == "english_name":
        content["excluded_names"] = [name | {"source_lang": "en"}]
    elif fault == "target_width":
        content["excluded_targets"] = [target | {"official_id": "123"}]
    elif fault == "duplicate_name":
        content["excluded_names"] = [name, name]
    else:
        content["excluded_targets"] = [target, target]
    rewrite(root, LINKS, raw)
    reject(root, "Invalid digital-name policy fields")


def test_link_exclusions_are_read_directly(
    baseline: tuple[Path, str], tmp_path: Path
) -> None:
    root = copied(baseline, tmp_path / "repo")
    raw = current(root, LINKS)
    object_value(raw["content"])["excluded_targets"] = [
        {
            "card_id": "c:" + "1" * 32,
            "game": "svwb",
            "official_id": "22345678",
            "reason": "Synthetic different card",
        }
    ]
    rewrite(root, LINKS, raw)
    links = load(root / "authored", root, commit(root)).links
    assert links is not None
    assert [t.official_id for t in links.content.excluded_targets] == ["22345678"]


def test_missing_immutable_revision_is_rejected(baseline: tuple[Path, str]) -> None:
    root, _ = baseline
    with pytest.raises(ValueError, match=r"^Policy immutable tree is unavailable$"):
        load(root / "authored", root, "0" * 40)
