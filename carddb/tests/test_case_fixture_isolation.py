"""Copied text and correction inputs retain private providers, plans and raw files."""

from dataclasses import FrozenInstanceError, replace
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from .fixture_files import freeze_files

if TYPE_CHECKING:
    from pathlib import Path

    from .shared_case_fixtures import CorrectionCaseTemplate, TextCaseTemplate


def test_text_copies_isolate_mutable_state_and_files(
    default_text_case: TextCaseTemplate, tmp_path: Path
) -> None:
    first = default_text_case.copy(tmp_path / "first")
    second = default_text_case.copy(tmp_path / "second")
    assert first is not second
    assert first.provider is not second.provider
    assert first.provider.cards is not second.provider.cards
    assert first.plan == second.plan
    expected = dict(second.provider.cards)
    first.provider.cards.clear()
    first.plan = replace(first.plan, groups=())
    next(first.root.rglob("*.yaml")).write_bytes(b"destroyed authored input")
    next((first.store / "raw").rglob("*.raw")).write_bytes(b"destroyed raw input")
    assert second.provider.cards == expected
    assert second.plan.groups
    assert freeze_files(tmp_path / "second") == default_text_case.files
    third = default_text_case.copy(tmp_path / "third")
    assert third.provider.cards == expected
    assert third.plan == second.plan
    assert freeze_files(tmp_path / "third") == default_text_case.files


def test_shared_text_values_reject_nested_mutation(
    default_text_case: TextCaseTemplate, tmp_path: Path
) -> None:
    case = default_text_case.copy(tmp_path)
    card = next(iter(case.provider.cards.values()))
    with pytest.raises(ValidationError, match="frozen"):
        card.source.parser_version = "changed"  # type: ignore[misc]  # exercise immutable nested provenance
    with pytest.raises(FrozenInstanceError):
        case.plan.groups = ()  # type: ignore[misc]  # exercise immutable shared plan
    record = next(iter(case.catalog.records.values()))
    with pytest.raises(TypeError):
        case.catalog.records["changed"] = record  # type: ignore[index]  # exercise immutable shared catalog
    with pytest.raises(FrozenInstanceError):
        default_text_case.plan = replace(case.plan, groups=())  # type: ignore[misc]  # exercise immutable template holder
    assert case.plan.groups
    assert tuple(case.provider.cards.items()) == default_text_case.cards


def test_correction_copies_isolate_image_lookup_and_archive(
    default_correction_case: CorrectionCaseTemplate, tmp_path: Path
) -> None:
    first = default_correction_case.copy(tmp_path / "first")
    second = default_correction_case.copy(tmp_path / "second")
    assert first is not second
    assert first.texts is not second.texts
    assert first.images is not second.images
    assert first.images.versions is not second.images.versions
    assert first.images.sources.entries is not second.images.sources.entries
    assert second.texts.plan.corrections
    evidence = second.texts.plan.corrections[0].data.evidence[0]
    expected = second.images.image(evidence)
    first.images.versions.clear()
    first.images.sources.entries.clear()
    next((first.image_store / "raw").rglob("*.raw")).write_bytes(b"broken image")
    first.texts.provider.cards.clear()
    assert second.images.image(evidence) == expected
    assert second.texts.provider.cards
    assert freeze_files(tmp_path / "second") == default_correction_case.texts.files
    third = default_correction_case.copy(tmp_path / "third")
    assert third.images.image(evidence) == expected
    assert freeze_files(tmp_path / "third") == default_correction_case.texts.files
