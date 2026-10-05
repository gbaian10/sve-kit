"""One complete synthetic JP flavor/identity closure per module; no private data."""

from dataclasses import dataclass
from datetime import date
from typing import TYPE_CHECKING

import pytest

from sve_carddb.extract.compare_jp import legacy_projection
from sve_carddb.extract.official_jp import extract_card
from sve_carddb.manifest import Kind
from sve_carddb.registry.build import build as build_registry
from sve_carddb.registry.inputs import Mapping
from sve_carddb.registry.review import InitDecisions, Inputs
from sve_carddb.registry.storage import plan_files, write_files
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources.official_jp import card_url

from .adoption_fixtures import commit, git
from .test_registry_preview_archive import RAW
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path


@dataclass(frozen=True)
class Case:
    repository: Path
    store: Path
    batch: str


@pytest.fixture(scope="module")
def flavor_case(tmp_path_factory: pytest.TempPathFactory) -> Case:
    folder = tmp_path_factory.mktemp("flavor-intake")
    repository = folder / "repository"
    repository.mkdir()
    git(repository, "init", "-b", "main")
    store = _store(folder / "sources")
    jp = {}
    for index, paragraph in enumerate(
        (
            "甲２<br>（乙）<br><br>丙🌱",
            "甲２<br>（乙）<br><br>丙🌱",
            "甲３『乙』",
            None,
            "",
        ),
        1,
    ):
        number = f"SYN-{index:03}"
        raw = RAW.replace(
            b'<div class="speech"></div>',
            b""
            if paragraph is None
            else ('<div class="speech">' + paragraph + "</div>").encode(),
        )
        _put(
            store,
            _resource(card_url(number), f"raw/{number}.html", raw, Kind.CARD),
            raw,
        )
        jp[number] = legacy_projection(extract_card(raw, number=number))
    batch = seal_batch(store)
    inputs = Inputs(
        jp=jp,
        en={},
        mapping=Mapping(targets={}, original_art=set(), reskins={}),
        decisions=InitDecisions(
            separate_groups={"jp:" + number: number for number in jp},
        ),
        as_of=date(2026, 9, 28),
        jp_hash=digest(b"synthetic flavor"),
    )
    write_files(plan_files(repository / "authored", build_registry(inputs, {})))
    index_file = repository / "authored/translations/index.yaml"
    index_file.parent.mkdir(parents=True)
    index_file.write_bytes(
        canonical(
            {
                "translation_authored_format": 2,
                "kind": "translation_index",
                "includes": {},
                "inventories": {},
            }
        )
    )
    commit(repository)
    return Case(repository, store.root, batch.batch_id)
