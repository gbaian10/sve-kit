"""Frozen raw bytes must belong to each exact card owner, even when texts are equal."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.source_binding import SourceDescriptor
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_sources import card_source

from ...support.build_db_fixtures import rows
from ...support.digital_link_import_fixtures import make_fixture
from ...support.translation_fixtures import template

if TYPE_CHECKING:
    from pathlib import Path


def test_equal_name_on_another_frozen_card_cannot_authorize_owner(
    tmp_path: Path,
) -> None:
    fixture = make_fixture(tmp_path, dual=True)
    with template().copy() as db:
        fixture.publish(db)
        revision = next(
            r.values
            for r in db.rows("face_revision")
            if r.values["id"] == "link-revision"
        )
        unit_id = revision["name_unit_id"]
        assert isinstance(unit_id, str)
        data: dict[str, JsonValue] = {
            "owner": {"kind": "face_revision", "revision_id": "link-revision"},
            "field": "name",
            "ordinal": None,
            "source_unit_id": unit_id,
            "source_hash": fixture.jp.text_hash[7:],
            "source_ref": fixture.jp.model_dump(mode="json")
            | {"text_hash": fixture.jp.text_hash[7:]},
        }
        descriptor = SourceDescriptor.model_validate_json(canonical(data))
        verified = card_source(db, fixture.sources(), descriptor)
        assert verified.text == "Synthetic card"
        assert verified.card_id == fixture.card.id
        assert verified.face_id == fixture.face.id
        other = fixture.others[0][3]
        assert other.text_hash == fixture.jp.text_hash
        assert other.source_version_id != fixture.jp.source_version_id
        wrong_version = data | {
            "source_ref": other.model_dump(mode="json")
            | {"text_hash": other.text_hash[7:]}
        }
        with pytest.raises(ValueError, match="another owner source"):
            card_source(
                db,
                fixture.sources(),
                SourceDescriptor.model_validate_json(canonical(wrong_version)),
            )
        wrong_face = data | {
            "source_ref": descriptor.source_ref.model_dump(mode="json")
            | {"locator": "/faces/1/name"}
        }
        with pytest.raises(ValueError, match="exact face field"):
            card_source(
                db,
                fixture.sources(),
                SourceDescriptor.model_validate_json(canonical(wrong_face)),
            )
        wrong_unit = data | {"source_unit_id": "text:another-owner"}
        with pytest.raises(ValueError, match="exact owner field"):
            card_source(
                db,
                fixture.sources(),
                SourceDescriptor.model_validate_json(canonical(wrong_unit)),
            )


def test_printed_unknown_effect_does_not_borrow_its_current_revision(
    tmp_path: Path,
) -> None:
    fixture = make_fixture(tmp_path)
    with template().copy() as db:
        fixture.publish(db)
        revision = next(
            r.values
            for r in db.rows("face_revision")
            if r.values["id"] == "link-revision"
        )
        unit_id = revision["name_unit_id"]
        assert isinstance(unit_id, str)
        with db.transaction():
            db.insert(
                "printing",
                rows()["printing"]
                | {
                    "id": "printing:sample",
                    "card_id": fixture.card.id,
                    "card_no": fixture.printing.card_no,
                    "home_set_id": fixture.printing.home_set_id,
                    "source_id": revision["source_id"],
                },
            )
            db.insert(
                "printing_face",
                rows()["printing_face"]
                | {
                    "printing_id": "printing:sample",
                    "face_id": fixture.face.id,
                    "card_id": fixture.card.id,
                    "printed_name_unit_id": unit_id,
                    "printed_effect_unit_id": None,
                    "printed_text_state": "unknown",
                    "source_id": revision["source_id"],
                },
            )
        data: dict[str, JsonValue] = {
            "owner": {
                "kind": "printing_face",
                "printing_id": "printing:sample",
                "face_id": fixture.face.id,
            },
            "field": "name",
            "ordinal": None,
            "source_unit_id": unit_id,
            "source_hash": fixture.jp.text_hash[7:],
            "source_ref": fixture.jp.model_dump(mode="json")
            | {"text_hash": fixture.jp.text_hash[7:]},
        }
        descriptor = SourceDescriptor.model_validate_json(canonical(data))
        assert card_source(db, fixture.sources(), descriptor).text == "Synthetic card"
        effect = data | {
            "field": "effect",
            "source_ref": descriptor.source_ref.model_dump(mode="json")
            | {"locator": "/faces/0/text"},
        }
        with pytest.raises(ValueError, match="nonempty typed identifier"):
            card_source(
                db,
                fixture.sources(),
                SourceDescriptor.model_validate_json(canonical(effect)),
            )
