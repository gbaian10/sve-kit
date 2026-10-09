"""One reusable synthetic glossary and three sealed JP pages."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import object_value
from sve_carddb.domains.translations.source_inventory.inventory import (
    Scan,
    scan_current,
)
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import seal_batch
from sve_carddb.parse.pages.official_jp import card_url

from ..domains.text_observations.test_effect_presence import page
from ..ingest.test_source_archive import _put, _resource, _store
from .adoption_fixtures import commit, git
from .translation_fixtures import envelope, term, write

if TYPE_CHECKING:
    from pathlib import Path


@dataclass(frozen=True)
class Case:
    repository: Path
    revision: str
    store: Path
    batch: str
    scan: Scan


@pytest.fixture(scope="module")
def template_case(tmp_path_factory: pytest.TempPathFactory) -> Case:
    root = tmp_path_factory.mktemp("template-sources")
    repository = root / "repository"
    repository.mkdir()
    concept = term()
    data = object_value(concept["data"])
    data.update(
        source_ref=None,
        authored_source_ja="SyntheticTerm",
        missing_source_reason="Synthetic unavailable source",
    )
    write(
        repository / "authored",
        {"translations/glossary/concepts/001.yaml": envelope([concept])},
    )
    git(repository, "init")
    revision = commit(repository)
    store = _store(root / "sources")
    first = '<div class="detail">Alpha２（Synthetic reminder）<br>Beta『Synthetic 9』３<br>-----<br>『Synthetic token』{Synthetic}フォロワー{コスト２}{攻撃力}１/{体力}３Gamma４<br>（Synthetic reminder only）<br>『Header only』{Synthetic}クレスト<br>-----<br>Delta８</div>'
    for number, effect, double in (
        ("SYN-01", first, False),
        ("SYN-02", '<div class="detail">Alpha３</div>', True),
        ("SYN-03", "", False),
    ):
        raw = page("jp", effect, double=double).replace(b"SYN-01", number.encode())
        _put(
            store,
            _resource(card_url(number), f"raw/{number}.html", raw, Kind.CARD),
            raw,
        )
    batch = seal_batch(store)
    scan = scan_current(FrozenSources(store.root, store.store_id, batch.batch_id))
    return Case(
        repository,
        revision,
        store.root,
        batch.batch_id,
        scan,
    )
