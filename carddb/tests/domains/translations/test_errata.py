"""Historical translations require a Japanese notice and both exact source fragments."""

import json
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.translations.errata import apply, load

if TYPE_CHECKING:
    from pathlib import Path

    from tests.support.database_fixtures import DatabaseTemplate


@pytest.mark.parametrize("matches", [True, False])
def test_fragment_translation_is_source_paired(
    tmp_path: Path, t1b_database_template: DatabaseTemplate, *, matches: bool
) -> None:
    directory = tmp_path / "translations" / "errata"
    directory.mkdir(parents=True)
    entry = {
        "source_hash": digest(
            canonical(["Before", "Synthetic text" if matches else "Changed"])
        ),
        "source_url": "https://example.invalid/errata",
        "source_date": None,
        "lang": "zh-Hant",
        "before": "Synthetic old translation",
        "after": "Synthetic corrected translation",
        "origin": "machine",
        "low_confidence": True,
    }
    (directory / "TEST.yaml").write_text(
        json.dumps({"format": 1, "kind": "errata_translations", "entries": [entry]})
    )
    inputs = load(tmp_path)
    with t1b_database_template.copy() as db:
        with db.transaction():
            db.insert(
                "language",
                {
                    "code": "zh-Hant",
                    "fallback_order": Json([]),
                    "display_name": "Synthetic",
                },
            )
            report = apply(db, inputs, revision="a" * 40)
        assert report["applied"] == int(matches)
        assert len(db.rows("errata_change_translation")) == int(matches)
        if matches:
            translated = db.rows("errata_change_translation")[0].values
            assert translated["origin"] == "machine"
            assert translated["low_confidence"] is True


def test_english_notice_never_receives_a_japanese_translation(
    tmp_path: Path, t1b_database_template: DatabaseTemplate
) -> None:
    with t1b_database_template.copy() as db:
        with db.transaction():
            db.delete(
                "errata_printing",
                {"errata_version_id": "errata_v", "printing_id": "printing"},
            )
            db.update(
                "errata_change",
                {"id": "change"},
                {"before_revision_id": None, "after_revision_id": None},
            )
            db.update("errata", {"id": "errata"}, {"region": "en"})
            report = apply(db, load(tmp_path), revision="a" * 40)
        assert report["applied"] == 0
