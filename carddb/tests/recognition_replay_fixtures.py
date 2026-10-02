"""One tiny sealed synthetic source closure and pinned adopted concepts per module."""

import shutil
from dataclasses import dataclass
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.manifest import Kind
from sve_carddb.snapshot.values import canonical, digest, object_value
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.template_parameter_rules.loader import Loaded, load_pairs
from sve_carddb.template_parameter_rules.replay import (
    CODE_PATH,
    ProposalInputs,
    _evidence,
)
from sve_carddb.template_parameters.analysis import VERSION_PARAMETERS
from sve_carddb.template_parameters.references import adopted
from sve_carddb.template_sources.models import Recipe
from sve_carddb.template_sources.pins import recipes

from .adoption_fixtures import commit
from .recognition_policy_fixtures import RUNTIME, GitCase, pair, publish
from .test_effect_presence import page
from .test_source_archive import _put, _resource, _store
from .translation_fixtures import envelope, term, write

if TYPE_CHECKING:
    from pathlib import Path


@dataclass(frozen=True)
class SourceCase:
    repository: Path
    main: str
    store: Path
    batch: str
    recipe: Recipe
    loaded: Loaded
    legacy: bytes
    proposals: ProposalInputs


@pytest.fixture(scope="module")
def recognition_source(
    policy_git: GitCase, tmp_path_factory: pytest.TempPathFactory
) -> SourceCase:
    return _source(policy_git, tmp_path_factory, sign_change=False)


@pytest.fixture(scope="module")
def changed_sign_source(
    policy_git: GitCase, tmp_path_factory: pytest.TempPathFactory
) -> SourceCase:
    return _source(policy_git, tmp_path_factory, sign_change=True)


def _source(  # ruff: ignore[too-many-locals] -- independent sealed source closures share the same small fixture builder
    policy_git: GitCase, tmp_path_factory: pytest.TempPathFactory, *, sign_change: bool
) -> SourceCase:
    folder = tmp_path_factory.mktemp("recognition-source")
    repository = folder / "repository"
    shutil.copytree(policy_git.repository, repository)
    shutil.copytree(
        RUNTIME / "carddb/src",
        repository / "carddb/src",
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "_version.py"),
    )
    glossary = term()
    object_value(glossary["data"]).update(
        source_ref=None,
        authored_source_ja="SyntheticTerm",
        missing_source_reason="Synthetic fixture unavailable source",
    )
    write(
        repository / "authored",
        {"translations/glossary/concepts/001.yaml": envelope([glossary])},
    )
    revision = commit(repository)
    source_pins = recipes(repository, revision)
    store = _store(folder / "sources")
    effect = '<div class="detail">{SyntheticClass}甲2枚<br>乙3ダメージ<br>（丙4ダメージ）<br>Gamma『NoConcept』<br>コストを+5試験</div>'
    if sign_change:
        effect = effect.replace("（丙4ダメージ）", "（丙＋4枚）")
    raw = page("jp", effect)
    _put(store, _resource(card_url("SYN-01"), "raw/SYN-01.html", raw, Kind.CARD), raw)
    batch = seal_batch(store)
    legacy = b"".join(
        canonical(
            {
                "template": "T" + digest(text.encode())[7:17],
                "normalized": text,
                "lines": 1,
                "members": [member],
            }
        )
        + b"\n"
        for text, member in (
            ("{SyntheticClass}甲N枚", "SYN-01#0/text/0"),
            ("乙Nダメージ", "SYN-01#0/text/1"),
            ("Gamma『X』", "SYN-01#0/text/3"),
            ("コストを+N試験", "SYN-01#0/text/4"),
        )
    )
    preliminary = Recipe(
        id=VERSION_PARAMETERS,
        code_revision=revision,
        code_path=CODE_PATH,
        code_hash=digest((repository / CODE_PATH).read_bytes()),
        config={},
        config_hash=digest(canonical({})),
    )
    refs = adopted(
        repository / "authored",
        _evidence(
            PinnedRepository(repository), preliminary, {store.store_id: store.root}
        ),
    )
    glossary_pin = object_value(refs.pins["glossary"])
    refs.pins["glossary"] = {**glossary_pin, "authored_revision": revision}
    policy, receipt = pair(
        revision, bridge=True, store=store.store_id, batch=batch.batch_id
    )
    authored_revision = publish(repository, policy, receipt)
    loaded = load_pairs(
        PinnedRepository(repository), authored_revision, main_revision=revision
    )["synthetic-v1"]
    proposals = ProposalInputs(
        canonical(
            {
                "bindings": [
                    {
                        "region": "jp",
                        "kind": "class",
                        "raw": "SyntheticClass",
                        "code": "synthetic",
                        "special_kinds": [],
                    }
                ],
                "terms": [],
            }
        ),
        b"Synthetic proposed vocabulary basis; no adoption",
    )
    refs.pins["vocabulary_proposals_hash"] = digest(proposals.vocabulary)
    refs.pins["vocabulary_basis_hash"] = digest(proposals.basis)
    config: dict[str, JsonValue] = {
        "recognition_policy": loaded.pin.model_dump(mode="json"),
        "source_recipes": [p.model_dump(mode="json") for p in source_pins],
        "source_batch": {"store_id": store.store_id, "batch_id": batch.batch_id},
        "references": refs.pins,
        "legacy_file_hash": digest(legacy),
    }
    recipe = preliminary.model_copy(
        update={"config": config, "config_hash": digest(canonical(config))}
    )
    return SourceCase(
        repository,
        revision,
        store.root,
        batch.batch_id,
        recipe,
        loaded,
        legacy,
        proposals,
    )
