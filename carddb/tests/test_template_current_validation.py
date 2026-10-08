"""Retained current semantic guards are exercised without historical loader fixtures."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import digest
from sve_carddb.domains.translations.templates.definitions import (
    _definitions,
    groups,
    payload,
)
from sve_carddb.domains.translations.templates.loader import validate_templates
from sve_carddb.domains.translations.templates.members import POSITIVE_ROLES
from sve_carddb.domains.translations.templates.records import DefinitionRecord

from .test_template_current import make_case

if TYPE_CHECKING:
    from sve_carddb.domains.translations.templates.definitions import Groups
    from sve_carddb.domains.translations.templates.members import Reconstructed

    from .test_template_current import Case


@pytest.fixture(scope="module")
def current_case(tmp_path_factory: pytest.TempPathFactory) -> Case:
    return make_case(tmp_path_factory.mktemp("current-validation"))


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        (
            "normalized_hash",
            "sha256:" + "0" * 64,
            "Template definition pattern has no current source position",
        ),
        (
            "role",
            "reminder",
            "Template definition pattern has no current source position",
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
            replace(current_case.inputs, records=(changed,)),
            current_case.sources,
            current_case.batches,
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
            current_case.batches,
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
            current_case.batches,
        )


def _with_other(
    current_case: Case,
    *,
    roles: tuple[str, ...] | None = None,
    pending: tuple[str, ...] = (),
) -> tuple[DefinitionRecord, Reconstructed, Groups]:
    record = current_case.inputs.records[0]
    assert isinstance(record, DefinitionRecord)
    members = {m.entry.id: m for m in current_case.generated.entries}
    member = next(
        m
        for m in members.values()
        if m.candidate.template_normalized_hash == record.data.normalized_hash
    )
    other = replace(
        member,
        entry=member.entry.model_copy(update={"id": "inv:other"}),
        roles=member.roles if roles is None else roles,
        pending=pending,
    )
    return record, member, groups({**members, "inv:other": other})


def test_equal_text_with_another_schema_stays_unmatched(current_case: Case) -> None:
    record, member, patterns = _with_other(current_case, pending=("unresolved",))
    _, matches, frequencies = _definitions((record,), patterns)
    assert matches == {record.data.id: (member.entry.id,)}
    assert frequencies == ((record.data.id, 1),)


def test_equal_schema_with_different_slot_roles_is_refused(current_case: Case) -> None:
    member = current_case.generated.entries[0]
    # Keep the numeric bound class so only the semantic role differs.
    role = (
        next(r for r in POSITIVE_ROLES if r != member.roles[0])
        if member.roles[0] in POSITIVE_ROLES
        else "other_role"
    )
    record, _, patterns = _with_other(current_case, roles=(role,) * len(member.roles))
    with pytest.raises(
        ValueError,
        match=r"^Template definition positions disagree on slot semantic roles$",
    ):
        _definitions((record,), patterns)
