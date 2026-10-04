"""Current whole-source candidate completeness and exact position counterexamples."""

import copy
import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.frozen_sources import FrozenSources
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
                template_case.scan.entries[0].source_ref.store_id,
                template_case.batch,
            ),
            scan,
            References(),
        )


def test_term_diagnostics_do_not_turn_into_bindings_or_false_completion() -> None:
    result = summary(Candidates())
    assert result["term_mentions_are_bindings"] is False


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
                template_case.scan.entries[0].source_ref.store_id,
                template_case.batch,
            ),
            template_case.scan,
            References(),
        )


def test_legacy_normalized_replay_cannot_be_replaced_with_a_different_value(
    template_case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(inventory, "replay", lambda *_: "Synthetic wrong replay")
    with pytest.raises(
        ValueError,
        match=r"\AParameter source recipe must reproduce exact legacy normalized bytes\Z",
    ):
        build(
            FrozenSources(
                template_case.store,
                template_case.scan.entries[0].source_ref.store_id,
                template_case.batch,
            ),
            template_case.scan,
            References(),
        )
