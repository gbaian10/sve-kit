"""Atomic corrected previews bind every image/comparison use to the complete bundle."""

import sqlite3
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_bundle import publish_bundle, verify_bundle
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import input_record
from sve_carddb.source_corrections import importer as corrections
from sve_carddb.text_observations import (
    importer,
    populate_text_preview,
    text_preview_uses,
)

from .registry_preview_fixtures import REVISION
from .text_observation_fixtures import LANGUAGES

if TYPE_CHECKING:
    from collections.abc import Iterable
    from pathlib import Path

    from sve_carddb.build_db import Database, Value
    from sve_carddb.build_inputs import BuildContext, InputRecord, SourceUse
    from sve_carddb.source_corrections.plan import Application
    from sve_carddb.text_observations.plan import TextPlan

    from .shared_case_fixtures import CorrectionCaseTemplate


class TestDefaultCorrectionInputs:
    def test_two_corrected_bundles_preserve_all_exact_sources_and_uses(
        self, tmp_path: Path, default_correction_case: CorrectionCaseTemplate
    ) -> None:
        fixture = default_correction_case.copy(tmp_path / "case")
        case = fixture.texts
        context = case.context()
        schema = compile_build(("en", "related", "correction"))
        stores = {"test-store": case.store, "image-store": fixture.image_store}
        expected = text_preview_uses(case.catalog, case.plan, stores)

        def populate(db: Database) -> InputRecord:
            return populate_text_preview(
                db,
                case.catalog,
                case.plan,
                authored_revision=REVISION,
                build=context,
                vocabulary=case.vocabulary,
                published=(),
                languages=LANGUAGES,
                stores=stores,
            )

        roots = (tmp_path / "one", tmp_path / "two")
        for root in roots:
            record = publish_bundle(
                schema,
                root,
                context,
                expected,
                populate,
                case.plan.report(),
                stores=stores,
            )
            assert record == verify_bundle(
                schema, root, context, expected, stores=stores
            )
            images = [
                use for use in record.uses if use.usage == "source_correction_evidence"
            ]
            assert len(images) == 1
            assert images[0].source.kind == "image"
            assert images[0].source.parser_version == "correction-image-evidence-v1"
            assert images[0].source.archive.batch_id == fixture.images.sources.batch_id
        for name in ("build.sqlite", "inputs.json", "report.json", "seal.json"):
            assert (roots[0] / name).read_bytes() == (roots[1] / name).read_bytes()
        assert b"Rule." not in (roots[0] / "report.json").read_bytes()

    @pytest.mark.parametrize(
        "change", ["omit_image", "omit_comparison", "parser", "locator", "archive"]
    )
    def test_each_correction_use_corruption_rolls_back_full_composition(
        self,
        tmp_path: Path,
        default_correction_case: CorrectionCaseTemplate,
        monkeypatch: pytest.MonkeyPatch,
        change: str,
    ) -> None:
        fixture = default_correction_case.copy(tmp_path / "case")
        case = fixture.texts
        context = case.context()
        schema = compile_build(("en", "related", "correction"))
        stores = {"test-store": case.store, "image-store": fixture.image_store}

        def broken(build: BuildContext, uses: Iterable[SourceUse]) -> InputRecord:
            values = list(uses)
            target = (
                "source_correction_comparison"
                if change == "omit_comparison"
                else "source_correction_evidence"
            )
            use = next(use for use in values if use.usage == target)
            values.remove(use)
            if change in {"parser", "archive"}:
                source = use.source.model_copy(
                    update={"parser_version": "wrong"}
                    if change == "parser"
                    else {
                        "archive": use.source.archive.model_copy(
                            update={"batch_id": "sha256:" + "0" * 64}
                        )
                    }
                )
                values.append(use.model_copy(update={"source": source}))
            elif change == "locator":
                values.append(use.model_copy(update={"locator": "wrong"}))
            return input_record(build, values)

        monkeypatch.setattr(importer, "input_record", broken)

        def populate(db: Database) -> InputRecord:
            return populate_text_preview(
                db,
                case.catalog,
                case.plan,
                authored_revision=REVISION,
                build=context,
                vocabulary=case.vocabulary,
                published=(),
                languages=LANGUAGES,
                stores=stores,
            )

        destination = tmp_path / "invalid"
        with pytest.raises(ValueError, match="use closure"):
            publish_bundle(
                schema,
                destination,
                context,
                text_preview_uses(case.catalog, case.plan, stores),
                populate,
                case.plan.report(),
                stores=stores,
            )
        assert not destination.exists()

    def test_late_application_failure_does_not_publish_any_partial_bundle(
        self,
        tmp_path: Path,
        default_correction_case: CorrectionCaseTemplate,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        fixture = default_correction_case.copy(tmp_path / "case")
        case = fixture.texts
        context = case.context()
        stores = {"test-store": case.store, "image-store": fixture.image_store}
        original = corrections.application_values

        def wrong(application: Application, plan: TextPlan) -> dict[str, Value]:
            values = original(application, plan)
            values["result_unit_id"] = "t:ja:0000000000000000"
            return values

        monkeypatch.setattr(corrections, "application_values", wrong)

        def populate(db: Database) -> InputRecord:
            return populate_text_preview(
                db,
                case.catalog,
                case.plan,
                authored_revision=REVISION,
                build=context,
                vocabulary=case.vocabulary,
                published=(),
                languages=LANGUAGES,
                stores=stores,
            )

        destination = tmp_path / "invalid"
        with pytest.raises(sqlite3.IntegrityError, match=r"[Ff]oreign key"):
            publish_bundle(
                compile_build(("en", "related", "correction")),
                destination,
                context,
                text_preview_uses(case.catalog, case.plan, stores),
                populate,
                case.plan.report(),
                stores=stores,
            )
        assert not destination.exists()
