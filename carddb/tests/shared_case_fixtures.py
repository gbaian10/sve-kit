"""Immutable default text inputs with fresh mutable consumers and sealed bytes."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

import pytest

from sve_carddb.source_corrections import FrozenImages

from .fixture_files import FrozenFiles, freeze_files, restore_files
from .source_correction_fixtures import CorrectionCase, make_correction_case
from .test_registry import make_inputs
from .text_observation_fixtures import Case, MemoryTexts, make_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.products.loader import ProductSnapshot
    from sve_carddb.registry.preview import PreviewPlan
    from sve_carddb.registry.records import Region
    from sve_carddb.text_observations import TextCard, TextPlan, Vocabulary


@dataclass(frozen=True)
class TextCaseTemplate:
    files: FrozenFiles
    root: Path
    store: Path
    identity: PreviewPlan
    cards: tuple[tuple[tuple[Region, str], TextCard], ...]
    plan: TextPlan
    catalog: ProductSnapshot
    vocabulary: Vocabulary

    @classmethod
    def capture(cls, case: Case, base: Path) -> TextCaseTemplate:
        return cls(
            freeze_files(base),
            case.root.relative_to(base),
            case.store.relative_to(base),
            case.identity,
            tuple(case.provider.cards.items()),
            case.plan,
            case.catalog,
            case.vocabulary,
        )

    def copy(self, destination: Path) -> Case:
        restore_files(self.files, destination)
        return Case(
            destination / self.root,
            destination / self.store,
            self.identity,
            MemoryTexts(dict(self.cards)),
            self.plan,
            self.catalog,
            self.vocabulary,
        )


@dataclass(frozen=True)
class CorrectionCaseTemplate:
    texts: TextCaseTemplate
    image_store: Path
    store_id: str
    batch_id: str

    def copy(self, destination: Path) -> CorrectionCase:
        texts = self.texts.copy(destination)
        image_store = destination / self.image_store
        return CorrectionCase(
            texts, FrozenImages(image_store, self.store_id, self.batch_id), image_store
        )


@pytest.fixture(scope="session")
def default_text_case(tmp_path_factory: pytest.TempPathFactory) -> TextCaseTemplate:
    base = tmp_path_factory.mktemp("text-template")
    return TextCaseTemplate.capture(make_case(base / "authored", make_inputs()), base)


@pytest.fixture(scope="session")
def default_correction_case(
    tmp_path_factory: pytest.TempPathFactory,
) -> CorrectionCaseTemplate:
    base = tmp_path_factory.mktemp("correction-template")
    case = make_correction_case(base, make_inputs())
    return CorrectionCaseTemplate(
        TextCaseTemplate.capture(case.texts, base),
        case.image_store.relative_to(base),
        case.images.sources.store_id,
        case.images.sources.batch_id,
    )
