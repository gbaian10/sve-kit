"""One reusable synthetic Git runtime and three sealed JP pages."""

import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.manifest import Kind
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.template_sources.checkpoint import LegacyTemplate, read_legacy
from sve_carddb.template_sources.inventory import Scan, scan_batch
from sve_carddb.template_sources.pins import recipes

from .adoption_fixtures import commit, git
from .test_effect_presence import page
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from sve_carddb.template_sources.models import Recipe


@dataclass(frozen=True)
class Case:
    repository: Path
    revision: str
    store: Path
    batch: str
    pins: tuple[Recipe, ...]
    legacy_path: Path
    legacy: tuple[LegacyTemplate, ...]
    scan: Scan


@pytest.fixture(scope="module")
def template_case(tmp_path_factory: pytest.TempPathFactory) -> Case:
    root = tmp_path_factory.mktemp("template-sources")
    runtime = Path(__file__).resolve().parents[2]
    repository = root / "repository"
    shutil.copytree(
        runtime / "carddb/src",
        repository / "carddb/src",
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "_version.py"),
    )
    for name in ("carddb/uv.lock", "carddb/pyproject.toml"):
        shutil.copyfile(runtime / name, repository / name)
    git(repository, "init")
    revision = commit(repository)
    pins = recipes(repository, revision)
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
    legacy_path = root / "legacy.jsonl"
    definitions = (
        ("AlphaN", ["SYN-01#0/text/0", "SYN-02#0/text/0", "SYN-02#1/text/0"]),
        ("Beta『X』N", ["SYN-01#0/text/1"]),
        ("GammaN", ["SYN-01#0/section:0/token:Synthetic token/0"]),
    )
    legacy_path.write_bytes(
        b"".join(
            canonical(
                {
                    "template": "T" + digest(normalized.encode())[7:17],
                    "normalized": normalized,
                    "lines": len(members),
                    "members": list[JsonValue](members),
                }
            )
            + b"\n"
            for normalized, members in definitions
        )
    )
    scan = scan_batch(
        FrozenSources(store.root, store.store_id, batch.batch_id),
        repository=repository,
        pins=pins,
    )
    return Case(
        repository,
        revision,
        store.root,
        batch.batch_id,
        pins,
        legacy_path,
        read_legacy(legacy_path),
        scan,
    )
