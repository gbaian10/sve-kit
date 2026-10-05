"""Retained current semantic guards are exercised without historical loader fixtures."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.snapshot.values import digest
from sve_carddb.template_parameter_rules.current import resolve
from sve_carddb.template_parameters.inventory import Candidates
from sve_carddb.template_translations.current import validate_templates
from sve_carddb.template_translations.current_models import DefinitionRecord
from sve_carddb.template_translations.definitions import _definitions, payload

from .test_template_current import make_case

if TYPE_CHECKING:
    from pydantic import JsonValue

    from .test_template_current import Case


@pytest.fixture(scope="module")
def current_case(tmp_path_factory: pytest.TempPathFactory) -> Case:
    return make_case(tmp_path_factory.mktemp("current-validation"))


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        (
            "inventory_id",
            "absent",
            "Template definition references an absent inventory entry",
        ),
        (
            "source_lang",
            "en",
            "Template definition language normalizer or semantic variant is unsupported",
        ),
        (
            "normalizer_version",
            "unknown",
            "Template definition language normalizer or semantic variant is unsupported",
        ),
        (
            "semantic_variant",
            "unknown",
            "Template definition language normalizer or semantic variant is unsupported",
        ),
        (
            "content_hash",
            "sha256:" + "0" * 64,
            "Template definition content hash differs from its six-field payload",
        ),
        (
            "id",
            "T" + "0" * 16,
            "Template ID differs from its allocated payload hash",
        ),
    ],
)
def test_current_definition_rejects_changed_semantic_identity(
    current_case: Case, field: str, value: str, message: str
) -> None:
    record = current_case.inputs.records[0]
    assert isinstance(record, DefinitionRecord)
    changed = record.model_copy(
        update={"data": record.data.model_copy(update={field: value})}
    )
    with pytest.raises(ValueError, match="^" + message + "$"):
        validate_templates(
            replace(current_case.inputs, records=(changed,)), current_case.sources
        )


def test_current_payload_has_one_id_and_each_source_has_one_definition(
    current_case: Case,
) -> None:
    record = current_case.inputs.records[0]
    assert isinstance(record, DefinitionRecord)
    member = current_case.generated.entries[0]
    alternate_id = "T" + record.data.content_hash[7:25]
    alternate = record.model_copy(
        update={"data": record.data.model_copy(update={"id": alternate_id})}
    )
    with pytest.raises(
        ValueError, match=r"^Template payload hash must have exactly one allocated ID$"
    ):
        validate_templates(
            replace(current_case.inputs, records=(record, alternate)),
            current_case.sources,
        )
    slot = record.data.parameter_schema.slots[0].model_copy(update={"name": "amount"})
    alternate = record.model_copy(
        update={
            "data": record.data.model_copy(
                update={
                    "parameter_schema": record.data.parameter_schema.model_copy(
                        update={"slots": (slot,)}
                    ),
                }
            )
        }
    )
    checksum = digest(payload(member, alternate))
    alternate = alternate.model_copy(
        update={
            "data": alternate.data.model_copy(
                update={
                    "content_hash": checksum,
                    "id": "T" + checksum[7:23],
                }
            )
        }
    )
    with pytest.raises(
        ValueError, match=r"^Template source member has multiple current definitions$"
    ):
        validate_templates(
            replace(current_case.inputs, records=(record, alternate)),
            current_case.sources,
        )


@pytest.mark.parametrize("change", ["roles", "pending"])
def test_equal_text_with_another_schema_or_role_stays_unmatched(
    current_case: Case, change: str
) -> None:
    record = current_case.inputs.records[0]
    assert isinstance(record, DefinitionRecord)
    members = {m.entry.id: m for m in current_case.generated.entries}
    member = members[record.data.inventory_id]
    other = replace(
        member,
        entry=member.entry.model_copy(update={"id": "inv:other"}),
        roles=("other_role",) * len(member.roles)
        if change == "roles"
        else member.roles,
        pending=("unresolved",) if change == "pending" else member.pending,
    )
    _, matches, frequencies = _definitions((record,), {**members, "inv:other": other})
    assert matches == {record.data.id: (member.entry.id,)}
    assert frequencies == ((record.data.id, 1),)


@pytest.mark.parametrize("duplicate", [False, True])
def test_current_matcher_cannot_duplicate_or_reclaim_numeric_ownership(
    current_case: Case, *, duplicate: bool
) -> None:
    candidate = current_case.generated.entries[0].candidate
    # The ordinary unit rule already owns this source position.
    row: dict[str, JsonValue] = {
        "inventory_id": candidate.inventory_id,
        "slot": candidate.slots[0].name,
        "rule_id": "suffix_recovery_amount",
    }
    candidates = Candidates(
        entries=[candidate], rule_matches=[row, row] if duplicate else [row]
    )
    message = (
        "Recognition matches must have unique slot ownership"
        if duplicate
        else "Recognition new rules cannot claim old numeric ownership"
    )
    with pytest.raises(ValueError, match="^" + message + "$"):
        resolve(current_case.sources.rules, candidates)


def test_current_matcher_requires_the_exact_unresolved_reason(
    current_case: Case,
) -> None:
    candidate = current_case.generated.entries[0].candidate
    hint = candidate.slots[0].model_copy(update={"issues": ()})
    changed = candidate.model_copy(update={"slots": (hint,)})
    with pytest.raises(
        ValueError, match=r"^Recognition matched slot lacks its exact pending reason$"
    ):
        resolve(current_case.sources.rules, Candidates(entries=[changed]))
