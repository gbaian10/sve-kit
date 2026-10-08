"""Current name associations recheck their own immutable physical identity."""

import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.registry.storage import load, relayout, write_files
from sve_carddb.snapshot.values import digest, object_value
from sve_carddb.translations.name_identity import IdentityEvidence

from .adoption_fixtures import commit
from .digital_link_import_fixtures import copied, make_fixture

if TYPE_CHECKING:
    from pathlib import Path

    from .digital_link_import_fixtures import Fixture


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> Fixture:
    return make_fixture(tmp_path_factory.mktemp("current-name-identity"))


def test_current_identity_binds_exact_permanent_face(baseline: Fixture) -> None:
    identity = IdentityEvidence(baseline.sources(), baseline.authored)
    result = identity.association(
        baseline.jp, card_id=baseline.card.id, face_id=baseline.face.id
    )
    assert result == ("ja", "Synthetic card", baseline.card.id, baseline.face.id)
    assert identity.uses[0].usage == "name_identity"
    assert identity.registry() is identity.registry()


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("face", "Name override frozen evidence belongs to another card or face"),
        ("card", "Name override frozen evidence belongs to another card or face"),
        ("locator", "Evidence JSON Pointer is absent"),
    ],
)
def test_current_identity_pin_and_owner_refusals(
    baseline: Fixture, fault: str, message: str
) -> None:
    ref = baseline.jp
    consumer = baseline.authored
    card, face = baseline.card.id, baseline.face.id
    if fault == "face":
        face = "f:" + "0" * 32
    elif fault == "card":
        card = "c:" + "0" * 32
    else:
        ref = ref.model_copy(update={"locator": "/faces/9/name"})
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        IdentityEvidence(baseline.sources(), consumer).association(
            ref, card_id=card, face_id=face
        )


@pytest.mark.parametrize(
    ("guard", "message"),
    [
        (
            "observation",
            "Name override physical observation differs from identity basis",
        ),
        ("identity", "Name override requires confirmed physical identity"),
    ],
)
def test_current_physical_observation_and_identity(
    baseline: Fixture, tmp_path: Path, guard: str, message: str
) -> None:
    fixture = copied(baseline, tmp_path / "repo")
    root = fixture.root / "authored"
    _, entries = load(root)
    for entry in entries.values():
        if guard == "identity" and entry.kind == "card":
            entry.data["identity_state"] = "provisional"
        elif guard == "observation" and entry.kind in {"printing", "art"}:
            object_value(entry.data["observation"])["observation_hash"] = digest(
                b"wrong"
            )
    write_files(relayout(root, list(entries.values())))
    revision = commit(fixture.root)
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        IdentityEvidence(fixture.sources(), revision).association(
            fixture.jp, card_id=fixture.card.id, face_id=fixture.face.id
        )
