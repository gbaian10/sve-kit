"""Current flavor builds keep exact physical owners without historical translation pins."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_models import Batch
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.snapshot.values import array, canonical, object_value, parse
from sve_carddb.template_parameter_rules.current import parse as parse_rules
from sve_carddb.template_parameters.references import References
from sve_carddb.template_translations.current_owners import Owners
from sve_carddb.template_translations.current_sources import Sources

from .template_flavor_fixtures import flavor_case

if TYPE_CHECKING:
    from pytest_mock import MockerFixture

    from .template_flavor_fixtures import Case

__all__ = ("flavor_case",)


def provider(case: Case, *, owners: bool = True) -> Sources:
    stores = {"test-store": case.store}
    registry = load_registry(case.repository / "authored")
    rules = parse_rules(
        canonical(
            {
                "parameter_rule_format": 2,
                "kind": "template_parameter_rules",
                "rules": [],
            }
        )
    )
    return Sources(
        stores, References(), rules, owners=Owners(registry, stores) if owners else None
    )


def test_current_flavor_whole_field_exact_owner_and_separate_states(
    flavor_case: Case,
) -> None:
    sources = provider(flavor_case)
    result = sources.generate((Batch(batch_id=flavor_case.batch),))
    members = tuple(
        member for member in result.entries if member.entry.role == "flavor"
    )
    assert len(members) == 3
    assert all(member.owner is not None and not member.pending for member in members)
    assert members[0].owner != members[1].owner
    assert all(
        member.normalized == member.field_text and not member.hints
        for member in members
    )
    exact = {member.normalized for member in members}
    assert "甲２\n（乙）\n\n丙🌱" in exact
    reports = array(parse(result.report))
    states = object_value(object_value(reports[0])["flavor_states"])
    assert states == {"present": 3, "unknown": 1, "empty": 1}
    sources.generate((Batch(batch_id=flavor_case.batch),))
    assert sources.generated_batches == 1


def test_current_flavor_missing_owner_cannot_be_a_definition(flavor_case: Case) -> None:
    result = provider(flavor_case, owners=False).generate(
        (Batch(batch_id=flavor_case.batch),)
    )
    members = tuple(
        member for member in result.entries if member.entry.role == "flavor"
    )
    assert len(members) == 3
    assert all(
        member.owner is None and member.pending == ("missing_flavor_identity_owner",)
        for member in members
    )


def test_current_flavor_still_checks_exact_own_physical_text(
    flavor_case: Case, mocker: MockerFixture
) -> None:
    sources = provider(flavor_case)
    mocker.patch(
        "sve_carddb.template_translations.current_owners.digest",
        return_value="sha256:" + "0" * 64,
    )
    with pytest.raises(
        ValueError, match=r"^Flavor owner source differs from its exact physical field$"
    ):
        sources.generate((Batch(batch_id=flavor_case.batch),))
