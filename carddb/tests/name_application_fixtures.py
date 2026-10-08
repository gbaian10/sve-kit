"""One immutable synthetic policy/replay and real name-capability DB per module."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.core.json import object_value, parse
from sve_carddb.core.provenance import BuildContext
from sve_carddb.digital_name_policies.application import Inputs
from sve_carddb.registry.preview import FrozenJP, plan_preview
from sve_carddb.text_observations import FrozenTexts, plan_text_observations
from sve_carddb.text_observations.models import candidate_revision_id
from sve_carddb.translations.current_names import Names
from sve_carddb.translations.importer import Inputs as TranslationInputs
from sve_carddb.translations.name_sources import NameOwner
from sve_carddb.translations.sources import Sources

from .adoption_fixtures import commit
from .build_db_fixtures import rows
from .database_fixtures import DatabaseTemplate
from .digital_name_policy_fixtures import PolicyFixture, make_policy_fixture
from .translation_fixtures import template, write

RUNTIME: tuple[str, ...] = ()


if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.text_observations import TextPlan


@dataclass(frozen=True)
class ApplicationCase:
    fixture: PolicyFixture
    inputs: Inputs
    context: BuildContext
    texts: TextPlan
    replay: Names
    database: DatabaseTemplate

    def sources(self) -> Sources:
        """Every expected/producer run has its own checked source-use collector."""
        return Sources(
            {"test-store": self.fixture.digital.store}, self.fixture.root, self.context
        )


def application_case(
    root: Path, *, translated_name: str = "合成測試名"
) -> ApplicationCase:
    """Actual publication candidates use frozen HTML, rather than a caller-forged ID."""
    fixture = make_policy_fixture(root, translated_name=translated_name)
    write(root / "authored", {})
    revision = commit(root)
    inputs = Inputs(root / "authored", root, revision, "2026-10-03T00:00:00Z")
    config = (
        object_value(parse(fixture.digital.build.configuration.encode()))
        | inputs.configuration()
    )
    object_value(config["catalog_registry"])["authored_revision"] = revision
    context = BuildContext.from_inputs(revision, config)
    owner = fixture.owner()
    assert owner.name_ref is not None
    batch = owner.name_ref.batch_id
    identity = plan_preview(
        root / "authored",
        FrozenJP(
            fixture.digital.store,
            "test-store",
            batch,
            parser_version="translation-jp-v1",
        ),
        regions=("jp",),
    )
    texts = plan_text_observations(
        identity,
        FrozenTexts(
            fixture.digital.store,
            "test-store",
            batch,
            region="jp",
            parser_version="translation-jp-v1",
        ),
    )
    translation = TranslationInputs(root / "authored", root, revision)
    replay = Names(translation.load(), ())
    schema = compile_build(("t0", "translation_names"))
    with create_database(schema) as db, template().copy() as original:
        with db.transaction():
            for table in schema.tables:
                if original.has_table(table.name):
                    for row in original.rows(table.name):
                        db.insert(table.name, dict(row.values))
        fixture.digital.publish(db)
        with db.transaction():
            db.update(
                "face_revision",
                {"id": "link-revision"},
                {"id": candidate_revision_id(texts.candidates()[0])},
            )
        database = DatabaseTemplate(schema, db._connection.serialize())
    return ApplicationCase(fixture, inputs, context, texts, replay, database)


def printed_owner(db: Database, case: ApplicationCase) -> NameOwner:
    """Use an actual included printing with its own frozen name."""

    printing = case.fixture.digital.printing
    revision = db.rows("face_revision")[0].values
    db.insert(
        "printing",
        rows()["printing"]
        | {
            "id": printing.id,
            "card_id": printing.card_id,
            "card_no": printing.card_no,
            "home_set_id": printing.home_set_id,
            "source_id": revision["source_id"],
        },
    )
    db.insert(
        "printing_face",
        rows()["printing_face"]
        | {
            "printing_id": printing.id,
            "face_id": case.fixture.digital.face.id,
            "card_id": printing.card_id,
            "printed_text_state": "verified",
            "printed_name_unit_id": revision["name_unit_id"],
            "source_id": revision["source_id"],
        },
    )

    return NameOwner("printing_face", printing.id, case.fixture.digital.face.id)
