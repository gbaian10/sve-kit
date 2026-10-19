"""Historical translations require a Japanese notice and both exact source fragments."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.card_extras.reviewed_errata import admit
from sve_carddb.domains.translations.errata import apply, load

if TYPE_CHECKING:
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
        assert sum(
            row.values["lang"] == "zh-Hant"
            for row in db.rows("errata_change_translation")
        ) == int(matches)
        if matches:
            translated = next(
                row.values
                for row in db.rows("errata_change_translation")
                if row.values["lang"] == "zh-Hant"
            )
            assert translated["origin"] == "machine"
            assert translated["low_confidence"] is True


def test_english_notice_never_receives_a_japanese_translation(
    tmp_path: Path, t1b_database_template: DatabaseTemplate
) -> None:
    directory = tmp_path / "translations" / "errata"
    directory.mkdir(parents=True)
    (directory / "TEST.yaml").write_text(
        json.dumps(
            {
                "format": 1,
                "kind": "errata_translations",
                "entries": [
                    {
                        "source_hash": digest(canonical(["Before", "Synthetic text"])),
                        "source_url": "https://example.invalid/errata",
                        "source_date": None,
                        "lang": "zh-Hant",
                        "before": "Synthetic old",
                        "after": "Synthetic new",
                        "origin": "machine",
                        "low_confidence": False,
                    }
                ],
            }
        )
    )
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


def test_repository_translations_all_match_japanese_changes() -> None:

    root = Path(__file__).parents[4]
    notices = admit(root / "card-data")
    available = {
        (
            notice.source.url,
            digest(
                canonical(
                    [
                        change.model_dump(mode="json")["before_value"],
                        change.model_dump(mode="json")["after_value"],
                    ]
                )
            ),
            notice.announced_on,
        )
        for notice in notices
        if notice.region == "jp"
        for change in notice.changes
    }
    entries = [
        entry
        for _, _, shard in load(root / "authored").files
        for entry in shard.entries
    ]
    assert entries
    assert all(
        (entry.source_url, entry.source_hash, entry.source_date) in available
        for entry in entries
    )
