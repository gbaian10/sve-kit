"""Human admission, complete-list reconciliation and fail-closed offline projection."""

import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import array, canonical, digest, object_value, parse
from sve_carddb.core.provenance import BuildContext, InputRecord
from sve_carddb.domains.construction.adoption import prepare
from sve_carddb.domains.construction.adoption_loader import load
from sve_carddb.domains.construction.adoption_reports import (
    REQUIRED_DEPENDENCIES,
    StorePin,
)
from sve_carddb.export.project import Decisions, project
from sve_carddb.export.project import Settings as ProjectSettings
from sve_carddb.workflows.offline import require_offline_coverage

from ...support.build_db_fixtures import rows
from ...support.card_extras_fixtures import seed
from ...support.construction_adoption_fixtures import (
    AdoptionFixture,
    append_successor,
    create,
)
from ...support.source_import_fixtures import git

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

    from sve_carddb.build import Database


@pytest.fixture(scope="module")
def template(tmp_path_factory: pytest.TempPathFactory) -> AdoptionFixture:
    return create(tmp_path_factory.mktemp("construction-human"))


@pytest.fixture
def authored(template: AdoptionFixture, tmp_path: Path) -> AdoptionFixture:
    shutil.copytree(template.directory, tmp_path / "copy")
    directory = tmp_path / "copy"
    settings = template.settings
    assert settings is not None
    copied = settings.model_copy(
        update={
            "source_stores": {template.store_id: directory / "archive"},
            "review_stores": (
                StorePin(
                    store_id="synthetic-reviews",
                    root=directory / "reviews",
                    backup=directory / "backup",
                    restored=directory / "restored",
                    inventory_sha256=settings.review_stores[0].inventory_sha256,
                ),
            ),
        }
    )
    return replace(
        template,
        repository=directory / "program",
        directory=directory,
        archive=directory / "archive",
        context=object_value(parse(canonical(template.context))),
        records=[
            object_value(value)
            for value in array(parse(canonical(list(template.records))))
        ],
        settings=copied,
        report_hashes=set(template.report_hashes),
    )


@pytest.fixture
def db() -> Iterator[Database]:
    schema = compile_build(("cr",))
    values = rows()
    text = values["text_unit"]["text"]
    assert isinstance(text, str)
    text_hash = digest(text.encode())
    text_id = "t:ja:" + text_hash[7:23]
    with create_database(schema) as baseline:
        seed(baseline)
        initial = [
            (table.name, dict(row.values))
            for table in schema.tables
            for row in baseline.rows(table.name)
        ]
    with create_database(schema) as database:
        with database.transaction():
            for table, raw in initial:
                fields = {
                    key: text_id if value == "text" else value
                    for key, value in raw.items()
                }
                if table == "text_unit":
                    fields["content_hash"] = text_hash
                database.insert(table, fields)
            for printing, number in (
                ("printing", 20001),
                ("printing1", 20002),
                ("printing2", 10001),
            ):
                database.insert(
                    "card_int_id", {"printing_id": printing, "int_id": number}
                )
            for identifier, face, name in (
                ("rules_name", "face", "Synthetic name"),
                ("rules_name2", "face1", "Synthetic two"),
            ):
                database.insert(
                    "rules_name",
                    values["rules_name"] | {"id": identifier, "official_name": name},
                )
                database.insert(
                    "face_rules_name",
                    values["face_rules_name"]
                    | {"face_id": face, "rules_name_id": identifier},
                )
                database.insert(
                    "face_revision",
                    values["face_revision"]
                    | {
                        "id": "revision:" + face,
                        "face_id": face,
                        "name_unit_id": text_id,
                        "effect_unit_id": text_id,
                    },
                )
        yield database


def test_human_current_list_preserves_date_evidence_and_unknown_cr(
    authored: AdoptionFixture, db: Database
) -> None:
    assert authored.settings is not None
    prepared = prepare(
        authored.repository / "authored",
        authored.repository,
        authored.revision,
        authored.settings,
    )
    build = BuildContext.from_inputs(authored.tool_revision, prepared.configuration())
    with db.transaction():
        adopted = prepared.populate(db, build)
    assert [
        (row.values["max_copies"], row.values["state"])
        for row in db.rows("restriction")
    ] == [(0, "confirmed"), (1, "confirmed")]
    assert {row.values["deck_scope"] for row in db.rows("restriction_member")} == {
        "main"
    }
    assert not db.rows("cr_version")
    assert not db.rows("rules_profile_revision")
    sources = [
        row.values
        for row in db.rows("source_record")
        if row.values["observed_on"] is not None
    ]
    assert len(sources) == 1
    assert sources[0]["fetched_at"] is None
    assert sources[0]["observed_on"] == "2020-01-02"
    assert sources[0]["observed_timezone"] == "Asia/Taipei"
    assert sources[0]["parser_version"] is None
    assert len(adopted.record.date_uses) == 4
    assert InputRecord.model_validate_json(adopted.record.content()) == adopted.record
    projection = project(
        db,
        regions=("jp",),
        settings=ProjectSettings(
            "https://example.invalid/feedback", "synthetic", "synthetic"
        ),
        decisions=Decisions(),
    )
    require_offline_coverage(projection, errata=False, construction=adopted)
    assert projection.metadata["restriction_coverage"] == [
        {
            "profile_id": "standard",
            "from_date": "2020-01-02",
            "until_date": "2020-01-03",
            "state": "complete",
            "source_url": "https://shadowverse-evolve.com/card_limit/",
        }
    ]
    projection.tables["restriction"][0]["max_copies"] = 1
    with pytest.raises(ValueError, match="projection differs"):
        require_offline_coverage(projection, errata=False, construction=adopted)


def test_history_selects_the_successor_and_keeps_both_receipts(
    authored: AdoptionFixture, db: Database
) -> None:
    append_successor(authored)
    assert authored.settings is not None
    prepared = prepare(
        authored.repository / "authored",
        authored.repository,
        authored.revision,
        authored.settings,
    )
    with db.transaction():
        result = prepared.populate(
            db,
            BuildContext.from_inputs(authored.tool_revision, prepared.configuration()),
        )
    assert len(db.rows("restriction")) == 2
    assert len(result.record.date_uses) == 5
    assert len(db.rows("decision")) == 5


@pytest.mark.parametrize("missing_future", [False, True])
def test_instant_list_adopts_future_dates_without_extending_coverage(
    tmp_path: Path, db: Database, missing_future: bool
) -> None:
    fixture = create(tmp_path, instant=True, future=True)
    if missing_future:
        fixture.records = [
            record
            for record in fixture.records
            if not (
                record["kind"] == "construction_restriction"
                and object_value(object_value(record["data"])["value"])["max_copies"]
                == 1
            )
        ]
        fixture.write()
    assert fixture.settings is not None
    prepared = prepare(
        fixture.repository / "authored",
        fixture.repository,
        fixture.revision,
        fixture.settings,
    )
    build = BuildContext.from_inputs(fixture.tool_revision, prepared.configuration())
    if missing_future:
        with pytest.raises(ValueError, match="every active entry"), db.transaction():
            prepared.populate(db, build)
        assert not db.rows("restriction")
    else:
        with db.transaction():
            result = prepared.populate(db, build)
        assert not result.record.date_uses
        assert len(result.record.uses) == 4
        assert {row.values["effective_from"] for row in db.rows("restriction")} == {
            "2020-01-01",
            "2020-02-01",
        }
        assert db.rows("restriction_coverage")[0].values["until_date"] == "2020-01-03"
        assert InputRecord.model_validate_json(result.record.content()) == result.record


@pytest.mark.parametrize("wrong_card", [False, True])
def test_explicit_lift_reconciles_only_the_original_linked_card(
    tmp_path: Path, db: Database, wrong_card: bool
) -> None:
    fixture = create(tmp_path, instant=True, lift=True)
    if wrong_card:
        records = [
            record
            for record in fixture.records
            if record["kind"] == "construction_restriction"
        ]
        original = next(
            record for record in records if len(array(record["evidence"])) == 2
        )
        other = next(
            record for record in records if len(array(record["evidence"])) == 1
        )
        reconciliation = next(
            object_value(item)
            for item in array(original["evidence"])
            if not object_value(item)["primary"]
        )
        use = object_value(reconciliation["use"])
        locator = object_value(parse(str(use["locator"]).encode()))
        locator["record_key"] = other["record_key"]
        use["locator"] = canonical(locator).decode()
        array(original["evidence"]).remove(reconciliation)
        other["evidence"] = sorted(
            [*array(other["evidence"]), reconciliation], key=canonical
        )
        fixture.write()
    assert fixture.settings is not None
    prepared = prepare(
        fixture.repository / "authored",
        fixture.repository,
        fixture.revision,
        fixture.settings,
    )
    build = BuildContext.from_inputs(fixture.tool_revision, prepared.configuration())
    if wrong_card:
        with (
            pytest.raises(ValueError, match="Lift notice must match"),
            db.transaction(),
        ):
            prepared.populate(db, build)
        assert not db.rows("restriction")
    else:
        with db.transaction():
            result = prepared.populate(db, build)
        assert {row.values["effective_until"] for row in db.rows("restriction")} == {
            None,
            "2020-02-01",
        }
        assert len(result.record.uses) == 5
        assert {use.source.url for use in result.record.uses} == {
            "https://shadowverse-evolve.com/card_limit/",
            "https://shadowverse-evolve.com/news/post-900001/",
        }


def test_coverage_mode_cannot_escape_its_review_scope(
    authored: AdoptionFixture,
) -> None:
    coverage = next(
        record
        for record in authored.records
        if record["kind"] == "construction_coverage"
    )
    object_value(
        object_value(object_value(coverage["data"])["value"])["reconciliation"]
    )["mode"] = "observed-list-day-v1"
    authored.write()
    assert authored.settings is not None
    with pytest.raises(ValueError, match="Coverage mode differs"):
        prepare(
            authored.repository / "authored",
            authored.repository,
            authored.revision,
            authored.settings,
        )


def test_instant_evidence_cannot_use_the_original_date_only_review_scope(
    tmp_path: Path,
) -> None:
    fixture = create(tmp_path, instant=True)
    context = object_value(fixture.context["context"])
    configuration = object_value(parse(str(context["configuration"]).encode()))
    configuration["scope"] = "current-list-day-v1"
    configuration["dependencies"] = [
        pin
        for pin in array(configuration["dependencies"])
        if object_value(pin)["name"] in REQUIRED_DEPENDENCIES
    ]
    context["configuration"] = canonical(configuration).decode()
    fixture.write()
    assert fixture.settings is not None
    with pytest.raises(ValueError, match="Instant or reconciled evidence"):
        prepare(
            fixture.repository / "authored",
            fixture.repository,
            fixture.revision,
            fixture.settings,
        )


def test_history_rejects_a_signed_wrong_predecessor(authored: AdoptionFixture) -> None:
    append_successor(authored, broken=True)
    with pytest.raises(ValueError, match="exact prior adoption"):
        load(authored.repository / "authored", authored.repository, authored.revision)


@pytest.mark.parametrize(
    "change",
    [
        "extend_day",
        "missing_entry",
        "stale_dependency",
        "wrong_result",
        "extra_exclusion",
        "wrong_card_name",
    ],
)
def test_signed_incomplete_or_stale_values_roll_back(
    authored: AdoptionFixture, db: Database, change: str
) -> None:
    restriction = next(
        record
        for record in authored.records
        if record["kind"] == "construction_restriction"
    )
    coverage = next(
        record
        for record in authored.records
        if record["kind"] == "construction_coverage"
    )
    if change == "extend_day":
        object_value(object_value(coverage["data"])["value"])["until_date"] = (
            "2020-01-04"
        )
    elif change == "missing_entry":
        authored.records.remove(restriction)
    elif change == "stale_dependency":
        dependencies = array(object_value(restriction["data"])["dependencies"])
        object_value(dependencies[0])["basis_hash"] = "sha256:" + "a" * 64
    elif change == "wrong_result":
        use = object_value(object_value(array(restriction["evidence"])[0])["use"])
        locator = object_value(parse(str(use["locator"]).encode()))
        locator["result_hash"] = "sha256:" + "b" * 64
        use["locator"] = canonical(locator).decode()
    elif change == "extra_exclusion":
        object_value(
            object_value(object_value(coverage["data"])["value"])["reconciliation"]
        )["excluded_entry_pointers"] = ["/entries/0"]
    else:
        members = array(
            object_value(object_value(restriction["data"])["value"])["members"]
        )
        object_value(members[0])["rules_name_id"] = "rules_name2"
    authored.write()
    assert authored.settings is not None
    prepared = prepare(
        authored.repository / "authored",
        authored.repository,
        authored.revision,
        authored.settings,
    )
    expected = {
        "extend_day": "exact observed day",
        "missing_entry": "every active entry",
        "stale_dependency": "stale",
        "wrong_result": "locator result",
        "extra_exclusion": "Inactive",
        "wrong_card_name": "replayed list entry",
    }[change]
    with pytest.raises(ValueError, match=expected), db.transaction():
        prepared.populate(
            db,
            BuildContext.from_inputs(authored.tool_revision, prepared.configuration()),
        )
    assert not db.rows("rules_profile")
    assert not db.rows("restriction")
    assert not db.rows("restriction_coverage")
    assert not [
        row for row in db.rows("source_record") if row.values["observed_on"] is not None
    ]


@pytest.mark.parametrize(
    "change", ["missing", "damaged", "restore", "overlap", "unknown_store"]
)
def test_review_report_closure_is_required_before_admission(
    authored: AdoptionFixture, change: str
) -> None:
    assert authored.settings is not None
    store = authored.settings.review_stores[0]
    path = next((store.root / "reports").rglob("*.json"))
    if change == "missing":
        path.unlink()
    elif change == "damaged":
        path.write_bytes(b"{}")
    elif change == "restore":
        (store.restored / path.relative_to(store.root)).write_bytes(b"{}")
    elif change == "overlap":
        authored.settings = authored.settings.model_copy(
            update={"review_stores": (store.model_copy(update={"backup": store.root}),)}
        )
    else:
        authored.settings = authored.settings.model_copy(
            update={"review_stores": (store.model_copy(update={"store_id": "other"}),)}
        )
    with pytest.raises((ValueError, OSError)):
        prepare(
            authored.repository / "authored",
            authored.repository,
            authored.revision,
            authored.settings,
        )


@pytest.mark.parametrize(
    "change",
    [
        "same_vendor",
        "no_human",
        "wrong_member",
        "policy",
        "unknown_field",
        "bool_format",
        "orphan",
        "symlink",
    ],
)
def test_invalid_human_envelopes_fail_closed(  # ruff: ignore[complex-structure] -- Keep the complete admission boundary visible.
    authored: AdoptionFixture, change: str
) -> None:
    path = (
        authored.repository
        / "authored/construction-adoptions/restrictions/jp/standard/001.yaml"
    )
    shard = object_value(parse(path.read_bytes()))
    record = object_value(array(shard["records"])[0])
    if change == "same_vendor":
        reviews = array(object_value(record["review"])["model_reviews"])
        object_value(reviews[1])["vendor"] = "anthropic"
    elif change == "no_human":
        object_value(record["review"])["maintainer_resolution"] = None
    elif change == "wrong_member":
        object_value(array(shard["decisions"])[0])["sample_ids"] = []
    elif change == "policy":
        object_value(record["adoption_review"])["mode"] = "approved_policy"
    elif change == "unknown_field":
        record["bypass"] = True
    elif change == "bool_format":
        shard["construction_adoption_format"] = True
    elif change == "orphan":
        path.with_name("002.yaml").write_bytes(path.read_bytes())
    else:
        raw = path.read_bytes()
        path.unlink()
        elsewhere = authored.directory / "linked.yaml"
        elsewhere.write_bytes(raw)
        path.symlink_to(elsewhere)
    if change not in {"orphan", "symlink"}:
        path.write_bytes(canonical(shard))
    if change not in {"orphan", "symlink"}:
        decision = object_value(array(shard["decisions"])[0])
        members = sorted(
            [
                [str(object_value(item)["record_key"]), digest(canonical(item))]
                for item in array(shard["records"])
            ]
        )
        membership = digest(canonical([list(pair) for pair in members]))
        decision["members"] = [list(pair) for pair in members]
        decision["membership_hash"] = membership
        decision["id"] = "d:" + membership[7:]
        if change != "wrong_member":
            decision["sample_ids"] = [pair[0] for pair in members]
        shard["default_decision_id"] = decision["id"]
        path.write_bytes(canonical(shard))
        index_path = authored.repository / "authored/construction-adoptions/index.yaml"
        index = object_value(parse(index_path.read_bytes()))
        object_value(index["includes"])[
            path.relative_to(authored.repository / "authored").as_posix()
        ] = digest(path.read_bytes())
        index_path.write_bytes(canonical(index))
        git(authored.repository, "add", "authored")
        git(authored.repository, "commit", "-qm", "Synthetic malformed envelope")
        authored.revision = git(authored.repository, "rev-parse", "HEAD")
    expected = {
        "same_vendor": "two distinct vendors",
        "no_human": "genuine adopted event",
        "wrong_member": "full category batch",
        "policy": "human",
        "unknown_field": "Extra inputs",
        "bool_format": "integer one",
        "orphan": "cover every file",
        "symlink": "Symlink",
    }[change]
    with pytest.raises((ValueError, ValidationError), match=expected):
        load(authored.repository / "authored", authored.repository, authored.revision)
