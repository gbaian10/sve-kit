"""Current-tree reads and full source validation have separate responsibilities."""

import copy
import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import array, object_value
from sve_carddb.template_translations.loader import read_templates, validate_templates

from .template_intake_fixtures import (
    DEFINITIONS,
    INVENTORY,
    intake_case,
    policy_git,
    recognition_term_source,
    shard,
    write,
)

if TYPE_CHECKING:
    from pathlib import Path

    from pytest_mock import MockerFixture

    from .template_intake_fixtures import Case

__all__ = ("intake_case", "policy_git", "recognition_term_source")


def test_read_does_not_require_sources_or_history(
    intake_case: Case, mocker: MockerFixture
) -> None:
    mocker.patch(
        "sve_carddb.template_translations.loader.immutable",
        side_effect=AssertionError("Ordinary reads must not traverse history"),
    )
    mocker.patch.object(
        intake_case.replay_sources,
        "reconstruct",
        side_effect=AssertionError("Ordinary reads must not read raw sources"),
    )
    inputs = read_templates(
        PinnedRepository(intake_case.repository), intake_case.revision
    )
    assert inputs.records() == intake_case.verified.records()
    assert (
        inputs.effective_translations() == intake_case.verified.effective_translations()
    )
    assert not hasattr(inputs, "source_reports")


def test_bad_source_is_refused_by_full_validation(
    intake_case: Case, tmp_path: Path
) -> None:
    root = intake_case.fork(tmp_path / "bad-source")
    inventory = copy.deepcopy(intake_case.files[INVENTORY])
    entries = array(object_value(inventory)["entries"])
    object_value(entries[0])["normalized_hash"] = "sha256:" + "0" * 64
    revision = write(root, {INVENTORY: inventory})
    inputs = read_templates(PinnedRepository(root), revision)
    with pytest.raises(
        ValueError,
        match="^"
        + re.escape("Formal template entry differs from its exact frozen replay")
        + "$",
    ):
        validate_templates(inputs, intake_case.sources())


def test_bad_slot_bounds_are_refused_by_full_validation(
    intake_case: Case, tmp_path: Path
) -> None:
    root = intake_case.fork(tmp_path / "bad-slot")
    records = [copy.deepcopy(record) for record in intake_case.definitions]
    numeric = next(
        record
        for record in records
        if any(
            object_value(slot)["type"] == "uint"
            for slot in array(
                object_value(object_value(record["data"])["parameter_schema"])["slots"]
            )
        )
    )
    slots = array(
        object_value(object_value(numeric["data"])["parameter_schema"])["slots"]
    )
    for slot in slots:
        if object_value(slot)["type"] == "uint":
            object_value(slot)["max"] = 1
    files = copy.deepcopy(intake_case.files)
    files[DEFINITIONS] = shard(records)
    revision = write(root, files)
    inputs = read_templates(PinnedRepository(root), revision)
    with pytest.raises(
        ValueError,
        match="^"
        + re.escape("Template numeric bounds differ from the recognized role")
        + "$",
    ):
        validate_templates(inputs, intake_case.sources())
