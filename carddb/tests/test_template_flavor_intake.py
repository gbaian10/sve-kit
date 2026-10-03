"""Exact source, owner and human-adoption boundaries for narrative template intake."""

import copy
import json
import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.template_parameters.models import Range, Schema, Slot
from sve_carddb.template_translations.loader import load_templates
from sve_carddb.template_translations.sources import TemplateSources
from sve_carddb.template_translations.text import verify_flavor
from sve_carddb.translations.loader import load_glossary

from .template_flavor_fixtures import Case, flavor_case
from .template_intake_fixtures import DEFINITIONS, INVENTORY, TRANSLATIONS, shard, write

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

__all__ = ("flavor_case",)


def exact(message: str) -> str:
    return "^" + re.escape(message) + "$"


def test_complete_frozen_flavor_intake_and_shared_paragraphs(
    flavor_case: Case, tmp_path: Path
) -> None:
    case = flavor_case
    root = case.fork(tmp_path / "adopted")
    revision = write(root, copy.deepcopy(case.files))
    result = load_templates(PinnedRepository(root), revision, case.sources)
    assert sorted(n for _, n in result.frequencies) == [1, 2]
    assert len(result.effective_translations()) == 2
    assert not load_glossary(root / "authored").records()
    assert len(load_glossary(root / "authored").closure) == 3
    coverage = object_value(parse(case.replay.source_coverage))
    assert coverage["complete"] is False
    assert coverage["field_states"] == {"present": 3, "unknown": 1, "empty": 1}
    assert coverage["owner_checked_rows"] == 3
    assert coverage["missing_owner_rows"] == 0
    assert len(array(coverage["identity_files"])) > 0
    assert len(array(coverage["runtime_dependencies"])) > 20
    checkpoint = object_value(parse(case.replay.checkpoint))
    assert checkpoint == {
        "legacy_checkpoint_applicable": False,
        "paragraph_uses": 3,
        "distinct_paragraphs": 2,
    }
    same = [m for m in case.replay.entries if "🌱" in m.normalized]
    assert len(same) == 2
    assert same[0].normalized == "甲２\n（乙）\n\n丙🌱"
    assert same[0].owner is not None
    assert same[1].owner is not None
    assert same[0].owner != same[1].owner
    assert same[0].owner.flavor_unit_id == same[1].owner.flavor_unit_id
    assert len({m.entry.id for m in case.replay.entries}) == 3
    assert all(m.candidate.legacy_id is None for m in case.replay.entries)
    assert all(
        not m.hints and not m.roles and not m.pending for m in case.replay.entries
    )


@pytest.mark.parametrize(
    "change", ["missing_parser", "duplicate", "config", "hash", "program", "revision"]
)
def test_flavor_requires_independent_closed_recipe_pins(
    flavor_case: Case, change: str
) -> None:
    pins = list(flavor_case.pins)
    if change == "missing_parser":
        pins.pop()
    elif change == "duplicate":
        pins.append(pins[0])
    else:
        updates: dict[str, object] = {
            "config": {"trim": False},
            "hash": {"code_hash": "sha256:" + "0" * 64},
            "program": {
                "code_path": "carddb/src/sve_carddb/template_sources/normalizer.py"
            },
            "revision": {"code_revision": "0" * 40},
        }
        update = (
            {
                "config": updates["config"],
                "config_hash": digest(canonical({"trim": False})),
            }
            if change == "config"
            else updates[change]
        )
        assert isinstance(update, dict)
        pins[0] = pins[0].model_copy(update=update)
    message = (
        "Recognition immutable Git history is unavailable"
        if change == "revision"
        else "Flavor requires its exact independent recipe and parser pins"
    )
    with pytest.raises(ValueError, match=exact(message)):
        flavor_case.sources.reconstruct(tuple(pins))


def test_flavor_requires_explicit_batch_and_identity_before_adoption(
    flavor_case: Case,
) -> None:
    case = flavor_case
    missing = TemplateSources(
        PinnedRepository(case.repository),
        {"test-store": case.store},
        main_revision=case.prior,
        legacy_bytes=b"",
    )
    with pytest.raises(
        ValueError,
        match=exact("Flavor replay requires an explicit available frozen batch"),
    ):
        missing.reconstruct(case.pins)
    assert case.sources.flavor is not None
    candidate = TemplateSources(
        PinnedRepository(case.repository),
        {"test-store": case.store},
        main_revision=case.prior,
        legacy_bytes=b"",
        flavor=case.sources.flavor.model_copy(update={"identity_basis": None}),
    )
    replay = candidate.reconstruct(case.pins)
    assert len(replay.entries) == 3
    assert all(
        m.owner is None and m.pending == ("missing_flavor_identity_owner",)
        for m in replay.entries
    )
    with pytest.raises(
        ValueError,
        match=exact("Template definition source has unresolved parameter roles"),
    ):
        replay.entries[0].verify_schema(Schema(slots=()))


@pytest.mark.parametrize(
    ("change", "message"),
    [
        (
            "missing_entry",
            "Formal template inventory must cover every frozen batch entry",
        ),
        (
            "extra_entry",
            "Formal template inventory must cover every frozen batch entry",
        ),
        ("hash", "Formal template entry differs from its exact frozen replay"),
        ("locator", "Formal template entry differs from its exact frozen replay"),
        ("span", "Template definition span must match its exact inventory part"),
        (
            "old_recipe",
            "Template definition language normalizer or semantic variant is unsupported",
        ),
        (
            "old_id",
            "Template ID differs from its legacy fingerprint or allocated payload hash",
        ),
        ("slot", "Flavor template schema must have zero parameters"),
        (
            "parent",
            "Template supersedes requires an adopted parent or its verified legacy family",
        ),
        ("review", "Template model review must pin the final exact text hash"),
        ("foreign_envelope", "Template decision exact membership mismatch"),
    ],
)
def test_formal_flavor_source_and_adoption_guards(  # ruff: ignore[complex-structure,too-many-branches] -- each prospective batch mutation targets one downstream guard
    flavor_case: Case, tmp_path: Path, change: str, message: str
) -> None:
    case = flavor_case
    root = case.fork(tmp_path / change)
    files = copy.deepcopy(case.files)
    inventory = object_value(files[INVENTORY])
    entries = array(inventory["entries"])
    if change == "missing_entry":
        entries.pop()
    elif change == "extra_entry":
        extra = object_value(copy.deepcopy(entries[0]))
        extra["id"] = "inv:" + "f" * 64
        entries.append(extra)
        entries.sort(key=lambda e: str(object_value(e)["id"]))
    elif change in {"hash", "locator"}:
        first = object_value(entries[0])
        if change == "hash":
            first["normalized_hash"] = "sha256:" + "f" * 64
        else:
            object_value(first["source_ref"])["locator"] = "/faces/0/text"
    else:
        area = TRANSLATIONS if change == "review" else DEFINITIONS
        records = [object_value(r) for r in array(object_value(files[area])["records"])]
        data = object_value(records[0]["data"])
        if change == "span":
            object_value(array(object_value(data["source_span"])["segments"])[0])[
                "end"
            ] = 1
        elif change == "old_recipe":
            data["normalizer_version"] = "template-parameters-jp-candidate-v1"
        elif change == "old_id":
            data["id"] = "T" + "0" * 10
            records[0]["record_key"] = canonical(
                ["sentence_template", data["id"]]
            ).decode()
        elif change == "slot":
            object_value(data["parameter_schema"])["slots"] = [
                Slot(
                    name="amount",
                    type="uint",
                    occurrences=(Range(start=0, end=1),),
                    reference_kind=None,
                    min=0,
                    max=9007199254740991,
                ).model_dump(mode="json")
            ]
        elif change == "parent":
            data["supersedes_id"] = "T" + "0" * 10
        elif change == "review":
            object_value(data["model_review"])["text_hash"] = "sha256:" + "f" * 64
        files[area] = shard(records)
    if change == "foreign_envelope":
        object_value(array(object_value(files[DEFINITIONS])["decisions"])[0])[
            "membership_hash"
        ] = "sha256:" + "f" * 64
    revision = write(root, files)
    target = (
        (lambda: load_glossary(root / "authored"))
        if change == "foreign_envelope"
        else (lambda: load_templates(PinnedRepository(root), revision, case.sources))
    )
    with pytest.raises(ValueError, match=exact(message)):
        target()


@pytest.mark.parametrize(
    "text",
    [
        "\r甲",
        "甲\r\n乙",
        " 甲",
        "甲\n",
        "甲\t",
        "甲\n乙\u3000",
        "甲 \n乙",
        "甲\t\n乙",
        "甲\u00a0\n乙",
    ],
)
def test_flavor_final_wire_and_display_whitespace_are_refused(text: str) -> None:
    message = (
        "Flavor translation nonempty lines cannot end in whitespace"
        if "\n乙" in text and "\r" not in text and not text.endswith("\u3000")
        else "Flavor translation must be nonempty LF text without outer whitespace"
    )
    with pytest.raises(ValueError, match=exact(message)):
        verify_flavor(text, Schema(slots=()))


def test_flavor_final_escaped_literal_and_blank_internal_line() -> None:
    verify_flavor("甲\\{乙\\}\\\\\n\n丙", Schema(slots=()))
    with pytest.raises(
        ValueError, match=exact("Template parameter is not declared by its schema")
    ):
        verify_flavor("{{unknown}}", Schema(slots=()))
    with pytest.raises(
        ValueError, match=exact("Template literal braces must be escaped")
    ):
        verify_flavor("甲{乙}", Schema(slots=()))


@pytest.mark.parametrize("value", [False, 0.0, "0"])
def test_flavor_inventory_ordinal_is_not_coerced(
    flavor_case: Case, value: JsonValue
) -> None:
    data = copy.deepcopy(flavor_case.files[INVENTORY])
    object_value(array(object_value(data)["entries"])[0])["line_ordinal"] = value
    from sve_carddb.template_translations.loader import _inventory  # ruff: ignore[import-outside-top-level] -- safe boundary message rather than Pydantic input echo

    with pytest.raises(ValueError, match=exact("Invalid formal template inventory")):
        _inventory(json.dumps(data).encode())


@pytest.mark.parametrize("change", ["extra", "changed", "removed", "symlink"])
def test_exact_flavor_runtime_closure_is_pinned(
    flavor_case: Case, tmp_path: Path, change: str
) -> None:
    from sve_carddb.template_translations.flavor_pins import recipes  # ruff: ignore[import-outside-top-level] -- isolate the runtime closure check

    from .adoption_fixtures import commit  # ruff: ignore[import-outside-top-level] -- synthetic Git only

    root = flavor_case.fork(tmp_path / change)
    path = root / "carddb/src/sve_carddb/template_sources/flavor.py"
    if change == "extra":
        (path.parent / "extra_synthetic.py").write_text(
            "# Synthetic additional dependency.\n"
        )
    elif change in {"removed", "symlink"}:
        path.unlink()
        if change == "symlink":
            path.symlink_to("normalizer.py")
    else:
        path.write_bytes(path.read_bytes() + b"\n# Synthetic changed implementation.\n")
    revision = commit(root)
    message = (
        "Flavor historical runtime cannot be replayed"
        if change == "changed"
        else "Flavor runtime module closure differs from pinned Git"
    )
    if change == "symlink":
        message = "Flavor runtime modules must be regular Git files"
    with pytest.raises(ValueError, match=exact(message)):
        recipes(PinnedRepository(root), revision)


@pytest.mark.parametrize(
    "guard", ["batches", "registry_hash", "observation", "unreachable"]
)
def test_physical_identity_basis_is_independent_of_paragraph_hash(
    flavor_case: Case, tmp_path: Path, guard: str
) -> None:
    from sve_carddb.registry.storage import (  # ruff: ignore[import-outside-top-level] -- reseal a valid registry before testing source replay
        _area,
        load,
        read_yaml,
        relayout,
        write_files,
    )

    from .adoption_fixtures import commit, git  # ruff: ignore[import-outside-top-level] -- isolated synthetic authored changes

    case = flavor_case
    root = case.fork(tmp_path / guard)
    assert case.sources.flavor is not None
    inputs = case.sources.flavor
    basis = case.basis
    main = case.prior
    if guard == "batches":
        inputs = inputs.model_copy(update={"identity_batches": ()})
        message = "Flavor identity replay requires its complete explicit source batches"
    elif guard == "registry_hash":
        basis = basis.model_copy(update={"registry_index_hash": "sha256:" + "f" * 64})
        message = "Name identity registry index hash mismatch"
    elif guard == "unreachable":
        git(root, "checkout", "--orphan", "synthetic-other")
        revision = commit(root)
        basis = basis.model_copy(update={"authored_revision": revision})
        message = "Recognition matcher commit must be reachable from pinned main"
    else:
        _, records = load(root / "authored")
        for entry in records.values():
            if entry.kind in {"printing", "art"}:
                object_value(entry.data["observation"])["observation_hash"] = (
                    "sha256:" + "0" * 64
                )
        reviews = {
            (_area(e), e.owner): ("Synthetic human", "2026-10-03")
            for e in records.values()
        }
        write_files(relayout(root / "authored", list(records.values()), reviews))
        main = commit(root)
        basis = basis.model_copy(
            update={
                "authored_revision": main,
                "registry_index_hash": digest(
                    canonical(read_yaml(root / "authored/ids/index.yaml"))
                ),
            }
        )
        message = "Name identity complete frozen observation closure is absent"
    inputs = inputs.model_copy(update={"identity_basis": basis})
    sources = TemplateSources(
        PinnedRepository(root),
        {"test-store": case.store},
        main_revision=main,
        legacy_bytes=b"",
        flavor=inputs,
    )
    with pytest.raises(ValueError, match=exact(message)):
        sources.reconstruct(case.pins)


@pytest.mark.parametrize("text", ["甲\r\n乙", "甲 \n乙", "甲\n", "甲{乙}"])
def test_loader_validates_final_flavor_text_after_matching_review_hash(
    flavor_case: Case, tmp_path: Path, text: str
) -> None:
    root = flavor_case.fork(tmp_path / "final-text")
    files = copy.deepcopy(flavor_case.files)
    records = [
        object_value(r) for r in array(object_value(files[TRANSLATIONS])["records"])
    ]
    data = object_value(records[0]["data"])
    data["text"] = text
    object_value(data["model_review"])["text_hash"] = digest(text.encode())
    files[TRANSLATIONS] = shard(records)
    revision = write(root, files)
    message = (
        "Template literal braces must be escaped"
        if "{" in text
        else "Flavor translation nonempty lines cannot end in whitespace"
        if text == "甲 \n乙"
        else "Flavor translation must be nonempty LF text without outer whitespace"
    )
    with pytest.raises(ValueError, match=exact(message)):
        load_templates(PinnedRepository(root), revision, flavor_case.sources)
