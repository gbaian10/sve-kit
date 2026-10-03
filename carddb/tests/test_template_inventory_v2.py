"""Synthetic historical envelopes do not imply source replay or human adoption."""

import copy
import json
import re
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.template_translations.loader import _inventory, load_templates
from sve_carddb.template_translations.replay_models import (
    InventoryV2,
    ReplayContext,
    SemanticManifest,
)
from sve_carddb.translations.loader import load_glossary

from .template_intake_fixtures import (
    INVENTORY,
    intake_case,
    policy_git,
    recognition_term_source,
    write,
)

if TYPE_CHECKING:
    from pathlib import Path

    from .template_intake_fixtures import Case

__all__ = ("intake_case", "policy_git", "recognition_term_source")
HASH = "sha256:" + "a" * 64
REVISION = "b" * 40
STREAMS = ("entries", "fields", "members", "coverage", "checkpoint", "source_uses")


def context() -> dict[str, JsonValue]:
    """Empty streams make a structurally valid baseline, not evidence of a real batch."""
    outputs: dict[str, JsonValue] = {
        "format": 1,
        "recipe": "semantic-output-v1",
        "streams": {k: {"count": 0, "hash": digest(canonical([]))} for k in STREAMS},
    }
    outputs["root"] = digest(canonical(outputs))
    return {
        "format": 1,
        "semantic_bindings": [
            {
                "id": "flavor-exact-v1",
                "revision": REVISION,
                "path": "carddb/semantic/flavor-v1.json",
                "hash": HASH,
            }
        ],
        "environment": {
            "python": {
                "version": "3.14.7",
                "implementation": "CPython",
                "unicode_version": "16.0.0",
            },
            "packages": [
                {
                    "name": "selectolax",
                    "version": "0.4.0",
                    "artifacts": [{"path": "selectolax/native.so", "hash": HASH}],
                }
            ],
            "uv_lock_hash": HASH,
            "pyproject_hash": HASH,
        },
        "inputs": {"kind": "effect"},
        "expected_outputs": outputs,
    }


@pytest.fixture
def wire() -> dict[str, JsonValue]:
    return {
        "template_source_format": 2,
        "kind": "template_source_inventory",
        "recipes": [
            {
                "id": "synthetic-v1",
                "code_revision": REVISION,
                "code_path": "carddb/semantic/test.py",
                "code_hash": HASH,
                "config": {},
                "config_hash": digest(canonical({})),
            }
        ],
        "replay_context": context(),
        "entries": [
            {
                "id": "inv:" + "c" * 64,
                "level": "sentence",
                "source_ref": {
                    "store_id": "synthetic",
                    "batch_id": HASH,
                    "source_version_id": "src:v1:" + "d" * 64,
                    "parser": "translation-jp-v1",
                    "locator": "/faces/0/effect",
                    "text_hash": HASH,
                },
                "line_ordinal": 0,
                "role": "body",
                "normalizer_id": "classification-jp-v0-v1",
                "normalized_hash": HASH,
                "legacy_fingerprint": HASH,
            }
        ],
    }


def set_value(
    data: dict[str, JsonValue], path: tuple[str, ...], value: JsonValue
) -> None:
    target = data
    for part in path[:-1]:
        target = object_value(target[part])
    target[path[-1]] = value


def test_v2_roundtrip_and_environment_difference_are_structural(
    wire: dict[str, JsonValue],
) -> None:
    first = _inventory(json.dumps(wire).encode())
    assert isinstance(first, InventoryV2)
    assert canonical(first.model_dump(mode="json")) == canonical(wire)
    set_value(wire, ("replay_context", "environment", "python", "version"), "3.15.1")
    second = _inventory(json.dumps(wire).encode())
    assert isinstance(second, InventoryV2)
    assert second.replay_context.environment.python.version == "3.15.1"
    assert first.group_key() != second.group_key()


@pytest.mark.parametrize("version", [True, False, 1.0, 2.0, "2", 3, None])
def test_v2_format_is_exact(wire: dict[str, JsonValue], version: JsonValue) -> None:
    wire["template_source_format"] = version
    with pytest.raises(ValueError, match=r"^Invalid formal template inventory$"):
        _inventory(json.dumps(wire).encode())


@pytest.mark.parametrize(
    ("path", "value"),
    [
        (("replay_context", "format"), True),
        (("replay_context", "format"), 1.0),
        (("replay_context", "format"), 2),
        (("replay_context", "environment", "python", "version"), ""),
        (("replay_context", "environment", "uv_lock_hash"), "not-a-hash"),
        (("replay_context", "environment", "packages"), []),
        (
            ("replay_context", "environment", "packages"),
            [{"name": "parser", "version": "1", "artifacts": []}],
        ),
        (("replay_context", "semantic_bindings"), []),
        (
            ("replay_context", "inputs"),
            {"kind": "effect", "source_batch": {"store_id": "test", "batch_id": HASH}},
        ),
        (("replay_context", "inputs"), {"kind": "effect", "legacy_file_hash": HASH}),
        (("replay_context", "inputs"), {"kind": "effect", "references": {}}),
        (("replay_context", "inputs"), {"kind": "other"}),
        (("replay_context", "inputs"), {"kind": "flavor"}),
        (("replay_context", "expected_outputs", "format"), True),
        (("replay_context", "expected_outputs", "format"), 1.0),
        (("replay_context", "expected_outputs", "recipe"), "semantic-output-v2"),
        (("replay_context", "expected_outputs", "root"), HASH),
        (("replay_context", "expected_outputs", "streams", "members", "count"), True),
        (("replay_context", "expected_outputs", "streams", "members", "count"), -1),
        (("replay_context", "expected_outputs", "streams", "members", "count"), 1.0),
        (
            ("replay_context", "expected_outputs", "streams", "members", "count"),
            9007199254740992,
        ),
        (("replay_context", "expected_outputs", "streams", "members", "hash"), "short"),
    ],
)
def test_closed_v2_value_guards(
    wire: dict[str, JsonValue], path: tuple[str, ...], value: JsonValue
) -> None:
    set_value(wire, path, value)
    with pytest.raises(ValueError, match=r"^Invalid formal template inventory$"):
        _inventory(json.dumps(wire).encode())


@pytest.mark.parametrize(
    "path",
    [
        (),
        ("replay_context",),
        ("replay_context", "environment"),
        ("replay_context", "environment", "python"),
        ("replay_context", "inputs"),
        ("replay_context", "expected_outputs"),
        ("replay_context", "expected_outputs", "streams"),
        ("replay_context", "expected_outputs", "streams", "fields"),
    ],
)
def test_v2_rejects_unknown_keys(
    wire: dict[str, JsonValue], path: tuple[str, ...]
) -> None:
    node = wire
    for part in path:
        node = object_value(node[part])
    node["extra"] = "synthetic-secret-free"
    with pytest.raises(ValueError, match=r"^Invalid formal template inventory$"):
        _inventory(json.dumps(wire).encode())


@pytest.mark.parametrize(
    "key", ["format", "semantic_bindings", "environment", "inputs", "expected_outputs"]
)
def test_context_has_no_defaults(wire: dict[str, JsonValue], key: str) -> None:
    del object_value(wire["replay_context"])[key]
    with pytest.raises(ValueError, match=r"^Invalid formal template inventory$"):
        _inventory(json.dumps(wire).encode())


@pytest.mark.parametrize("stream", STREAMS)
def test_expected_requires_every_stream(
    wire: dict[str, JsonValue], stream: str
) -> None:
    outputs = object_value(object_value(wire["replay_context"])["expected_outputs"])
    del object_value(outputs["streams"])[stream]
    outputs["root"] = digest(
        canonical({k: v for k, v in outputs.items() if k != "root"})
    )
    with pytest.raises(ValueError, match=r"^Invalid formal template inventory$"):
        _inventory(json.dumps(wire).encode())


def test_count_change_must_change_root(wire: dict[str, JsonValue]) -> None:
    set_value(
        wire, ("replay_context", "expected_outputs", "streams", "entries", "count"), 1
    )
    with pytest.raises(ValueError, match=r"^Invalid formal template inventory$"):
        _inventory(json.dumps(wire).encode())


@pytest.mark.parametrize(
    "path",
    [
        "/absolute/source",
        "../file",
        "part/../file",
        "part/./file",
        "part//file",
        "part/",
        "part\\file",
        "part\nfile",
    ],
)
def test_binding_paths_are_relative_and_safe(
    wire: dict[str, JsonValue], path: str
) -> None:
    ctx = object_value(wire["replay_context"])
    ctx["semantic_bindings"] = [
        {"id": "parser-v1", "revision": REVISION, "path": path, "hash": HASH}
    ]
    with pytest.raises(ValueError, match=r"^Invalid formal template inventory$"):
        _inventory(json.dumps(wire).encode())


def test_flavor_context_is_per_inventory_and_can_remain_pending(
    wire: dict[str, JsonValue],
) -> None:
    ctx = object_value(wire["replay_context"])
    ctx["inputs"] = {
        "kind": "flavor",
        "source_batch": {"store_id": "test", "batch_id": HASH},
        "identity_basis": None,
        "identity_batches": [],
    }
    recipe = object_value(array(wire["recipes"])[0])
    recipe["id"] = "flavor-exact-v1"
    entry = object_value(array(wire["entries"])[0])
    entry.update(
        role="flavor", normalizer_id="flavor-exact-v1", legacy_fingerprint=None
    )
    object_value(entry["source_ref"])["locator"] = "/faces/0/flavor"
    first = InventoryV2.model_validate_json(canonical(wire))
    inputs = object_value(ctx["inputs"])
    inputs["identity_basis"] = {
        "authored_revision": REVISION,
        "registry_index_hash": HASH,
        "transition_index_hash": None,
    }
    second = InventoryV2.model_validate_json(canonical(wire))
    assert first.group_key() != second.group_key()
    object_value(inputs["source_batch"])["batch_id"] = "sha256:" + "e" * 64
    third = InventoryV2.model_validate_json(canonical(wire))
    assert second.group_key() != third.group_key()
    recipe["config"] = {"source_batch": {"store_id": "test", "batch_id": HASH}}
    with pytest.raises(ValueError, match=r"^Invalid formal template inventory$"):
        _inventory(json.dumps(wire).encode())


def test_group_key_contains_bindings_inputs_and_expected(
    wire: dict[str, JsonValue],
) -> None:
    first = InventoryV2.model_validate_json(canonical(wire))
    expected = digest(canonical({k: wire[k] for k in ("recipes", "replay_context")}))
    assert first.group_key() == expected
    for key in ("recipes", "replay_context"):
        assert key in first.model_dump(mode="json")
    changed = copy.deepcopy(wire)
    object_value(array(changed["entries"])[0])["id"] = "inv:" + "f" * 64
    assert InventoryV2.model_validate_json(canonical(changed)).group_key() == expected


def test_manifest_closed_and_ordered() -> None:
    raw: dict[str, JsonValue] = {
        "semantic_version_format": 1,
        "id": "parser-v1",
        "entrypoint": "parser_v1",
        "files": [{"path": "carddb/parser.py", "hash": HASH}],
        "environment_packages": ["selectolax"],
    }
    manifest = SemanticManifest.model_validate_json(canonical(raw))
    assert manifest.id == "parser-v1"
    raw["files"] = [{"path": "carddb/parser.py", "hash": HASH}] * 2
    with pytest.raises(
        ValidationError,
        match=re.escape("Semantic manifest files must be sorted and unique"),
    ):
        SemanticManifest.model_validate_json(canonical(raw))


def test_context_order_does_not_pick_a_last_binding(wire: dict[str, JsonValue]) -> None:
    raw = object_value(wire["replay_context"])
    bindings = array(raw["semantic_bindings"])
    raw["semantic_bindings"] = [bindings[0], bindings[0]]
    with pytest.raises(
        ValidationError, match=re.escape("Semantic bindings must be sorted and unique")
    ):
        ReplayContext.model_validate_json(canonical(raw))


@pytest.mark.parametrize(
    "change",
    [
        "bindings_order",
        "packages_duplicate",
        "packages_order",
        "artifacts_duplicate",
        "artifacts_order",
        "artifact_path",
        "artifact_unknown",
        "package_unknown",
        "binding_unknown",
        "entries_duplicate",
        "recipes_duplicate",
        "kind_mismatch",
    ],
)
def test_order_uniqueness_and_nested_closed_shapes(  # ruff: ignore[complex-structure] -- independently test each closed level from a valid base
    wire: dict[str, JsonValue], change: str
) -> None:
    ctx = object_value(wire["replay_context"])
    package = object_value(array(object_value(ctx["environment"])["packages"])[0])
    binding = object_value(array(ctx["semantic_bindings"])[0])
    artifact = object_value(array(package["artifacts"])[0])
    if change == "bindings_order":
        ctx["semantic_bindings"] = [{**binding, "id": "z-v1"}, binding]
    elif change == "packages_duplicate":
        object_value(ctx["environment"])["packages"] = [package, package]
    elif change == "packages_order":
        object_value(ctx["environment"])["packages"] = [
            {**package, "name": "z-parser"},
            package,
        ]
    elif change == "artifacts_duplicate":
        package["artifacts"] = [artifact, artifact]
    elif change == "artifacts_order":
        package["artifacts"] = [{**artifact, "path": "z.so"}, artifact]
    elif change == "artifact_path":
        artifact["path"] = "../native.so"
    elif change == "artifact_unknown":
        artifact["extra"] = 0
    elif change == "package_unknown":
        package["extra"] = 0
    elif change == "binding_unknown":
        binding["extra"] = 0
    elif change == "entries_duplicate":
        wire["entries"] = [*array(wire["entries"]), *array(wire["entries"])]
    elif change == "recipes_duplicate":
        wire["recipes"] = [*array(wire["recipes"]), *array(wire["recipes"])]
    else:
        ctx["inputs"] = {
            "kind": "flavor",
            "source_batch": {"store_id": "test", "batch_id": HASH},
            "identity_basis": None,
            "identity_batches": [],
        }
        object_value(array(wire["recipes"])[0])["id"] = "flavor-exact-v1"
    with pytest.raises(ValueError, match=r"^Invalid formal template inventory$"):
        _inventory(json.dumps(wire).encode())


@pytest.mark.parametrize("version", [True, 1.0, "1", 2])
def test_manifest_format_is_strict(version: JsonValue) -> None:
    raw = {
        "semantic_version_format": version,
        "id": "parser-v1",
        "entrypoint": "parser_v1",
        "files": [{"path": "carddb/parser.py", "hash": HASH}],
        "environment_packages": ["selectolax"],
    }
    with pytest.raises(ValidationError):
        SemanticManifest.model_validate_json(json.dumps(raw))


def test_flavor_identity_batches_are_exact_unique_and_ordered(
    wire: dict[str, JsonValue],
) -> None:
    ctx = object_value(wire["replay_context"])
    batch: dict[str, JsonValue] = {"store_id": "test", "batch_id": HASH}
    ctx["inputs"] = {
        "kind": "flavor",
        "source_batch": batch,
        "identity_basis": None,
        "identity_batches": [batch, batch],
    }
    with pytest.raises(
        ValidationError,
        match=re.escape("Flavor replay identity batches must be sorted and unique"),
    ):
        ReplayContext.model_validate_json(canonical(ctx))
    inputs = object_value(ctx["inputs"])
    inputs["identity_batches"] = [{**batch, "store_id": "z-store"}, batch]
    with pytest.raises(
        ValidationError,
        match=re.escape("Flavor replay identity batches must be sorted and unique"),
    ):
        ReplayContext.model_validate_json(canonical(ctx))
    inputs["identity_batches"] = [batch, {**batch, "store_id": "z-store"}]
    assert ReplayContext.model_validate_json(canonical(ctx)).inputs.kind == "flavor"


def test_foreign_v2_is_structural_but_semantic_entry_refuses(
    intake_case: Case, tmp_path: Path
) -> None:
    root = intake_case.fork(tmp_path / "v2")
    files = copy.deepcopy(intake_case.files)
    inv = object_value(files[INVENTORY])
    inv["template_source_format"] = 2
    ctx = context()
    from sve_carddb.template_semantics.environment import capture  # ruff: ignore[import-outside-top-level] -- complete necessary-package shape
    from sve_carddb.template_semantics.registry import ROOT, manifest  # ruff: ignore[import-outside-top-level] -- finite binding evidence is loaded only for this fixture
    from sve_carddb.template_semantics.versions import REGISTERED  # ruff: ignore[import-outside-top-level] -- no source execution enters the format-only reader

    ctx["semantic_bindings"] = [
        {
            "id": name,
            "revision": REVISION,
            "path": manifest(name)[0],
            "hash": REGISTERED[name][1],
        }
        for name in sorted(
            ("physical-parser-v1", "template-effect-v1", "semantic-output-v1")
        )
    ]
    ctx["environment"] = capture(ROOT, producer=True).model_dump(mode="json")
    inv["replay_context"] = ctx
    revision = write(root, files)
    glossary = load_glossary(root / "authored")
    assert any(path == INVENTORY for path, _, _ in glossary.closure)
    with pytest.raises(
        ValueError,
        match=r"^Pinned immutable dependency unavailable$",
    ):
        load_templates(PinnedRepository(root), revision, intake_case.sources())
    inv["replay_context"] = {"format": 1}
    write(root, files)
    with pytest.raises(ValueError, match=r"^Invalid formal template inventory$"):
        load_glossary(root / "authored")


@pytest.mark.parametrize("packages", [["z", "a"], ["a", "a"]])
def test_manifest_environment_packages_are_sorted_and_unique(
    packages: list[str],
) -> None:
    good: dict[str, JsonValue] = {
        "semantic_version_format": 1,
        "id": "parser-v1",
        "entrypoint": "parser_v1",
        "files": [{"path": "carddb/parser.py", "hash": HASH}],
        "environment_packages": ["a", "z"],
    }
    assert SemanticManifest.model_validate_json(
        canonical(good)
    ).environment_packages == ("a", "z")
    with pytest.raises(ValidationError) as failure:
        SemanticManifest.model_validate_json(
            canonical({**good, "environment_packages": list[JsonValue](packages)})
        )
    assert [e["msg"] for e in failure.value.errors()] == [
        "Value error, Semantic environment packages must be sorted and unique"
    ]


@pytest.mark.parametrize("flavor", [False, True])
def test_flavor_inputs_and_exact_recipe_are_bidirectional(
    wire: dict[str, JsonValue], flavor: bool
) -> None:
    recipe = object_value(array(wire["recipes"])[0])
    if flavor:
        object_value(wire["replay_context"])["inputs"] = {
            "kind": "flavor",
            "source_batch": {"store_id": "test", "batch_id": HASH},
            "identity_basis": None,
            "identity_batches": [],
        }
        item = object_value(array(wire["entries"])[0])
        item.update(
            role="flavor", normalizer_id="flavor-exact-v1", legacy_fingerprint=None
        )
        object_value(item["source_ref"])["locator"] = "/faces/0/flavor"
        recipe["id"] = "flavor-exact-v1"
        InventoryV2.model_validate_json(canonical(wire))
        recipe["id"] = "synthetic-v1"
    else:
        InventoryV2.model_validate_json(canonical(wire))
        recipe["id"] = "flavor-exact-v1"
    with pytest.raises(ValidationError) as failure:
        InventoryV2.model_validate_json(canonical(wire))
    assert [e["msg"] for e in failure.value.errors()] == [
        "Value error, Flavor replay requires its exact empty recipe config"
    ]
