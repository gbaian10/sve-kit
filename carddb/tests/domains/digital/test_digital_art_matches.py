"""Synthetic frozen API through authored art intake and real build constraints."""

from typing import TYPE_CHECKING, Literal

import pytest

from sve_carddb.core.json import canonical, digest, object_value, parse
from sve_carddb.domains.digital.art_matches.importer import populate
from sve_carddb.domains.digital.art_matches.loader import load
from sve_carddb.domains.digital.art_matches.models import (
    Candidate,
    Index,
    Record,
    Shard,
    Witness,
)
from sve_carddb.domains.digital.links.importer import populate_links
from sve_carddb.domains.registry.storage import encode
from sve_carddb.domains.translations.sources import Sources

from ...support.digital_link_import_fixtures import Fixture, make_fixture
from ...support.translation_fixtures import HASH, template

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database
    from sve_carddb.domains.digital.art_matches.loader import Snapshot
    from sve_carddb.domains.digital.links.models import Phase

ART = "a:" + "5" * 32


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> Fixture:
    return make_fixture(tmp_path_factory.mktemp("digital-art"), art=True)


def art_record(
    fixture: Fixture, state: Literal["matched", "not_found", "unknown"] = "matched"
) -> Record:
    images: tuple[tuple[Phase, str, str], ...] = (
        ("normal", "base", "a"),
        ("evolved", "base", "b"),
        ("normal", "alternate-one", "c"),
        ("evolved", "alternate-one", "d"),
    )
    candidates = tuple(
        Candidate(
            official_id="22345678",
            phase=phase,
            style_key=style,
            image_url="https://shadowverse-wb.com/uploads/card_image/jpn/card/"
            + letter * 32
            + ".png",
            image_sha256=HASH,
            crop=(10, 20, 410, 320),
            inliers=(150 if state == "matched" else 25 if state == "unknown" else 0)
            if letter == "c"
            else 0,
        )
        for phase, style, letter in images
    )
    return Record(
        art_id=ART,
        card_id=fixture.card.id,
        face_id=fixture.face.id,
        game="svwb",
        state=state,
        same_card_targets=("22345678",),
        witness=Witness(
            printing_id=fixture.printing.id,
            face_id=fixture.face.id,
            image_url="https://shadowverse-evolve.com/synthetic.png",
            image_sha256=HASH,
            crop=(10, 20, 410, 320),
        ),
        candidates=candidates,
        complete=True,
        review_level="model_reviewed",
        reason="Synthetic comparison; no human confirmation.",
    )


def entry(root: Path, records: tuple[Record, ...]) -> Snapshot:
    path = "digital/art-matches/synthetic/001.yaml"
    shard = Shard(format=1, kind="digital_art_match_shard", records=records)
    target = root / path
    target.parent.mkdir(parents=True)
    target.write_bytes(encode(shard))
    index = Index(
        format=1,
        kind="digital_art_match_index",
        opencv_version="synthetic",
        as_of="2026-11-16",
        includes={path: digest(canonical(shard.model_dump(mode="json")))},
    )
    (root / "digital/art-matches/index.yaml").write_bytes(encode(index))
    result = load(root)
    assert result is not None
    return result


def publish_art(db: Database, fixture: Fixture) -> None:
    fixture.publish(db)
    with db.transaction():
        db.insert(
            "art",
            {
                "id": ART,
                "card_id": fixture.card.id,
                "face_id": fixture.face.id,
                "classification": "unclassified",
            },
        )
        db.insert(
            "printing",
            {
                "id": fixture.printing.id,
                "card_id": fixture.card.id,
                "region": "jp",
                "card_no": "SYN-001",
                "card_no_state": "official",
                "catalog_state": "official",
                "decklog_available": True,
                "decklog_verification": "unverified",
                "variant_key": "standard",
                "home_set_id": fixture.card.home_set_id,
                "rarity_raw": "LG",
                "source_id": "source",
            },
        )
        db.insert(
            "printing_face",
            {
                "printing_id": fixture.printing.id,
                "face_id": fixture.face.id,
                "card_id": fixture.card.id,
                "art_id": ART,
                "embellishment_state": "unreviewed",
                "printed_text_state": "unknown",
                "source_id": "source",
            },
        )


@pytest.mark.parametrize("state", ["matched", "not_found", "unknown"])
def test_frozen_style_match_and_explicit_absence(
    baseline: Fixture,
    tmp_path: Path,
    state: Literal["matched", "not_found", "unknown"],
) -> None:
    adopted = entry(tmp_path, (art_record(baseline, state),))
    config = object_value(parse(baseline.build.configuration.encode()))
    context = baseline.changed(config | adopted.configuration())
    sources = Sources({"test-store": baseline.store}, baseline.root, context)
    with template(("digital_art",)).copy() as db:
        publish_art(db, baseline)
        with db.transaction():
            links = populate_links(
                db,
                baseline.inputs(),
                build=context,
                stores={"test-store": baseline.store},
                sources=sources,
            )
            result = populate(
                db,
                adopted,
                revision=baseline.authored,
                links=links,
                sources=sources,
            )
        assert result[state] == 1
        coverage = db.rows("digital_art_coverage")
        assert len(coverage) == 1
        assert (coverage[0].values["art_id"], coverage[0].values["state"]) == (
            ART,
            state,
        )
        matches = db.rows("digital_art_link")
        assert len(matches) == (1 if state == "matched" else 0)
        if state == "matched":
            asset = db.rows("digital_art")[0].values
            assert (asset["digital_face_id"], asset["style_key"]) == (
                "digital:svwb:22345678:normal",
                "alternate-one",
            )
        decision = next(
            r.values
            for r in db.rows("decision")
            if r.values["id"] == coverage[0].values["decision_id"]
        )
        assert decision["state"] == "model_reviewed"
        assert any(use.usage == "digital_art_candidate_scope" for use in sources.uses)
        db.verify()


@pytest.mark.parametrize("human", [False, True])
def test_missing_style_cannot_become_absence(
    baseline: Fixture, tmp_path: Path, human: bool
) -> None:
    record = art_record(baseline, "not_found")
    if human:
        raw = art_record(baseline, "unknown").model_dump(mode="json")
        raw.update(
            state="not_found",
            review_level="confirmed",
            manual_review={
                "source": "Synthetic visual review",
                "reviewed_on": "2026-11-17",
                "matched_candidates": [],
            },
        )
        record = Record.model_validate_json(canonical(raw))
    record = record.model_copy(update={"candidates": record.candidates[:2]})
    adopted = entry(tmp_path, (record,))
    config = object_value(parse(baseline.build.configuration.encode()))
    context = baseline.changed(config | adopted.configuration())
    sources = Sources({"test-store": baseline.store}, baseline.root, context)
    with template(("digital_art",)).copy() as db:
        publish_art(db, baseline)
        with db.transaction():
            links = populate_links(
                db,
                baseline.inputs(),
                build=context,
                stores={"test-store": baseline.store},
                sources=sources,
            )
            result = populate(
                db, adopted, revision=baseline.authored, links=links, sources=sources
            )
        assert result == {"matched": 0, "not_found": 0, "unknown": 1}


def test_unpinned_entry_fails_before_materialization(
    baseline: Fixture, tmp_path: Path
) -> None:
    adopted = entry(tmp_path, (art_record(baseline),))
    with template(("digital_art",)).copy() as db, db.transaction():
        with pytest.raises(ValueError, match="does not pin"):
            populate(
                db,
                adopted,
                revision=baseline.authored,
                links=None,
                sources=baseline.sources(),
            )
        assert not db.rows("digital_art_coverage")


def test_removed_link_entry_does_not_reuse_old_absence(
    baseline: Fixture, tmp_path: Path
) -> None:
    adopted = entry(tmp_path, (art_record(baseline, "not_found"),))
    context = baseline.changed(adopted.configuration())
    sources = Sources({"test-store": baseline.store}, baseline.root, context)
    with template(("digital_art",)).copy() as db:
        publish_art(db, baseline)
        with db.transaction():
            result = populate(
                db,
                adopted,
                revision=baseline.authored,
                links=None,
                sources=sources,
            )
        assert result == {"matched": 0, "not_found": 0, "unknown": 1}
        assert not db.rows("digital_art_link")


def test_duplicate_subject_and_unindexed_file_fail(
    baseline: Fixture, tmp_path: Path
) -> None:
    record = art_record(baseline)
    with pytest.raises(ValueError, match="Duplicate art match subject"):
        entry(tmp_path / "duplicate", (record, record))
    root = tmp_path / "closure"
    entry(root, (record,))
    (root / "digital/art-matches/unindexed.yaml").write_text("format: 1\n")
    with pytest.raises(ValueError, match="closure"):
        load(root)


def test_gray_score_is_neither_adopted_match_nor_absence(
    baseline: Fixture,
) -> None:
    record = art_record(baseline, "unknown").model_dump(mode="json")
    record["state"] = "not_found"
    with pytest.raises(ValueError, match="below review"):
        Record.model_validate_json(canonical(record))
    record["state"] = "matched"
    with pytest.raises(ValueError, match="decisive"):
        Record.model_validate_json(canonical(record))


def test_one_style_cannot_reference_two_images(
    baseline: Fixture, tmp_path: Path
) -> None:
    first = art_record(baseline)
    changed = first.candidates[0].model_copy(
        update={"image_sha256": "sha256:" + "b" * 64}
    )
    second = first.model_copy(
        update={
            "art_id": "a:" + "6" * 32,
            "candidates": (changed, *first.candidates[1:]),
        }
    )
    with pytest.raises(ValueError, match="conflicting image evidence"):
        entry(tmp_path, (first, second))


@pytest.mark.parametrize("state", ["matched", "not_found"])
def test_confirmed_gray_verdict_materializes_exactly_selected_images(
    baseline: Fixture,
    tmp_path: Path,
    state: Literal["matched", "not_found"],
) -> None:
    raw = art_record(baseline, "unknown").model_dump(mode="json")
    raw.update(
        state=state,
        review_level="confirmed",
        reason="Synthetic human verdict with source; scores remain unchanged.",
        manual_review={
            "source": "Synthetic maintainer visual review",
            "reviewed_on": "2026-11-17",
            "matched_candidates": [
                {
                    "official_id": "22345678",
                    "phase": "normal",
                    "style_key": "alternate-one",
                }
            ]
            if state == "matched"
            else [],
        },
    )
    record = Record.model_validate_json(canonical(raw))
    adopted = entry(tmp_path, (record,))
    config = object_value(parse(baseline.build.configuration.encode()))
    context = baseline.changed(config | adopted.configuration())
    sources = Sources({"test-store": baseline.store}, baseline.root, context)
    with template(("digital_art",)).copy() as db:
        publish_art(db, baseline)
        with db.transaction():
            links = populate_links(
                db,
                baseline.inputs(),
                build=context,
                stores={"test-store": baseline.store},
                sources=sources,
            )
            result = populate(
                db, adopted, revision=baseline.authored, links=links, sources=sources
            )
        assert result == {
            "matched": int(state == "matched"),
            "not_found": int(state == "not_found"),
            "unknown": 0,
        }
        assert [r.values["digital_art_id"] for r in db.rows("digital_art_link")] == (
            ["digital-art:svwb:22345678:normal:alternate-one"]
            if state == "matched"
            else []
        )
        coverage = db.rows("digital_art_coverage")[0].values
        assert coverage["state"] == state
        decision = next(
            r for r in db.rows("decision") if r.values["id"] == coverage["decision_id"]
        )
        assert decision.values["state"] == "confirmed"
        assert record.candidates[2].inliers == 25
        db.verify()


@pytest.mark.parametrize(
    "change",
    [
        "source",
        "review_level",
        "unknown",
        "missing_candidate",
        "duplicate_candidate",
        "no_positive",
        "negative_with_positive",
        "incomplete_negative",
    ],
)
def test_manual_verdict_cannot_bypass_its_evidence_contract(
    baseline: Fixture, change: str
) -> None:
    raw = art_record(baseline, "unknown").model_dump(mode="json")
    key = {"official_id": "22345678", "phase": "normal", "style_key": "alternate-one"}
    review = {
        "source": "Synthetic visual review",
        "reviewed_on": "2026-11-17",
        "matched_candidates": [key],
    }
    raw.update(state="matched", review_level="confirmed", manual_review=review)
    if change == "source":
        review["source"] = " "
    elif change == "review_level":
        raw["review_level"] = "model_reviewed"
    elif change == "unknown":
        raw["state"] = "unknown"
    elif change == "missing_candidate":
        key["style_key"] = "missing"
    elif change == "duplicate_candidate":
        review["matched_candidates"] = [key, key]
    elif change == "no_positive":
        review["matched_candidates"] = []
    elif change == "negative_with_positive":
        raw["state"] = "not_found"
    else:
        raw.update(state="not_found", complete=False)
        review["matched_candidates"] = []
    with pytest.raises(ValueError, match=r"Manual art review|Art absence"):
        Record.model_validate_json(canonical(raw))


@pytest.mark.parametrize("state", ["matched", "not_found"])
@pytest.mark.parametrize("explicit_null", [False, True])
def test_confirmed_level_alone_cannot_adopt_a_gray_conclusion(
    baseline: Fixture, state: Literal["matched", "not_found"], explicit_null: bool
) -> None:
    raw = art_record(baseline, "unknown").model_dump(mode="json")
    raw.update(state=state, review_level="confirmed")
    if explicit_null:
        raw["manual_review"] = None
    with pytest.raises(ValueError, match=r"Art match requires|Art absence requires"):
        Record.model_validate_json(canonical(raw))
