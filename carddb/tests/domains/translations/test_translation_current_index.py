"""Glossary and template readers consume fixed working-tree data areas."""

from typing import TYPE_CHECKING

from sve_carddb.domains.translations.four_layer_authored import read_inputs
from sve_carddb.domains.translations.inputs import load_glossary

from ...support.translation_fixtures import envelope, term, write

if TYPE_CHECKING:
    from pathlib import Path


def test_shared_working_tree_without_index_or_git(tmp_path: Path) -> None:
    authored = tmp_path / "authored"
    write(authored, {"translations/glossary/concepts/007.yaml": envelope([term()])})
    assert not (authored / "translations/index.yaml").exists()
    glossary = load_glossary(authored)
    templates = read_inputs(authored)
    assert templates.files == glossary.closure
    assert len(glossary.current_records()) == 1
    (authored / "translations/private-draft.yaml").write_text("not a data shard")
    assert load_glossary(authored).current_records() == glossary.current_records()
