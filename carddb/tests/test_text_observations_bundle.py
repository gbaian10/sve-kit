"""Verify full source-use closure, deterministic bundles and transaction rollback."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_bundle import publish_bundle, verify_bundle
from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import input_record, insert_raw_sources
from sve_carddb.snapshot.values import canonical, parse
from sve_carddb.text_observations import (
    importer,
    populate_text_preview,
    text_preview_uses,
)
from sve_carddb.text_observations.importer import import_text_observations

from .registry_preview_fixtures import REVISION
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture
from .text_observation_fixtures import LANGUAGES, make_case

if TYPE_CHECKING:
    from collections.abc import Iterable
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import BuildContext, InputRecord, SourceUse
    from sve_carddb.registry.review import Inputs
    from sve_carddb.text_observations.intern import TextInterner
    from sve_carddb.text_observations.models import FaceObservation

    from .shared_case_fixtures import TextCaseTemplate


def test_two_clean_bundles_are_identical_and_include_null_effect_uses(
    tmp_path: Path, inputs: Inputs
) -> None:
    for card in inputs.jp.values():
        card.faces[0].text = None
    case = make_case(tmp_path / "authored", inputs)
    schema = compile_build(("en", "related"))
    context = case.context()
    stores = {"test-store": case.store}
    expected = text_preview_uses(case.catalog, case.plan, stores)
    report = case.plan.report()

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

    roots = (tmp_path / "first", tmp_path / "second")
    for root in roots:
        record = publish_bundle(
            schema, root, context, expected, populate, report, stores=stores
        )
        assert record == verify_bundle(schema, root, context, expected, stores=stores)
        text_uses = [
            use
            for use in record.uses
            if use.usage in {"face_text_observation", "face_current_comparison"}
        ]
        assert len(text_uses) == 8
        assert {use.source.parser_version for use in text_uses} == {"synthetic-text-v1"}
        assert all(
            use.source.archive.batch_id.startswith("sha256:") for use in text_uses
        )
        assert all(
            use.source.archive.first_receipt_id.startswith("sha256:")
            for use in text_uses
        )
    for name in ("build.sqlite", "inputs.json", "report.json", "seal.json"):
        assert (roots[0] / name).read_bytes() == (roots[1] / name).read_bytes()
    assert b"Rule." not in (roots[0] / "report.json").read_bytes()


@pytest.mark.parametrize(
    "change", ["omit_null", "parser", "usage", "locator", "archive"]
)
def test_each_text_use_corruption_fails_before_publishing(
    tmp_path: Path, inputs: Inputs, monkeypatch: pytest.MonkeyPatch, change: str
) -> None:

    for card in inputs.jp.values():
        card.faces[0].text = None
    case = make_case(tmp_path / "authored", inputs)
    stores = {"test-store": case.store}
    context = case.context()
    original = input_record

    def broken(build: BuildContext, uses: Iterable[SourceUse]) -> InputRecord:
        values = list(uses)
        use = next(use for use in values if '"region":"jp"' in use.locator)
        values.remove(use)
        if change != "omit_null":
            if change in {"parser", "archive"}:
                source = (
                    use.source.model_copy(update={"parser_version": "wrong"})
                    if change == "parser"
                    else use.source.model_copy(
                        update={
                            "archive": use.source.archive.model_copy(
                                update={"batch_id": "sha256:" + "0" * 64}
                            )
                        }
                    )
                )
                use = use.model_copy(update={"source": source})
            else:
                use = use.model_copy(update={change: "wrong"})
            values.append(use)
        return original(build, values)

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
            compile_build(("en", "related")),
            destination,
            context,
            text_preview_uses(case.catalog, case.plan, stores),
            populate,
            case.plan.report(),
            stores=stores,
        )
    assert not destination.exists()


class TestDefaultTextInputs:
    @pytest.mark.parametrize(
        "key", ["text_observations", "text_vocabulary", "published_text_hash"]
    )
    def test_each_configuration_pin_is_independently_required(
        self, tmp_path: Path, default_text_case: TextCaseTemplate, key: str
    ) -> None:
        case = default_text_case.copy(tmp_path)
        configuration = parse(case.context().configuration.encode())
        assert isinstance(configuration, dict)
        del configuration[key]
        broken = case.context().model_copy(
            update={"configuration": canonical(configuration).decode()}
        )
        schema = compile_build(("en", "related"))
        with create_database(schema) as db:
            with db.transaction():
                case.stage(db)
            before = {table.name: db.rows(table.name) for table in schema.tables}
            with pytest.raises(ValueError, match="configuration"):
                import_text_observations(
                    db,
                    case.plan,
                    build=broken,
                    vocabulary=case.vocabulary,
                    published=(),
                )
            assert before == {
                table.name: db.rows(table.name) for table in schema.tables
            }

    def test_late_observation_failure_rolls_back_all_new_rows(
        self,
        tmp_path: Path,
        default_text_case: TextCaseTemplate,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:

        case = default_text_case.copy(tmp_path)
        schema = compile_build(("en", "related"))
        original = importer._physical
        called = 0

        def fail_late(db: Database, item: FaceObservation, texts: TextInterner) -> None:
            nonlocal called
            called += 1
            original(db, item, texts)
            if called == 2:
                raise ValueError("Synthetic late failure")

        monkeypatch.setattr(importer, "_physical", fail_late)
        with create_database(schema) as db:
            with db.transaction():
                case.stage(db)
            before = {table.name: db.rows(table.name) for table in schema.tables}
            with pytest.raises(ValueError, match="Synthetic late"):
                import_text_observations(
                    db,
                    case.plan,
                    build=case.context(),
                    vocabulary=case.vocabulary,
                    published=(),
                )
            assert called == 2
            assert before == {
                table.name: db.rows(table.name) for table in schema.tables
            }
            db.verify()

    def test_source_metadata_conflict_rolls_back_the_entire_composer(
        self, tmp_path: Path, default_text_case: TextCaseTemplate
    ) -> None:
        case = default_text_case.copy(tmp_path)
        source = case.plan.observations[0].card.source
        schema = compile_build(("en", "related"))
        with create_database(schema) as db:
            with db.transaction():
                insert_raw_sources(
                    db, (source.model_copy(update={"etag": "conflicting"}),)
                )
            before = {table.name: db.rows(table.name) for table in schema.tables}
            with pytest.raises(ValueError, match="metadata"), db.transaction():
                populate_text_preview(
                    db,
                    case.catalog,
                    case.plan,
                    authored_revision=REVISION,
                    build=case.context(),
                    vocabulary=case.vocabulary,
                    published=(),
                    languages=LANGUAGES,
                    stores={"test-store": case.store},
                )
            assert before == {
                table.name: db.rows(table.name) for table in schema.tables
            }
