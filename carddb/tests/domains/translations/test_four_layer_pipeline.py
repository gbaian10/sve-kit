"""The compiling consumer uses real frozen sources before selecting authored frames."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.source_binding import SourceDescriptor
from sve_carddb.core.json import canonical
from sve_carddb.domains.text_observations.vocabulary import Vocabulary
from sve_carddb.domains.translations.four_layer_matching import Frames
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.four_layer_pipeline import (
    compile_card_field,
    prepare_classifier,
)
from sve_carddb.domains.translations.inputs import load_glossary

from ...support.digital_link_import_fixtures import make_fixture
from ...support.translation_fixtures import envelope, template, term, write
from .test_four_layer_classification import classifier

if TYPE_CHECKING:
    from pathlib import Path


def test_frozen_name_reaches_authored_frame_using_the_adopted_concept(
    tmp_path: Path,
) -> None:
    fixture = make_fixture(tmp_path, dual=True)
    record = term("name.pipeline", category="card_name")
    data = record["data"]
    assert isinstance(data, dict)
    data["source_ref"] = fixture.jp.model_dump(mode="json")
    root = tmp_path / "authored"
    write(root, {"translations/glossary/defs/000.yaml": envelope([record])})
    with template().copy() as db:
        fixture.publish(db)
        revision = next(
            r.values
            for r in db.rows("face_revision")
            if r.values["id"] == "link-revision"
        )
        descriptor = SourceDescriptor.model_validate_json(
            canonical(
                {
                    "owner": {"kind": "face_revision", "revision_id": "link-revision"},
                    "field": "name",
                    "ordinal": None,
                    "source_unit_id": revision["name_unit_id"],
                    "source_hash": fixture.jp.text_hash[7:],
                    "source_ref": fixture.jp.model_dump(mode="json")
                    | {"text_hash": fixture.jp.text_hash[7:]},
                }
            )
        )
        sources = fixture.sources()
        engine = prepare_classifier(
            db,
            load_glossary(root),
            sources,
            Vocabulary(bindings=()),
            classifier().rules,
        )
        field = normalize_source("Synthetic card", descriptor)
        frame, _ = engine.recognize(
            "Synthetic card", descriptor, field.parts[0], field=field
        ).bind(descriptor, field.parts[0])
        compiled = compile_card_field(db, sources, descriptor, engine, Frames((frame,)))
        assert compiled.complete
        assert compiled.source.card_id == fixture.card.id
        assert compiled.matches[0] is not None
        assert compiled.matches[0].binding.values["leaf_0"].model_dump() == {
            "kind": "card_name",
            "term_id": "term:name.pipeline",
        }
        assert engine.references.kind_facts.named == (
            ("term:name.pipeline", "follower"),
        )
        absent = compile_card_field(db, sources, descriptor, engine, Frames(()))
        assert not absent.complete
        assert absent.issues == ("unmatched_source_frame",)
        other = fixture.others[0][3]
        borrowed = descriptor.model_copy(
            update={
                "source_ref": descriptor.source_ref.model_copy(
                    update={"source_version_id": other.source_version_id}
                )
            }
        )
        with pytest.raises(ValueError, match="another owner source"):
            compile_card_field(db, sources, borrowed, engine, Frames((frame,)))
