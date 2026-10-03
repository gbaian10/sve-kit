"""Real link replay grants names only to the exact owner and actual human samples."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_inputs import BuildContext
from sve_carddb.digital_links.importer import Inputs, populate_links
from sve_carddb.digital_name_policies.application import _counterparts, prepare
from sve_carddb.snapshot.values import array, canonical, object_value, parse
from sve_carddb.translations.digital import configuration
from sve_carddb.translations.name_build import NameOwner
from sve_carddb.translations.name_selection import select_owner_name

from .adoption_fixtures import commit
from .digital_link_fixtures import envelope, write
from .name_application_fixtures import (
    ApplicationCase,
    application_case,
    printed_owner,
    staged,
)

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> ApplicationCase:
    return application_case(tmp_path_factory.mktemp("counterpart-application"))


def links_case(case: ApplicationCase, root: Path, *, sample: str) -> ApplicationCase:
    changed = staged(case, root, {})
    normal = object_value(parse(case.fixture.digital.record))
    evolved = object_value(parse(canonical(normal)))
    subject = object_value(object_value(evolved["data"])["subject"])
    subject["digital_phase"] = "evolved"
    evolved["record_key"] = canonical(["digital_link_adoption", subject, 1]).decode()
    for name in array(
        object_value(object_value(evolved["data"])["value"])["digital_names"]
    ):
        object_value(name)["phase"] = "evolved"
    shard = envelope([normal, evolved])
    shard["review_context"] = object_value(parse(case.fixture.digital.shard))[
        "review_context"
    ]
    object_value(array(shard["decisions"])[0])["sample_ids"] = [
        normal["record_key"] if sample == "normal" else evolved["record_key"]
    ]
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
        {p.name: (root / p.name).read_bytes() for p in changed.context.dependencies},
        config,
    )
    return replace(changed, context=context)


@pytest.mark.parametrize("sample", ["normal", "evolved"])
def test_counterpart_priority_requires_actual_sample_and_rechecks_printing(
    baseline: ApplicationCase, tmp_path: Path, sample: str
) -> None:
    case = links_case(baseline, tmp_path / "links", sample=sample)
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
        assert sum(c.counterpart_checked for c in candidates) == 1
        assert {c.origin for c in candidates} == {"official_svwb"}
        samples = next(
            r.values
            for r in db.rows("decision")
            if r.values["id"] == result.decisions[0][1]
        )
        from sve_carddb.build_db import Json  # ruff: ignore[import-outside-top-level] -- corrupt only the private audit to test its independent immutable membership guard

        db.update(
            "decision",
            {"id": samples["id"]},
            {"sample_ids": Json([key for key, _ in result.decisions])},
        )
        assert (
            sum(
                c.counterpart_checked
                for c in _counterparts(db, case.sources(), owner, result)
            )
            == 1
        )
        db.update(
            "decision", {"id": samples["id"]}, {"sample_ids": samples["sample_ids"]}
        )
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
        policy = prepare(
            db, case.inputs, case.texts, sources=case.sources(), replay=case.replay
        ).owners[0]
        invalid = replace(policy.result, status="untranslated", game=None, text=None)
        selected = select_owner_name(
            policy.source,
            invalid,
            context_hash=invalid.context_hash,
            choices=(),
            counterparts=candidates,
            policy_candidate=None,
        )
        assert selected.reason == "counterpart"
        assert selected.candidate is not None
        assert selected.candidate.counterpart_checked
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
