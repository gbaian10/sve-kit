"""Actual frozen EN evidence passes through the existing identity and F1 gates."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.regions import SourceRegion as Region
from sve_carddb.domains.registry.build import build
from sve_carddb.domains.registry.parser_adapters.official_en import (
    legacy_projection as legacy_en,
)
from sve_carddb.domains.registry.parser_adapters.official_jp import (
    legacy_projection as legacy_jp,
)
from sve_carddb.domains.registry.preview import (
    FrozenEN,
    FrozenJP,
    FrozenRegions,
    import_preview,
    plan_preview,
)
from sve_carddb.domains.registry.records import CorrectionData, PrintingData
from sve_carddb.domains.registry.review import Correction
from sve_carddb.domains.registry.storage import plan_files, write_files
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import seal_batch
from sve_carddb.parse.pages import official_en as en
from sve_carddb.parse.pages import official_jp as jp
from sve_carddb.parse.pages.extract_en import extract_card as extract_en
from sve_carddb.parse.pages.extract_jp import extract_card as extract_jp

from .en_extract_fixtures import page
from .registry_preview_fixtures import BUILD, REVISION, parents
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- expose synthetic fixture dependency
from .test_registry_preview_archive import RAW as JP_RAW
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.regions import Region as RegistryRegion
    from sve_carddb.domains.registry.preview.evidence import CardEvidence
    from sve_carddb.domains.registry.review import Inputs


@pytest.fixture
def frozen_registry(tmp_path: Path, inputs: Inputs) -> tuple[Path, FrozenRegions]:
    store = _store(tmp_path)
    for number in inputs.jp:
        raw = JP_RAW.replace(b"TEST-001", number.encode())
        inputs.jp[number] = legacy_jp(extract_jp(raw, number=number))
        _put(
            store,
            _resource(jp.card_url(number), f"raw/{number}.html", raw, Kind.CARD),
            raw,
        )
    for number in inputs.en:
        raw = (
            page(number)
            .replace(b"Synthetic class", b"Forestcraft")
            .replace(b"Synthetic type", b"Follower")
            .replace(b"Alpha / Beta", b"-")
            .replace(b">-</div>", b">1</div>")
            .replace(b">02</div>", b">1</div>")
            .replace(b">X</div>", b">1</div>")
        )
        inputs.en[number] = legacy_en(extract_en(raw, number=number))
        _put(
            store,
            replace(
                _resource(en.card_url(number), f"raw/{number}.html", raw, Kind.CARD),
                region=Region.EN,
            ),
            raw,
        )
    inputs.decisions.corrections = [
        Correction(
            region="en",
            card_no="BP02-070EN",
            field="effect",
            expected_raw_value=inputs.en["BP02-070EN"].faces[0].text or "",
            corrected_value="Synthetic correction",
            image_sha256="sha256:" + "1" * 64,
            locator="effect",
            state="active",
            reason="Synthetic source correction",
        )
    ]
    root = tmp_path / "authored"
    write_files(plan_files(root, build(inputs, {})))
    sealed = seal_batch(store)
    return root, FrozenRegions(
        jp=FrozenJP(
            store.root, store.store_id, sealed.batch_id, parser_version="jp-pin"
        ),
        en=FrozenEN(
            store.root, store.store_id, sealed.batch_id, parser_version="en-pin"
        ),
    )


def test_verified_en_import_preserves_history_usage_and_text_review_block(
    frozen_registry: tuple[Path, FrozenRegions],
) -> None:
    root, provider = frozen_registry
    before = {path: path.read_bytes() for path in root.rglob("*.yaml")}
    plan = plan_preview(root, provider, regions=("jp", "en"))
    adopted = {
        record.data.card_no
        for record in plan.included("printing")
        if isinstance(record.data, PrintingData)
    }
    assert adopted == {"BP02-071", "PR-001", "BP02-070EN", "GF01-001EN"}
    excluded = next(
        p
        for p in plan.projections
        if plan.snapshot.records[p.record_key].kind == "region_mapping_review"
    )
    assert excluded.disposition == "excluded"
    assert "missing_review_coverage" in excluded.reasons
    assert all(check.status == "matched" for check in excluded.evidence)
    uses = plan.source_uses()
    assert len(uses) == 4
    assert {use.source.parser_version for use in uses} == {"jp-pin", "en-pin"}
    assert all(use.usage == "registry_observation" for use in uses)
    assert any('"card_no":"GF01-001EN"' in use.locator for use in uses)
    corrections = [
        p
        for p in plan.projections
        if isinstance(plan.snapshot.records[p.record_key].data, CorrectionData)
    ]
    assert len(corrections) == 1
    assert corrections[0].disposition == "deferred"
    with create_database(compile_build(("en", "related"))) as db:
        parents(db, plan)
        record = import_preview(db, plan, build=BUILD, authored_revision=REVISION)
        assert frozenset(record.uses) == frozenset(uses)
        raws = [
            row.values
            for row in db.rows("source_record")
            if row.values["kind"] == "official_page"
        ]
        assert len(raws) == 4
        assert all(row["parser_version"] is None for row in raws)
        assert db.rows("region_text_review") == ()
        assert all(
            row.values["printed_text_state"] == "unknown"
            for row in db.rows("printing_face")
        )
    assert before == {path: path.read_bytes() for path in before}


@pytest.mark.parametrize("field", ["observation_hash", "rules_hash"])
def test_each_independent_hash_mismatch_blocks_en_adoption_and_keeps_the_use(
    frozen_registry: tuple[Path, FrozenRegions],
    monkeypatch: pytest.MonkeyPatch,
    field: str,
) -> None:
    root, provider = frozen_registry
    original = provider.en.card

    def changed(region: RegistryRegion, number: str) -> CardEvidence | None:
        evidence = original(region, number)
        if evidence is not None and number == "BP02-070EN":
            return replace(
                evidence,
                observation=evidence.observation.model_copy(
                    update={field: "sha256:" + "0" * 64}
                ),
            )
        return evidence

    monkeypatch.setattr(provider.en, "card", changed)
    plan = plan_preview(root, provider, regions=("jp", "en"))
    row = next(
        p
        for p in plan.projections
        if isinstance((data := plan.snapshot.records[p.record_key].data), PrintingData)
        and data.card_no == "BP02-070EN"
    )
    assert row.disposition == "excluded"
    assert any(check.status == "observation_mismatch" for check in row.evidence)
    assert any('"card_no":"BP02-070EN"' in use.locator for use in plan.source_uses())
