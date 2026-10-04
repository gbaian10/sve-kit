"""Both readers recognize the shared index before individual areas migrate."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import canonical
from sve_carddb.template_translations.files import read
from sve_carddb.translations.loader import load_glossary
from sve_carddb.translations.models import Index

from .adoption_fixtures import commit, git
from .translation_fixtures import envelope, term, write

if TYPE_CHECKING:
    from pathlib import Path


def test_format_two_index_is_shared_by_glossary_and_template_readers(
    tmp_path: Path,
) -> None:
    authored = tmp_path / "authored"
    write(
        authored,
        {"translations/glossary/concepts/001.yaml": envelope([term()])},
    )
    index = authored / "translations/index.yaml"
    index.write_text(
        index.read_text().replace(
            "translation_authored_format: 1", "translation_authored_format: 2"
        )
    )
    glossary = load_glossary(authored)
    git(tmp_path, "init")
    revision = commit(tmp_path)
    templates = read(PinnedRepository(tmp_path), revision)
    assert templates.index == glossary.index
    assert templates.content == glossary.closure
    assert len(glossary.records()) == 1


@pytest.mark.parametrize("version", [True, 0, 3, "2"])
def test_index_rejects_unknown_or_noninteger_versions(version: JsonValue) -> None:
    with pytest.raises(ValidationError, match="translation_authored_format"):
        Index.model_validate_json(
            canonical(
                {
                    "translation_authored_format": version,
                    "kind": "translation_index",
                    "includes": {},
                    "inventories": {},
                }
            )
        )
