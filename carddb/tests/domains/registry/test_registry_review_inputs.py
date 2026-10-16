"""Permanent review pins restore only the historical scope accepted by the registry."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.registry.preview import (
    FrozenRegions,
    import_preview,
    plan_preview,
)
from sve_carddb.domains.registry.preview.review_inputs import (
    RegistryReviewInputs,
    ReviewInputPin,
)
from sve_carddb.domains.registry.records import MappingReviewData
from sve_carddb.workflows.offline import build

from ...support.registry_preview_fixtures import BUILD, REVISION, parents
from ...support.registry_snapshot_fixtures import edit_record
from ...workflows.test_snapshot_offline import prepared as prepared  # ruff: ignore[useless-import-alias] -- full offline build fixture
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- dependency of the frozen registry
from .test_registry_preview_en_sources import frozen_registry as frozen_registry  # ruff: ignore[useless-import-alias] -- share real synthetic frozen HTML evidence

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.domains.card_extras import CardPage
    from sve_carddb.workflows.offline import Inputs

    from ...support.text_observation_fixtures import Case


@pytest.fixture
def pinned_registry(
    tmp_path: Path,
    frozen_registry: tuple[Path, FrozenRegions],
    request: pytest.FixtureRequest,
) -> tuple[Path, FrozenRegions, ReviewInputPin]:
    root, provider = frozen_registry
    batch = tmp_path / "permanent" / "synthetic-reviewed-v1"
    batch.mkdir(parents=True)
    numbers = getattr(request, "param", ("BP02-071", "PR-001"))
    raw = b"".join(canonical({"number": number}) + b"\n" for number in numbers)
    (batch / "jp.jsonl").write_bytes(raw)
    inventory = canonical(
        {
            "predecessor": "/unavailable/private/predecessor",
            "files": {
                "jp.jsonl": {
                    "sha256": digest(raw),
                    "bytes": len(raw),
                    "copied_from": "/unavailable/private/copy",
                }
            },
        }
    )
    (batch / "inventory.json").write_bytes(inventory)
    (batch / "SHA256SUMS").write_text(
        digest(raw).removeprefix("sha256:") + "  jp.jsonl\n"
    )
    pin = ReviewInputPin(
        root=batch.parent,
        batch_id=batch.name,
        inventory_sha256=digest(inventory),
        coverage_hash=digest(raw),
        bytes=len(raw),
        as_of="2026-09-28",
    )
    edit_record(
        root,
        "region_mapping_review",
        lambda entry: entry.data.update(coverage_hash=pin.coverage_hash),
    )
    return root, provider, pin


def test_complete_pin_imports_review_with_original_date_and_no_fake_raw(
    pinned_registry: tuple[Path, FrozenRegions, ReviewInputPin],
) -> None:
    root, raw_provider, pin = pinned_registry
    provider = FrozenRegions(
        jp=raw_provider.jp, en=raw_provider.en, review_inputs=RegistryReviewInputs(pin)
    )
    plan = plan_preview(root, provider, regions=("en", "jp"))
    assert len(plan.included("region_mapping_review")) == 1
    assert plan.report()["registry_review_inputs"] == pin.configuration()
    assert plan.report()["uncovered_jp_card_numbers"] == []
    report = canonical(plan.report())
    assert str(pin.root).encode() not in report
    assert b"/unavailable/private" not in report
    with create_database(compile_build(("en", "related"))) as db:
        parents(db, plan)
        import_preview(db, plan, build=BUILD, authored_revision=REVISION)
        assert db.rows("region_mapping_review")[0].values["as_of"] == "2026-09-28"
        assert (
            len(
                [
                    r
                    for r in db.rows("source_record")
                    if r.values["kind"] == "official_page"
                ]
            )
            == 4
        )


def test_missing_pin_keeps_review_excluded(
    pinned_registry: tuple[Path, FrozenRegions, ReviewInputPin],
) -> None:
    root, provider, _ = pinned_registry
    plan = plan_preview(root, provider, regions=("en", "jp"))
    review = next(
        p for p in plan.projections if p.record_key.startswith("region_mapping_review:")
    )
    assert review.reasons == ("missing_review_coverage",)
    assert not plan.included("region_mapping_review")


def test_bad_pin_aborts_the_entire_offline_build(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    pinned_registry: tuple[Path, FrozenRegions, ReviewInputPin],
) -> None:
    _, recipe, _ = prepared
    _, _, pin = pinned_registry
    (pin.root / pin.batch_id / "jp.jsonl").write_bytes(b'{"number":"TAMPERED"}\n')
    with pytest.raises(ValueError, match="Registry review JP bytes or hash mismatch"):
        build(recipe.model_copy(update={"registry_review_inputs": pin}))


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("target_region", "en"),
        ("coverage_scope", "Partial Japanese scope"),
        ("as_of", "2026-10-01"),
    ],
)
def test_review_region_scope_and_date_must_match(
    pinned_registry: tuple[Path, FrozenRegions, ReviewInputPin],
    field: str,
    value: JsonValue,
) -> None:
    root, provider, pin = pinned_registry
    review_inputs = RegistryReviewInputs(pin)
    plan = plan_preview(root, provider, regions=("en", "jp"))
    review = next(
        r.data
        for r in plan.snapshot.records.values()
        if isinstance(r.data, MappingReviewData)
    )
    changed = MappingReviewData.model_validate(review.model_dump() | {field: value})
    assert not FrozenRegions(
        jp=provider.jp, en=provider.en, review_inputs=review_inputs
    ).coverage(changed)


@pytest.mark.parametrize("pinned_registry", [("BP02-071",)], indirect=True)
def test_registry_jp_outside_historical_scope_excludes_review(
    pinned_registry: tuple[Path, FrozenRegions, ReviewInputPin],
) -> None:
    root, raw_provider, pin = pinned_registry
    review_inputs = RegistryReviewInputs(pin)
    provider = FrozenRegions(
        jp=raw_provider.jp, en=raw_provider.en, review_inputs=review_inputs
    )
    plan = plan_preview(root, provider, regions=("en", "jp"))
    review = next(
        p for p in plan.projections if p.record_key.startswith("region_mapping_review:")
    )
    assert review.reasons == ("review_scope_outdated",)
    assert not plan.included("region_mapping_review")
    assert plan.report()["uncovered_jp_card_numbers"] == ["PR-001"]
