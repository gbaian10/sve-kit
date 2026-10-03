"""Real synthetic frozen entry-to-DB-to-name wiring with reusable construction."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

import sve_carddb.catalog.adoption_sources as pinned_module
import sve_carddb.digital_links.importer as importer_module
import sve_carddb.translations.sources as sources_module
from sve_carddb.build_inputs import BuildContext
from sve_carddb.digital_links.evidence import (
    Evidence,
    RegistryIndex,
    batch_refs,
    inventory,
)
from sve_carddb.digital_links.importer import Inputs, import_links, review_context
from sve_carddb.digital_links.models import Record, SveName
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.products.models import LocalizedText
from sve_carddb.registry.records import EnglishPrintingData
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.source_archive import ArchiveError
from sve_carddb.text_observations.intern import TextInterner
from sve_carddb.translations.digital import configuration, import_digital, select_name
from sve_carddb.translations.names import populate_name_translation
from sve_carddb.translations.sources import Sources

from .adoption_fixtures import commit
from .digital_link_fixtures import envelope, write
from .digital_link_import_fixtures import (
    Fixture,
    copied,
    current_api,
    make_fixture,
    signed,
)
from .translation_fixtures import template

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.catalog.adoption_models import ReviewContext
    from sve_carddb.digital_links.loader import Snapshot
    from sve_carddb.digital_links.models import Shard

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
        assert result.stale == result.withdrawn == ()
        chosen = select_name(
            db, card_id=baseline.card.id, face_id=baseline.face.id, lang="zh-Hant"
        )
        assert chosen is not None
        assert chosen[:2] == ("合成測試名", "official_svwb")
        assert db.rows("digital_link_coverage") == ()
        assert result.eligible(db, baseline.sources(), "link-revision")
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


def test_name_wiring_requires_proof_and_rechecks_each_owner(
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
        with pytest.raises(
            ValueError, match=r"^Authored digital names require owner evidence result$"
        ):
            populate_name_translation(
                db, baseline.sources(), revision_id="link-revision", lang="zh-Hant"
            )
        with db.transaction():
            translated = populate_name_translation(
                db,
                baseline.sources(),
                revision_id="link-revision",
                lang="zh-Hant",
                links=result,
            )
            assert translated is not None
            revision = next(
                r.values
                for r in db.rows("face_revision")
                if r.values["id"] == "link-revision"
            )
            # A same-card back face sharing the name/context still has no adopted link.
            db.insert(
                "face",
                {
                    "id": "f:" + "9" * 32,
                    "card_id": baseline.card.id,
                    "ordinal": 1,
                    "side": "back",
                },
            )
            db.insert(
                "face_revision",
                dict(revision) | {"id": "back-revision", "face_id": "f:" + "9" * 32},
            )
            before = db.rows("translation")
            assert (
                populate_name_translation(
                    db,
                    baseline.sources(),
                    revision_id="back-revision",
                    lang="zh-Hant",
                    links=result,
                )
                is None
            )
            assert db.rows("translation") == before
            # Exact owner hash differs; same face and shared build context are insufficient.

            unit = TextInterner(db).intern(
                LocalizedText(lang="ja", text="Synthetic different name")
            )
            db.update("face_revision", {"id": "link-revision"}, {"name_unit_id": unit})
            assert (
                populate_name_translation(
                    db,
                    baseline.sources(),
                    revision_id="link-revision",
                    lang="zh-Hant",
                    links=result,
                )
                is None
            )
            assert db.rows("translation") == before


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
    data = object_value(record["data"])
    value = object_value(data["value"])
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
        object_value(data["subject"])["official_id"] = "22345679"
        record["record_key"] = canonical(
            ["digital_link_adoption", data["subject"], 1]
        ).decode()
    record["evidence"] = list(
        {
            canonical(item): item
            for item in sorted(
                [
                    {"source_ref": sve["name_ref"], "role": "sve_name"},
                    *[
                        dict[str, JsonValue](
                            source_ref=object_value(n)["name_ref"], role="digital_name"
                        )
                        for n in array(value["digital_names"])
                    ],
                ],
                key=canonical,
            )
        }.values()
    )
    fixture = signed(fixture, [record])
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


@pytest.mark.parametrize("relation", ["same_character", "name_only"])
def test_confirmed_browsing_relation_never_supplies_name(
    baseline: Fixture, database: DatabaseTemplate, tmp_path: Path, relation: str
) -> None:

    record = object_value(parse(baseline.record))
    object_value(object_value(record["data"])["value"])["relation"] = relation
    fixture = signed(copied(baseline, tmp_path / "repo"), [record])
    with database.copy() as db:
        fixture.publish(db)
        result = import_links(
            db,
            fixture.inputs(),
            build=fixture.build,
            stores={"test-store": fixture.store},
        )
        assert len(result.fresh) == 1
        assert not result.eligible(db, fixture.sources(), "link-revision")
        assert (
            populate_name_translation(
                db,
                fixture.sources(),
                revision_id="link-revision",
                lang="zh-Hant",
                links=result,
            )
            is None
        )


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
            result.eligible(db, baseline.sources(), "link-revision")


def test_equal_length_raw_tamper_and_complete_source_closure(
    baseline: Fixture, database: DatabaseTemplate, tmp_path: Path
) -> None:

    with database.copy() as db:
        baseline.publish(db)
        result = import_links(
            db,
            baseline.inputs(),
            build=baseline.build,
            stores={"test-store": baseline.store},
        )
        with pytest.raises(
            ValueError, match=r"^Build input use closure or context mismatch$"
        ):
            result.record.verify(db, baseline.build, result.record.uses[:-1])
        result.record.verify(db, baseline.build, result.record.uses)
        with db.transaction():
            source = dict(
                next(
                    r.values
                    for r in db.rows("source_record")
                    if r.values["kind"] != "authored"
                )
            )
            source["id"] = "synthetic:unrelated-source"
            db.insert("source_record", source)
        # The composing build must account for unrelated raw inputs too.
        with pytest.raises(
            ValueError, match=r"^Build input raw source closure mismatch$"
        ):
            result.record.verify(db, baseline.build, result.record.uses)
    fixture = copied(baseline, tmp_path / "repo")
    frozen = FrozenSources(fixture.store, "test-store", fixture.jp.batch_id)
    path = fixture.store / frozen.entries[fixture.jp.source_version_id].blob.path
    raw = path.read_bytes()
    changed = raw.replace(b"Synthetic card", b"Synthetix card")
    assert len(changed) == len(raw)
    assert changed != raw
    path.write_bytes(changed)
    with database.copy() as db:
        with pytest.raises(
            ArchiveError,
            match="^" + re.escape(f"archive file hash or size mismatch: {path}") + "$",
        ):
            import_links(
                db,
                fixture.inputs(),
                build=fixture.build,
                stores={"test-store": fixture.store},
            )


@pytest.mark.parametrize(
    ("change", "stale"),
    [("name", True), ("unrelated", False), ("missing_target", True)],
)
def test_current_replay_marks_changed_names_stale_without_resurrection(
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
            bool(result.eligible(db, fixture.sources(), "link-revision")) is not stale
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


def test_withdrawal_is_terminal_but_preserves_historical_audit(
    baseline: Fixture, database: DatabaseTemplate, tmp_path: Path
) -> None:

    fixture = copied(baseline, tmp_path / "repo")
    first = object_value(parse(fixture.record))
    shard = object_value(parse(fixture.shard))
    second = object_value(parse(fixture.record))
    data = object_value(second["data"])
    data.update(
        adoption_no=2,
        predecessor={
            "record_key": first["record_key"],
            "record_hash": digest(canonical(first)),
            "decision_id": shard["default_decision_id"],
        },
        value=None,
    )
    second["record_key"] = canonical(
        ["digital_link_adoption", data["subject"], 2]
    ).decode()
    second["evidence"] = []
    last = envelope([second])
    last["review_context"] = shard["review_context"]
    write(
        fixture.root / "authored",
        {
            "digital-links/links/synthetic/001.yaml": shard,
            "digital-links/links/synthetic/002.yaml": last,
        },
    )
    fixture = replace(fixture, authored=commit(fixture.root))
    config = object_value(parse(fixture.build.configuration.encode()))
    config.update(
        Inputs(
            fixture.root / "authored", fixture.root, fixture.authored
        ).configuration()
    )
    fixture = replace(fixture, build=fixture.changed(config))
    with database.copy() as db:
        fixture.publish(db)
        result = import_links(
            db,
            fixture.inputs(),
            build=fixture.build,
            stores={"test-store": fixture.store},
        )
        assert result.withdrawn == (second["record_key"],)
        assert result.fresh == ()
        assert all(
            r.values["card_id"] != fixture.card.id for r in db.rows("digital_link")
        )
        assert (
            len(
                [
                    r
                    for r in db.rows("decision")
                    if r.values["category"] == "digital_link"
                ]
            )
            == 2
        )
        assert any(
            r.values["authored_path"]
            == "authored/digital-links/links/synthetic/001.yaml"
            for r in db.rows("source_record")
        )


@pytest.fixture(scope="module")
def dual(tmp_path_factory: pytest.TempPathFactory) -> Fixture:
    return make_fixture(tmp_path_factory.mktemp("digital-link-dual"), dual=True)


def test_two_same_name_cards_need_context_assignment_and_third_is_not_adopted(
    dual: Fixture, database: DatabaseTemplate
) -> None:
    with database.copy() as db:
        dual.publish(db)
        result = import_links(
            db, dual.inputs(), build=dual.build, stores={"test-store": dual.store}
        )
        assert len(result.fresh) == 2
        first = select_name(
            db,
            card_id=dual.card.id,
            face_id=dual.face.id,
            lang="zh-Hant",
            eligible_links=result.eligible(db, dual.sources(), "link-revision"),
        )
        second = select_name(
            db,
            card_id=dual.others[0][0].id,
            face_id=dual.others[0][1].id,
            lang="zh-Hant",
            eligible_links=result.eligible(db, dual.sources(), "other-link-revision-1"),
        )
        assert first is not None
        assert first[0] == "合成測試名"
        assert second is not None
        assert second[0] == "第二個測試譯名"
        with pytest.raises(
            ValueError,
            match=r"^Ambiguous source name requires adopted context assignment$",
        ):
            populate_name_translation(
                db,
                dual.sources(),
                revision_id="link-revision",
                lang="zh-Hant",
                links=result,
            )
        with db.transaction():
            card = dict(dual.card.model_dump(mode="json"))
            card["id"] = "c:" + "9" * 32
            db.insert("card", card)
            db.insert(
                "face",
                {
                    "id": "f:" + "8" * 32,
                    "card_id": card["id"],
                    "ordinal": 0,
                    "side": "front",
                },
            )
            revision = next(
                r.values
                for r in db.rows("face_revision")
                if r.values["id"] == "link-revision"
            )
            db.insert(
                "face_revision",
                dict(revision)
                | {"id": "third-link-revision", "face_id": "f:" + "8" * 32},
            )
            assert (
                populate_name_translation(
                    db,
                    dual.sources(),
                    revision_id="third-link-revision",
                    lang="zh-Hant",
                    links=result,
                )
                is None
            )


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
            result.eligible(db, dual.sources(), "link-revision")


def test_authored_byte_pin_and_runtime_closure(
    baseline: Fixture, tmp_path: Path
) -> None:
    fixture = copied(baseline, tmp_path / "repo")
    shard = fixture.root / "authored/digital-links/links/synthetic/001.yaml"
    shard.write_bytes(shard.read_bytes() + b"\n")
    with pytest.raises(
        ValueError,
        match=r"^Digital-link bytes differ from immutable authored revision$",
    ):
        fixture.inputs().load()
    # New validator/candidate files cannot be omitted while the old parser pin stays valid.
    pin = next(
        p
        for p in baseline.build.dependencies
        if p.name.endswith("digital_links/candidates.py")
    )
    config = object_value(parse(baseline.build.configuration.encode()))
    build = BuildContext.from_inputs(
        baseline.program,
        {
            p.name: (baseline.root / p.name).read_bytes()
            for p in baseline.build.dependencies
            if p != pin
        },
        config,
    )
    with pytest.raises(
        ValueError, match=r"^Translation runtime/dependency closure cannot be replayed$"
    ):
        Sources({"test-store": baseline.store}, baseline.root, build)


def test_evolved_phase_is_explicit_and_not_inferred_from_sve_front(
    baseline: Fixture, database: DatabaseTemplate, tmp_path: Path
) -> None:
    record = object_value(parse(baseline.record))
    data = object_value(record["data"])
    subject = object_value(data["subject"])
    subject["digital_phase"] = "evolved"
    record["record_key"] = canonical(["digital_link_adoption", subject, 1]).decode()
    for name in array(object_value(data["value"])["digital_names"]):
        object_value(name)["phase"] = "evolved"
    fixture = signed(copied(baseline, tmp_path / "repo"), [record])
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
        assert result.eligible(db, fixture.sources(), "link-revision")


def test_composer_verifies_actual_raw_metadata_before_returning(
    baseline: Fixture, database: DatabaseTemplate, monkeypatch: pytest.MonkeyPatch
) -> None:
    original = importer_module._audit

    def corrupted(
        db: Database,
        snapshot: Snapshot,
        inputs: Inputs,
        resolved: list[tuple[Shard, Sources]],
    ) -> None:
        original(db, snapshot, inputs, resolved)
        source = next(
            r.values for r in db.rows("source_record") if r.values["kind"] != "authored"
        )
        db.update(
            "source_record",
            {"id": source["id"]},
            {"etag": "Synthetic unexpected metadata"},
        )

    monkeypatch.setattr(importer_module, "_audit", corrupted)
    with database.copy() as db:
        baseline.publish(db)
        before = db.rows("source_record")
        with pytest.raises(ValueError, match=r"^Conflicting raw source metadata$"):
            import_links(
                db,
                baseline.inputs(),
                build=baseline.build,
                stores={"test-store": baseline.store},
            )
        assert db.rows("source_record") == before


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
        assert result.eligible(db, sources, "link-revision")


def test_card_level_relation_is_browsable_but_never_supplies_owner_name(
    baseline: Fixture, database: DatabaseTemplate, tmp_path: Path
) -> None:
    record = object_value(parse(baseline.record))
    subject = object_value(object_value(record["data"])["subject"])
    subject.update(face_id=None, digital_phase=None)
    record["record_key"] = canonical(["digital_link_adoption", subject, 1]).decode()
    fixture = signed(copied(baseline, tmp_path / "repo"), [record])
    with database.copy() as db:
        fixture.publish(db)
        result = import_links(
            db,
            fixture.inputs(),
            build=fixture.build,
            stores={"test-store": fixture.store},
        )
        assert len(result.fresh) == 1
        row = next(
            r.values
            for r in db.rows("digital_link")
            if r.values["card_id"] == fixture.card.id
        )
        assert row["face_id"] is None
        assert row["digital_face_id"] is None
        assert not result.eligible(db, fixture.sources(), "link-revision")
        assert (
            populate_name_translation(
                db,
                fixture.sources(),
                revision_id="link-revision",
                lang="zh-Hant",
                links=result,
            )
            is None
        )


@pytest.mark.parametrize("name", ["commands.py", "sources.py"])
def test_historical_review_survives_new_runtime(
    baseline: Fixture,
    database: DatabaseTemplate,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    name: str,
) -> None:

    fixture = copied(baseline, tmp_path / "repo")
    relative = (
        "carddb/src/sve_carddb/digital_links/commands.py"
        if name == "commands.py"
        else "carddb/src/sve_carddb/translations/sources.py"
    )
    path = fixture.root / relative
    path.write_bytes(path.read_bytes() + b"\n# Synthetic later runtime revision.\n")
    program = commit(fixture.root)
    config = object_value(parse(fixture.build.configuration.encode()))
    for recipe in object_value(config["translation_recipes"]).values():
        pin = object_value(recipe)
        pin["program_revision"] = program
        pin["code_hash"] = digest((fixture.root / str(pin["code_path"])).read_bytes())
    fixture = replace(fixture, program=program)
    fixture = replace(fixture, build=fixture.changed(config))
    monkeypatch.setattr(
        sources_module,
        "__file__",
        str(fixture.root / "carddb/src/sve_carddb/translations/sources.py"),
    )
    monkeypatch.setattr(
        pinned_module,
        "__file__",
        str(fixture.root / "carddb/src/sve_carddb/catalog/adoption_sources.py"),
    )
    with database.copy() as db:
        fixture.publish(db)
        result = import_links(
            db,
            fixture.inputs(),
            build=fixture.build,
            stores={"test-store": fixture.store},
        )
        assert len(result.fresh) == 1
        assert result.stale == result.withdrawn == ()
        assert result.eligible(db, fixture.sources(), "link-revision")
    assert fixture.inputs().load().shards == baseline.inputs().load().shards


def test_authored_revision_must_be_full_sha(baseline: Fixture) -> None:
    inputs = replace(baseline.inputs(), authored_revision=baseline.authored[:7])
    with pytest.raises(
        ValueError, match=r"^Digital-link authored revision must be full Git SHA$"
    ):
        inputs.load()


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
            result.eligible(db, sources, "link-revision")


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
            result.eligible(db, baseline.sources(), "link-revision")


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
    assert other_card.id != record.data.subject.card_id
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
    assert record.data.value is not None
    name = next(n for n in record.data.value.digital_names if n.lang == "ja")
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
    assert record.data.value is not None
    name = next(n for n in record.data.value.digital_names if n.lang == "ja")
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
    historical = object_value(parse(baseline.shard))["review_context"]

    def index(self: Evidence, review: ReviewContext) -> RegistryIndex:

        result = original(self, review)
        if review.model_dump(mode="json") != historical:
            return replace(result, cards={})
        return result

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


@pytest.mark.parametrize("fault", ["dependency", "recipe"])
def test_historical_mode_still_checks_immutable_git_pins(
    baseline: Fixture, fault: str
) -> None:
    config = object_value(parse(baseline.build.configuration.encode()))
    build = baseline.build
    if fault == "recipe":
        recipe = object_value(
            object_value(config["translation_recipes"])["translation-jp-v1"]
        )
        recipe["code_hash"] = digest(b"Synthetic wrong old parser")
        build = baseline.changed(config)
        sources = Sources(
            {"test-store": baseline.store}, baseline.root, build, historical=True
        )
        with pytest.raises(ValueError, match=r"^Recipe program/config hash mismatch$"):
            sources.text(baseline.jp)
    else:
        pin = build.dependencies[0].model_copy(
            update={"sha256": digest(b"Synthetic wrong old dependency")}
        )
        build = build.model_copy(
            update={"dependencies": (pin, *build.dependencies[1:])}
        )
        with pytest.raises(ValueError, match=r"^Review dependency hash mismatch$"):
            Sources(
                {"test-store": baseline.store}, baseline.root, build, historical=True
            )
