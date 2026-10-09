"""One source-to-view composition retains its processed inputs and DB dependencies."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.provenance import BuildContext

if TYPE_CHECKING:
    from pathlib import Path

    from ...support.shared_case_fixtures import TextCaseTemplate


def test_composition_imports_and_projects_its_original_source_inputs(
    tmp_path: Path, default_text_case: TextCaseTemplate
) -> None:
    case = default_text_case.copy(tmp_path)
    observations = case.compose()
    context = BuildContext.from_inputs(
        case.context().program_revision,
        observations.configuration(case.vocabulary, ()),
    )
    original = case.provider.cards["jp", "PR-001"]
    case.provider.cards["jp", "PR-001"] = original.model_copy(
        update={"source": original.source.model_copy(update={"etag": "changed"})}
    )
    with create_database(compile_build(("en", "related"))) as db:
        with db.transaction():
            case.stage(db)
        with pytest.raises(ValueError, match="revision is missing"):
            observations.views(db)
        record = observations.import_into(
            db, build=context, vocabulary=case.vocabulary, published=()
        )
        views = observations.views(db)
        assert len(views.observed) == 4
        assert any(
            item.region == "jp" for items in views.wording.values() for item in items
        )
        assert original.source in {use.source for use in record.uses}
        with pytest.raises(ValueError, match="independently verified"):
            case.compose()
