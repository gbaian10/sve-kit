"""The catalog participates in the existing inseparable text-preview transaction."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.provenance import BuildContext
from sve_carddb.domains.catalog.importer import catalog_configuration
from sve_carddb.domains.catalog.models import Alias, Catalog
from sve_carddb.domains.text_observations import text_configuration
from sve_carddb.domains.text_observations.composition import populate_text_preview

from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic inputs
from .text_observation_fixtures import LANGUAGES, make_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.registry.review import Inputs


@pytest.mark.parametrize("mode", ["catalog", "broken", "no_catalog"])
def test_catalog_composes_with_both_regional_sources(
    tmp_path: Path, inputs: Inputs, mode: str
) -> None:
    for card in inputs.jp.values():
        card.faces[0].text = "Synthetic unanimous rule."
    case = make_case(tmp_path / "authored", inputs)
    config = Catalog(
        languages=LANGUAGES,
        terms=(),
        aliases=(
            Alias(
                kind="type",
                code="missing" if mode == "broken" else "follower",
                lang="ja",
                text="Alias",
                normalized="alias",
            ),
        ),
        symbols=(),
        normalizer_version="synthetic-v1",
    )
    build = BuildContext.from_inputs(
        "a" * 40,
        text_configuration(case.plan, case.vocabulary, ())
        | catalog_configuration(config),
    )
    with create_database(compile_build(("en", "related"))) as db:

        def populate() -> None:
            with db.transaction():
                populate_text_preview(
                    db,
                    case.catalog,
                    case.compose(case.plan.identity),
                    authored_revision="a" * 40,
                    build=build,
                    vocabulary=case.vocabulary,
                    published=(),
                    languages=LANGUAGES,
                    stores={"test-store": case.store},
                    catalog_config=None if mode == "no_catalog" else config,
                )

        if mode == "broken":
            with pytest.raises(ValueError, match="target"):
                populate()
            assert not db.rows("card")
            assert not db.rows("source_record")
        else:
            populate()
            assert len(db.rows("rules_name")) > 0
            assert len(db.rows("face_rules_name")) > 0
            assert len(db.rows("face_rules_name")) == len(db.rows("face_current"))
            assert {row.values["region"] for row in db.rows("rules_name")} == {
                "jp",
                "en",
            }
            assert len(db.rows("search_alias")) == (0 if mode == "no_catalog" else 1)
