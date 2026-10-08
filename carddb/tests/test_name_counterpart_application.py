"""Authored same-card links grant names only to the exact owner's own face."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import array, canonical, object_value, parse
from sve_carddb.core.provenance import BuildContext
from sve_carddb.domains.digital.links.importer import Inputs, populate_links
from sve_carddb.domains.digital.name_policies.application import _counterparts
from sve_carddb.domains.translations.digital import configuration
from sve_carddb.domains.translations.names.sources import NameOwner

from .adoption_fixtures import commit
from .digital_link_fixtures import envelope, write
from .name_application_fixtures import ApplicationCase, application_case, printed_owner
from .test_name_current_application import current_case

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> ApplicationCase:
    return application_case(tmp_path_factory.mktemp("counterpart-application"))


def links_case(case: ApplicationCase, root: Path) -> ApplicationCase:
    changed = current_case(case, root)
    normal = object_value(parse(case.fixture.digital.record))
    evolved = object_value(parse(canonical(normal)))
    object_value(evolved["subject"])["digital_phase"] = "evolved"
    for name in array(object_value(evolved["value"])["digital_names"]):
        object_value(name)["phase"] = "evolved"
    shard = envelope([normal, evolved])
    write(root / "authored", {"digital-links/links/synthetic/001.yaml": shard})
    revision = commit(root)
    inputs = Inputs(root / "authored", root, revision)
    config = (
        object_value(parse(changed.context.configuration.encode()))
        | inputs.configuration()
        | configuration(case.fixture.digital.refs, (("svwb", "22345678"),))
    )
    context = BuildContext.from_inputs(
        changed.context.program_revision,
        config,
    )
    return replace(changed, context=context)


def test_counterpart_uses_each_linked_phase_and_rechecks_printing(
    baseline: ApplicationCase, tmp_path: Path
) -> None:
    case = links_case(baseline, tmp_path / "links")
    inputs = Inputs(
        case.inputs.root, case.inputs.repository, commit(case.inputs.repository)
    )
    # Commit is unchanged; configured authored pin includes the closed link entry.
    with case.database.copy() as db, db.transaction():
        result = populate_links(
            db,
            inputs,
            build=case.context,
            stores={"test-store": case.fixture.digital.store},
        )
        owner = NameOwner(
            "face_revision", str(db.rows("face_revision")[0].values["id"])
        )
        candidates = _counterparts(db, case.sources(), owner, result)
        assert len(candidates) == 2
        assert {c.origin for c in candidates} == {"official_svwb"}
        assert len({c.decision_id for c in candidates}) == 2
        printing = printed_owner(db, case)
        proof = case.fixture.owner().name_ref
        assert proof is not None
        assert (
            len(_counterparts(db, case.sources(), printing, result, name_ref=proof))
            == 2
        )
        db.update(
            "printing_face",
            {"printing_id": printing.identifier, "face_id": printing.face_id},
            {"printed_text_state": "unknown", "printed_name_unit_id": None},
        )
        assert _counterparts(db, case.sources(), printing, result, name_ref=proof) == ()
        with pytest.raises(
            ValueError,
            match=r"^Digital-link owner reference differs from its own name$",
        ):
            result.eligible_owner(
                db,
                case.sources(),
                owner,
                name_ref=proof.model_copy(update={"parser": "translation-en-v1"}),
            )
        card = case.fixture.digital.card.model_dump(mode="json")
        db.insert("card", card | {"id": "third-card"})
        db.insert(
            "face",
            case.fixture.digital.face.model_dump(mode="json")
            | {"id": "third-face", "card_id": "third-card"},
        )
        own = db.rows("face_revision")[0].values
        db.insert(
            "face_revision",
            dict(own) | {"id": "third-revision", "face_id": "third-face"},
        )
        assert (
            _counterparts(
                db, case.sources(), NameOwner("face_revision", "third-revision"), result
            )
            == ()
        )
