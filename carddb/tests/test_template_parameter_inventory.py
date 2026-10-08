"""Current whole-source candidate completeness and exact position counterexamples."""

import copy
import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import object_value
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.template_parameter_rules.models import LEGACY_IDS
from sve_carddb.template_parameters import inventory
from sve_carddb.template_parameters.inventory import Candidates, build, summary
from sve_carddb.template_parameters.references import References

from .template_source_fixtures import template_case as template_case  # ruff: ignore[useless-import-alias] -- reusable immutable offline Git/archive fixture

if TYPE_CHECKING:
    from sve_carddb.template_parameters.inventory import Field
    from sve_carddb.template_parameters.spans import Located
    from sve_carddb.template_sources.normalizer import Part

    from .template_source_fixtures import Case


@pytest.mark.parametrize(
    "change", ["missing_entry", "duplicate_entry", "missing_field"]
)
def test_inventory_cannot_shrink_or_duplicate_first_checkpoint_inputs(
    template_case: Case, change: str
) -> None:
    scan = copy.deepcopy(template_case.scan)
    if change == "missing_entry":
        scan.entries.pop(0)
        message = "Parameter candidates must locate every first-checkpoint entry"
    elif change == "duplicate_entry":
        scan.entries.append(scan.entries[0])
        message = "Parameter candidates must locate every first-checkpoint entry"
    else:
        scan.fields.pop(0)
        message = (
            "Parameter span proofs must cover every exact first-checkpoint text field"
        )
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        build(
            FrozenSources(
                template_case.store,
                "test-store",
                template_case.batch,
            ),
            scan,
            References(),
        )


def test_term_diagnostics_do_not_turn_into_bindings_or_false_completion() -> None:
    result = summary(Candidates())
    assert result["term_mentions_are_bindings"] is False


@pytest.mark.parametrize(
    ("enabled", "message"),
    [
        (
            (LEGACY_IDS[0], LEGACY_IDS[0]),
            "Candidate rule selection must be unique",
        ),
        (
            ("suffix_damage_amount", "suffix_damage_amount", LEGACY_IDS[0]),
            "Candidate rule selection must be unique",
        ),
        (
            (LEGACY_IDS[0], "unknown"),
            "Candidate rule selection contains an unknown rule",
        ),
    ],
)
def test_inventory_validates_the_entire_enabled_rule_selection(
    template_case: Case, enabled: tuple[str, ...], message: str
) -> None:
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        build(
            FrozenSources(template_case.store, "test-store", template_case.batch),
            template_case.scan,
            References(),
            enabled_rules=enabled,
        )


def test_inventory_sorts_all_switches_but_counts_only_contextual_matchers(
    template_case: Case,
) -> None:
    enabled = ("suffix_damage_amount", LEGACY_IDS[0], "bracket_choice_index")
    result = build(
        FrozenSources(template_case.store, "test-store", template_case.batch),
        template_case.scan,
        References(),
        enabled_rules=enabled,
    )
    assert result.enabled_rules == tuple(sorted(enabled))
    assert list(object_value(summary(result)["candidate_rule_counts"])) == [
        "bracket_choice_index",
        "suffix_damage_amount",
    ]


@pytest.mark.parametrize("change", ["missing", "extra", "duplicate"])
def test_candidate_output_must_cover_all_and_only_first_checkpoint_entries(
    template_case: Case, monkeypatch: pytest.MonkeyPatch, change: str
) -> None:
    original = inventory._candidate

    def damaged(
        result: Candidates,
        context: Field,
        part: Part,
        position: Located,
        refs: References,
    ) -> None:
        original(result, context, part, position, refs)
        if change == "missing":
            result.entries.pop()
        elif change == "extra":
            result.entries.append(
                result.entries[-1].model_copy(update={"inventory_id": "invented-entry"})
            )
        else:
            result.entries.append(result.entries[-1])

    monkeypatch.setattr(inventory, "_candidate", damaged)
    with pytest.raises(
        ValueError,
        match=r"\AParameter candidates must cover all and only first-checkpoint entries\Z",
    ):
        build(
            FrozenSources(
                template_case.store,
                "test-store",
                template_case.batch,
            ),
            template_case.scan,
            References(),
        )


def test_legacy_normalized_replay_cannot_be_replaced_with_a_different_value(
    template_case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(inventory, "replay", lambda *_, **__: "Synthetic wrong replay")
    with pytest.raises(
        ValueError,
        match=r"\AParameter source recipe must reproduce exact legacy normalized bytes\Z",
    ):
        build(
            FrozenSources(
                template_case.store,
                "test-store",
                template_case.batch,
            ),
            template_case.scan,
            References(),
        )
