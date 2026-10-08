"""Real synthetic frozen entry-to-DB-to-name wiring with reusable construction."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.core.json import array, canonical, digest, object_value, parse
from sve_carddb.domains.digital.links.evidence import (
    Evidence,
    RegistryIndex,
    batch_refs,
    inventory,
)
from sve_carddb.domains.digital.links.importer import import_links, review_context
from sve_carddb.domains.digital.links.loader import decision_id
from sve_carddb.domains.digital.links.models import Record, SveName
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.registry.records import EnglishPrintingData
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.translations.digital import configuration, import_digital
from sve_carddb.domains.translations.names.sources import NameOwner
from sve_carddb.domains.translations.sources import Sources

from .digital_link_import_fixtures import (
    Fixture,
    copied,
    current_api,
    make_fixture,
    with_records,
)
from .translation_fixtures import template

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.catalog.adoption_models import ReviewContext

    from .database_fixtures import DatabaseTemplate


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> Fixture:
    return make_fixture(tmp_path_factory.mktemp("digital-link-import"))


@pytest.fixture(scope="module")
def database() -> DatabaseTemplate:
    return template()


def test_real_entry_proves_relation_name_and_no_coverage(
    baseline: Fixture, database: DatabaseTemplate
) -> None:
    with database.copy() as db:
        baseline.publish(db)
        result = import_links(
            db,
            baseline.inputs(),
            build=baseline.build,
            stores={"test-store": baseline.store},
        )
        assert len(result.fresh) == 1
        assert result.stale == ()
        links = [
            row.values
            for row in db.rows("digital_link")
            if row.values["card_id"] == baseline.card.id
            and row.values["face_id"] == baseline.face.id
        ]
        assert len(links) == 1
        link = links[0]
        assert link["relation"] == "same_card"
        assert link["decision_id"] == decision_id(
            Record.model_validate_json(baseline.record)
        )
        decision = next(
            r.values
            for r in db.rows("decision")
            if r.values["id"] == link["decision_id"]
        )
        assert (decision["state"], decision["scope"], decision["sample_ids"]) == (
            "confirmed",
            "record",
            None,
        )
        shard = next(
            r.values
            for r in db.rows("decision_source")
            if r.values["decision_id"] == link["decision_id"]
        )
        assert shard["locator"] == "digital-links/links/synthetic/001.yaml"
        face = next(
            row.values
            for row in db.rows("digital_face")
            if row.values["id"] == link["digital_face_id"]
        )
        assert face["digital_card_id"] == link["digital_card_id"]
        assert face["phase"] == "normal"
        card = next(
            row.values
            for row in db.rows("digital_card")
            if row.values["id"] == link["digital_card_id"]
        )
        assert (card["game"], card["official_id"]) == ("svwb", "22345678")
        name = next(
            row.values
            for row in db.rows("digital_text")
            if row.values["digital_face_id"] == face["id"]
            and row.values["lang"] == "zh-Hant"
        )
        unit = next(
            row.values
            for row in db.rows("text_unit")
            if row.values["id"] == name["name_unit_id"]
        )
        assert (unit["lang"], unit["text"]) == ("zh-Hant", "合成測試名")
        assert db.rows("digital_link_coverage") == ()
        assert result.eligible_owner(
            db, baseline.sources(), NameOwner("face_revision", "link-revision")
        )
        used = {
            (u.source.id, u.locator, u.usage, u.source.parser_version)
            for u in result.record.uses
        }
        for ref in (baseline.jp, *baseline.refs):
            assert (
                ref.source_version_id,
                ref.locator,
                "translation_evidence",
                ref.parser,
            ) in used
        for ref in baseline.refs:
            assert (
                ref.source_version_id,
                "/data",
                "digital_name_inventory",
                ref.parser,
            ) in used


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("source_hash", "Evidence must locate exact hash-verified text"),
        ("printing", "Digital-link SVE identity is absent from reviewed registry"),
        ("sve_locator", "Digital-link SVE printing face source mismatch"),
        ("provider", "Digital-link name provider mismatch"),
        ("language", "Digital-link name language mismatch"),
        ("missing_language", "Digital-link frozen name language closure mismatch"),
        ("target", "Digital-link target phase or language is absent"),
    ],
)
def test_source_refusals_rollback(
    baseline: Fixture,
    database: DatabaseTemplate,
    tmp_path: Path,
    fault: str,
    message: str,
) -> None:

    fixture = copied(baseline, tmp_path / "repo")
    record = object_value(parse(baseline.record))
    value = object_value(record["value"])
    sve = object_value(array(value["sve_names"])[0])
    digital = [object_value(n) for n in array(value["digital_names"])]
    if fault == "source_hash":
        object_value(sve["name_ref"])["text_hash"] = digest(b"Synthetix card")
    elif fault == "printing":
        sve["printing_id"] = "p:" + "9" * 32
    elif fault == "sve_locator":
        object_value(sve["name_ref"]).update(
            locator="/faces/0/text", text_hash=digest(b"Synthetic rule.")
        )
    elif fault == "provider":
        object_value(digital[0]["name_ref"])["parser"] = "translation-sv1-v1"
    elif fault == "language":
        digital[1]["name_ref"] = digital[0]["name_ref"]
    elif fault == "missing_language":
        value["digital_names"] = [digital[0]]
    elif fault == "target":
        object_value(record["subject"])["official_id"] = "22345679"
    fixture = with_records(fixture, [record])
    with database.copy() as db:
        fixture.publish(db)
        before = {
            t: db.rows(t)
            for t in (
                "digital_link",
                "decision",
                "source_record",
                "digital_card",
                "text_unit",
            )
        }
        with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
            import_links(
                db,
                fixture.inputs(),
                build=fixture.build,
                stores={"test-store": fixture.store},
            )
        assert {t: db.rows(t) for t in before} == before


def test_owner_source_must_replay_its_printing(
    baseline: Fixture, database: DatabaseTemplate
) -> None:
    with database.copy() as db:
        baseline.publish(db)
        result = import_links(
            db,
            baseline.inputs(),
            build=baseline.build,
            stores={"test-store": baseline.store},
        )
        with db.transaction():
            source = next(
                r.values
                for r in db.rows("source_record")
                if r.values["kind"] == "official_api"
            )
            db.update(
                "face_revision", {"id": "link-revision"}, {"source_id": source["id"]}
            )
        with pytest.raises(
            ValueError,
            match=r"^Digital-link owner lacks frozen printing face evidence$",
        ):
            result.eligible_owner(
                db, baseline.sources(), NameOwner("face_revision", "link-revision")
            )


@pytest.mark.parametrize(
    ("change", "stale"),
    [("name", True), ("unrelated", False), ("missing_target", True)],
)
def test_current_catalogue_marks_changed_names_stale(
    baseline: Fixture,
    database: DatabaseTemplate,
    tmp_path: Path,
    change: str,
    stale: bool,
) -> None:

    def transform(data: dict[str, JsonValue], lang: str) -> None:
        common = object_value(
            object_value(object_value(data["card_details"])["22345678"])["common"]
        )
        if change == "name" and lang == "zh-Hant":
            common["name"] = "改動測試名"
        elif change == "missing_target" and lang == "zh-Hant":
            common["name"] = ""
        elif change == "unrelated":
            common["synthetic_unused_field"] = "changed"

    fixture = current_api(copied(baseline, tmp_path / "repo"), transform)
    with database.copy() as db:
        baseline.publish(db)
        result = import_links(
            db,
            fixture.inputs(),
            build=fixture.build,
            stores={"test-store": fixture.store},
        )
        assert bool(result.stale) is stale
        assert len(result.fresh) == (0 if stale else 1)
        assert db.rows("digital_link_coverage") == ()
        assert (
            bool(
                result.eligible_owner(
                    db, fixture.sources(), NameOwner("face_revision", "link-revision")
                )
            )
            is not stale
        )


def test_current_api_selected_subset_is_not_complete_evidence(
    baseline: Fixture, database: DatabaseTemplate
) -> None:

    config = object_value(parse(baseline.build.configuration.encode()))
    ref = next(
        ref for ref in baseline.refs if baseline.sources().document(ref)[0] == "ja"
    )
    config.update(configuration((ref,), (("svwb", "22345678"),)))
    with database.copy() as db:
        baseline.publish(db)
        with pytest.raises(
            ValueError, match=r"^Digital-link current API name closure is incomplete$"
        ):
            import_links(
                db,
                baseline.inputs(),
                build=baseline.changed(config),
                stores={"test-store": baseline.store},
            )


@pytest.fixture(scope="module")
def dual(tmp_path_factory: pytest.TempPathFactory) -> Fixture:
    return make_fixture(tmp_path_factory.mktemp("digital-link-dual"), dual=True)


def test_owner_rechecks_materialized_target_against_adoption(
    dual: Fixture, database: DatabaseTemplate
) -> None:
    with database.copy() as db:
        dual.publish(db)
        result = import_links(
            db, dual.inputs(), build=dual.build, stores={"test-store": dual.store}
        )
        link = next(
            r.values
            for r in db.rows("digital_link")
            if r.values["card_id"] == dual.card.id
        )
        with db.transaction():
            db.update(
                "digital_link",
                {"id": link["id"]},
                {
                    "digital_card_id": "digital:svwb:22345679",
                    "digital_face_id": "digital:svwb:22345679:normal",
                },
            )
        with pytest.raises(
            ValueError,
            match=r"^Digital-link materialized relation differs from adoption$",
        ):
            result.eligible_owner(
                db, dual.sources(), NameOwner("face_revision", "link-revision")
            )


def test_sampled_link_keeps_its_review_level(
    baseline: Fixture, database: DatabaseTemplate, tmp_path: Path
) -> None:
    record = object_value(parse(baseline.record))
    record["review_level"] = "sampled"
    fixture = with_records(copied(baseline, tmp_path / "repo"), [record])
    with database.copy() as db:
        fixture.publish(db)
        import_links(
            db,
            fixture.inputs(),
            build=fixture.build,
            stores={"test-store": fixture.store},
        )
        link = next(
            r.values
            for r in db.rows("digital_link")
            if r.values["card_id"] == fixture.card.id
        )
        decision = next(
            r.values
            for r in db.rows("decision")
            if r.values["id"] == link["decision_id"]
        )
        assert (decision["state"], decision["scope"]) == ("sampled", "record")


def test_evolved_phase_is_explicit_and_not_inferred_from_sve_front(
    baseline: Fixture, database: DatabaseTemplate, tmp_path: Path
) -> None:
    record = object_value(parse(baseline.record))
    object_value(record["subject"])["digital_phase"] = "evolved"
    for name in array(object_value(record["value"])["digital_names"]):
        object_value(name)["phase"] = "evolved"
    fixture = with_records(copied(baseline, tmp_path / "repo"), [record])
    with database.copy() as db:
        fixture.publish(db)
        result = import_links(
            db,
            fixture.inputs(),
            build=fixture.build,
            stores={"test-store": fixture.store},
        )
        link = next(
            r.values
            for r in db.rows("digital_link")
            if r.values["card_id"] == fixture.card.id
        )
        assert link["digital_face_id"] == "digital:svwb:22345678:evolved"
        assert result.eligible_owner(
            db, fixture.sources(), NameOwner("face_revision", "link-revision")
        )


def test_jp_owner_does_not_probe_same_number_english_printing(
    baseline: Fixture, database: DatabaseTemplate
) -> None:
    with database.copy() as db:
        baseline.publish(db)
        result = import_links(
            db,
            baseline.inputs(),
            build=baseline.build,
            stores={"test-store": baseline.store},
        )
        sources = baseline.sources()
        index = Evidence(sources).index(review_context(sources))
        # A valid typed registry boundary may put an EN printing ahead of JP.
        english = EnglishPrintingData.model_validate_json(
            canonical(
                {
                    **baseline.printing.model_dump(mode="json"),
                    "id": "p:" + "9" * 32,
                    "region": "en",
                    "cross_region_review": {
                        "checked": True,
                        "target_jp_card_no": baseline.printing.card_no,
                        "target_observation": baseline.printing.observation.model_dump(
                            mode="json"
                        ),
                    },
                }
            )
        )
        sources.identity_indexes[canonical(sources.build.model_dump(mode="json"))] = (
            replace(
                index,
                printings=dict(index.printings) | {english.id: english},
                by_card={baseline.card.id: (english, *index.by_card[baseline.card.id])},
            )
        )
        assert result.eligible_owner(
            db, sources, NameOwner("face_revision", "link-revision")
        )


@pytest.mark.parametrize("fault", ["missing", "changed"])
def test_composer_requires_exact_authored_configuration(
    baseline: Fixture,
    database: DatabaseTemplate,
    fault: str,
) -> None:
    config = object_value(parse(baseline.build.configuration.encode()))
    if fault == "missing":
        config.pop("digital_link_authored")
    else:
        object_value(config["digital_link_authored"])["index_hash"] = digest(b"other")
    with database.copy() as db:
        baseline.publish(db)
        before = db.rows("digital_link")
        with pytest.raises(
            ValueError,
            match=r"^Build configuration does not pin digital-link authored bytes$",
        ):
            import_links(
                db,
                baseline.inputs(),
                build=baseline.changed(config),
                stores={"test-store": baseline.store},
            )
        assert db.rows("digital_link") == before


def test_declared_batches_must_be_unique(baseline: Fixture) -> None:
    config = object_value(parse(baseline.build.configuration.encode()))
    config["digital_link_sources"] = array(config["digital_link_sources"]) * 2
    sources = Sources(
        {"test-store": baseline.store}, baseline.root, baseline.changed(config)
    )
    with pytest.raises(
        ValueError, match=r"^Digital-link build batches must be sorted and unique$"
    ):
        review_context(sources)


def test_owner_requires_same_build_context(
    baseline: Fixture,
    database: DatabaseTemplate,
) -> None:
    with database.copy() as db:
        baseline.publish(db)
        result = import_links(
            db,
            baseline.inputs(),
            build=baseline.build,
            stores={"test-store": baseline.store},
        )
        config = object_value(parse(baseline.build.configuration.encode()))
        config["synthetic_changed_background"] = True
        sources = Sources(
            {"test-store": baseline.store}, baseline.root, baseline.changed(config)
        )
        with pytest.raises(
            ValueError, match=r"^Digital-link name proof uses another build context$"
        ):
            result.eligible_owner(
                db, sources, NameOwner("face_revision", "link-revision")
            )


def test_owner_requires_japanese_name(
    baseline: Fixture,
    database: DatabaseTemplate,
) -> None:
    with database.copy() as db:
        baseline.publish(db)
        result = import_links(
            db,
            baseline.inputs(),
            build=baseline.build,
            stores={"test-store": baseline.store},
        )
        with db.transaction():
            unit = TextInterner(db).intern(
                LocalizedText(lang="en", text="Synthetic card")
            )
            db.update("face_revision", {"id": "link-revision"}, {"name_unit_id": unit})
        with pytest.raises(
            ValueError, match=r"^Name build source language differs from owner region$"
        ):
            result.eligible_owner(
                db, baseline.sources(), NameOwner("face_revision", "link-revision")
            )


def test_evidence_resolver_requires_same_review(baseline: Fixture) -> None:

    config = object_value(parse(baseline.build.configuration.encode()))
    config["synthetic_changed_background"] = True
    sources = Sources(
        {"test-store": baseline.store}, baseline.root, baseline.changed(config)
    )
    with pytest.raises(
        ValueError,
        match=r"^Digital-link evidence resolver differs from review context$",
    ):
        Evidence(sources).validate(
            Record.model_validate_json(baseline.record),
            review_context(baseline.sources()),
        )


def test_sve_evidence_must_belong_to_same_card(dual: Fixture) -> None:

    record = Record.model_validate_json(dual.record)
    other_card, other_face, other_printing, other_ref = dual.others[0]
    assert other_card.id != record.subject.card_id
    evidence = Evidence(dual.sources())
    with pytest.raises(
        ValueError, match=r"^Digital-link SVE evidence belongs to another card$"
    ):
        evidence.sve(
            SveName(
                printing_id=other_printing.id, face_id=other_face.id, name_ref=other_ref
            ),
            record,
            review_context(dual.sources()),
        )


def test_digital_locator_cannot_borrow_other_identical_name(dual: Fixture) -> None:

    record = Record.model_validate_json(dual.record)
    name = next(n for n in record.value.digital_names if n.lang == "ja")
    ref = name.name_ref.model_copy(
        update={"locator": "/data/card_details/22345679/common/name"}
    )
    sources = dual.sources()
    names = inventory(
        sources, batch_refs(sources, review_context(sources).source_batches, "svwb")
    )
    with pytest.raises(
        ValueError,
        match=r"^Digital-link name locator belongs to another target or field$",
    ):
        Evidence(sources).digital(
            name.model_copy(update={"name_ref": ref}), record, names
        )


def test_digital_name_must_equal_supplied_frozen_inventory(baseline: Fixture) -> None:

    record = Record.model_validate_json(baseline.record)
    name = next(n for n in record.value.digital_names if n.lang == "ja")
    sources = baseline.sources()
    names = inventory(
        sources, batch_refs(sources, review_context(sources).source_batches, "svwb")
    )
    key = ("svwb", "22345678", "normal", "ja")
    names[key] = replace(names[key], text="Synthetix card")
    with pytest.raises(
        ValueError, match=r"^Digital-link name differs from frozen inventory$"
    ):
        Evidence(sources).digital(name, record, names)


def test_current_registry_cannot_omit_adopted_card(
    baseline: Fixture,
    database: DatabaseTemplate,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    original = Evidence.index

    def index(self: Evidence, review: ReviewContext) -> RegistryIndex:
        return replace(original(self, review), cards={})

    monkeypatch.setattr(Evidence, "index", index)
    with database.copy() as db:
        baseline.publish(db)
        before = db.rows("digital_link")
        with pytest.raises(
            ValueError, match=r"^Digital-link current registry card is unknown$"
        ):
            import_links(
                db,
                baseline.inputs(),
                build=baseline.build,
                stores={"test-store": baseline.store},
            )
        assert db.rows("digital_link") == before


def test_legacy_digital_import_does_not_allow_declared_target_superset(
    baseline: Fixture,
    database: DatabaseTemplate,
) -> None:
    config = object_value(parse(baseline.build.configuration.encode()))
    config.update(
        configuration(baseline.refs, (("svwb", "22345678"), ("svwb", "22345679")))
    )
    sources = Sources(
        {"test-store": baseline.store}, baseline.root, baseline.changed(config)
    )
    with database.copy() as db:
        with pytest.raises(
            ValueError, match=r"^Build configuration does not pin digital inputs$"
        ):
            import_digital(db, sources, baseline.refs, (("svwb", "22345678"),))
