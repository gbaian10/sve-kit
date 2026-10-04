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
    result = sources.generate(
        (Batch(store_id="test-store", batch_id=flavor_case.batch),)
    )
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
    sources.generate((Batch(store_id="test-store", batch_id=flavor_case.batch),))
    assert sources.generated_batches == 1


def test_current_flavor_missing_owner_cannot_be_a_definition(flavor_case: Case) -> None:
    result = provider(flavor_case, owners=False).generate(
        (Batch(store_id="test-store", batch_id=flavor_case.batch),)
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
        sources.generate((Batch(store_id="test-store", batch_id=flavor_case.batch),))


def test_flavor_conversion_preserves_exact_final_text_and_draft_accounting(
    flavor_case: Case,
) -> None:
    from sve_carddb.snapshot.values import digest  # ruff: ignore[import-outside-top-level] -- stable private draft fingerprint
    from sve_carddb.template_translations.migration_definitions import derive  # ruff: ignore[import-outside-top-level] -- use actual current flavor sources
    from sve_carddb.template_translations.migration_drafts import FlavorDraft  # ruff: ignore[import-outside-top-level] -- private original target
    from sve_carddb.template_translations.migration_targets import flavors, translation  # ruff: ignore[import-outside-top-level] -- final text precedence

    sources = provider(flavor_case)
    result = sources.generate(
        (Batch(store_id="test-store", batch_id=flavor_case.batch),)
    )
    definitions = derive((), result.entries)
    member = next(m for m in result.entries if m.entry.role == "flavor")
    definition = next(
        r for r in definitions.records if r.data.inventory_id == member.entry.id
    )
    draft = FlavorDraft(
        digest(member.normalized.encode())[7:23],
        digest(member.normalized.encode()),
        member.normalized,
        "舊稿",
        False,
    )
    final = translation(definition.data.id, "定稿\\{原樣\\}", low=True)
    targets = flavors(definitions.records, result.entries, (draft,), (final,))
    assert targets.records == (final,)
    assert not targets.pending
    assert targets.dispositions == (("flavor", draft.identifier, "active"),)
    flagged = flavors(
        definitions.records,
        result.entries,
        (draft,),
        (final.model_copy(update={"low_confidence": False}),),
        quality_flags={definition.data.id: ("proper_name_needs_confirmation",)},
    )
    assert flagged.records[0].data == final.data
    assert flagged.records[0].low_confidence
    assert not flagged.pending
    missing = flavors((), (), (draft,))
    assert not missing.records
    assert missing.pending[0].text == draft.text
    assert missing.pending[0].reasons == ("draft_source_missing",)
