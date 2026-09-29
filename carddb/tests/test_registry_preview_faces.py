"""Face order, partial endpoint loss and independent source metadata checks."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.registry.build import build
from sve_carddb.registry.preview import import_preview, plan_preview
from sve_carddb.registry.preview.evidence import CardEvidence, FaceEvidence
from sve_carddb.registry.records import PrintingData
from sve_carddb.registry.storage import plan_files, write_files

from .registry_preview_fixtures import REVISION, evidence, observed, parents
from .registry_snapshot_fixtures import edit_record
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- expose shared fixture dependency

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.review import Inputs
    from sve_carddb.registry.storage import Entry


def test_reversed_source_face_map_retains_per_face_credits(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.mapping.reskins = {}
    for collection, number in ((inputs.jp, "BP02-071"), (inputs.en, "BP02-070EN")):
        card = collection[number]
        card.faces.append(card.faces[0].model_copy(deep=True))
        card.faces[1].name += " back"
    write_files(plan_files(tmp_path, build(inputs, {}), "reviewer", "2026-09-28"))

    def reverse(entry: Entry) -> None:
        maps = entry.data["source_face_map"]
        assert isinstance(maps, list)
        assert isinstance(maps[0], dict)
        assert isinstance(maps[1], dict)
        maps[0]["face_id"], maps[1]["face_id"] = maps[1]["face_id"], maps[0]["face_id"]

    edit_record(tmp_path, "printing", reverse, region="en")
    plan = plan_preview(tmp_path, evidence(inputs), regions=("jp", "en"))
    printing = next(
        r.data
        for r in plan.included("printing")
        if isinstance(r.data, PrintingData) and r.data.card_no == "BP02-070EN"
    )
    with create_database(compile_build(("en", "related"))) as db:
        parents(db, plan)
        import_preview(db, plan, authored_revision=REVISION)
        face_credits = {
            row.values["face_id"]: row.values["credit_raw"]
            for row in db.rows("printing_face")
            if row.values["printing_id"] == printing.id
        }
        faces = sorted(
            (
                row.values
                for row in db.rows("face")
                if row.values["card_id"] == printing.card_id
            ),
            key=lambda row: str(row["ordinal"]),
        )
        assert face_credits[faces[0]["id"]] == "Synthetic artist 1"
        assert face_credits[faces[1]["id"]] == "Synthetic artist 0"


def test_reskin_requires_every_endpoint_variant_not_just_one(
    tmp_path: Path, inputs: Inputs
) -> None:
    second = inputs.en["BP02-070EN"].model_copy(deep=True)
    second.number = "BP02-999EN"
    inputs.en[second.number] = second
    inputs.mapping.targets[second.number] = "BP02-071"
    write_files(plan_files(tmp_path, build(inputs, {}), "reviewer", "2026-09-28"))
    inputs.en["BP02-070EN"].faces[0].text = "Changed one variant only."
    plan = plan_preview(tmp_path, evidence(inputs), regions=("en",))
    assert len(plan.included("card")) == 2
    assert len(plan.included("printing")) == 2
    assert not plan.included("card_related")


def test_extraction_factory_rejects_missing_face_metadata(inputs: Inputs) -> None:
    original = observed(inputs.jp["BP02-071"], "jp")
    with pytest.raises(ValueError, match="metadata count"):
        CardEvidence.from_card(original.source, "jp", inputs.jp["BP02-071"], ())


def test_premium_is_only_known_for_exact_pure_premium(
    tmp_path: Path, inputs: Inputs
) -> None:
    write_files(plan_files(tmp_path, build(inputs, {}), "reviewer", "2026-09-28"))
    provider = evidence(inputs)
    cards = dict(provider.cards)
    for number, rarity in (("BP02-071", "プレミアム"), ("PR-001", "LG・プレミアム")):
        cards["jp", number] = replace(
            cards["jp", number], faces=(FaceEvidence(rarity, None),)
        )
    plan = plan_preview(tmp_path, replace(provider, cards=cards), regions=("jp",))
    with create_database(compile_build()) as db:
        parents(db, plan)
        import_preview(db, plan, authored_revision=REVISION)
        by_number = {row.values["card_no"]: row.values for row in db.rows("printing")}
        assert by_number["BP02-071"]["premium"] is True
        assert by_number["BP02-071"]["rarity_code"] is None
        assert by_number["PR-001"]["premium"] is None
        assert by_number["PR-001"]["rarity_raw"] == "LG・プレミアム"


def test_art_group_requires_sources_for_all_its_uses(
    tmp_path: Path, inputs: Inputs
) -> None:
    second = inputs.en["BP02-070EN"].model_copy(deep=True)
    second.number = "BP02-999EN"
    inputs.en[second.number] = second
    inputs.mapping.targets[second.number] = "BP02-071"
    inputs.mapping.original_art.add(second.number)
    inputs.receipt.art_groups = [["BP02-070EN", second.number]]
    write_files(plan_files(tmp_path, build(inputs, {}), "reviewer", "2026-09-28"))
    inputs.en["BP02-070EN"].faces[0].text = "Changed one art use only."
    plan = plan_preview(tmp_path, evidence(inputs), regions=("en",))
    assert len(plan.included("printing")) == 2
    assert not plan.included("art")
    item = next(p for p in plan.projections if p.record_key.startswith("art:"))
    assert item.reasons == ("art_evidence_unavailable",)


def test_conflicting_face_rarity_requires_review(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.mapping.reskins = {}
    for collection, number in ((inputs.jp, "BP02-071"), (inputs.en, "BP02-070EN")):
        card = collection[number]
        card.faces.append(card.faces[0].model_copy(deep=True))
        card.faces[1].name += " back"
    write_files(plan_files(tmp_path, build(inputs, {}), "reviewer", "2026-09-28"))
    provider = evidence(inputs)
    cards = dict(provider.cards)
    original = cards["jp", "BP02-071"]
    cards["jp", "BP02-071"] = replace(
        original, faces=(original.faces[0], FaceEvidence("PR", None))
    )
    plan = plan_preview(tmp_path, replace(provider, cards=cards), regions=("jp",))
    with create_database(compile_build()) as db:
        parents(db, plan)
        with pytest.raises(ValueError, match="disagree on rarity"):
            import_preview(db, plan, authored_revision=REVISION)
        assert not db.rows("source_record")
        assert not db.rows("printing")
