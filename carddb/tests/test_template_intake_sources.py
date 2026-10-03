"""A role is usable only after exact frozen-source and recognition pin validation."""

import copy
import json
import re
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import canonical, digest, object_value
from sve_carddb.template_parameters.models import Schema, Slot
from sve_carddb.template_sources.normalizer import partition
from sve_carddb.template_translations.models import Inventory
from sve_carddb.template_translations.sources import TemplateSources

from .template_intake_fixtures import intake_case, policy_git, recognition_term_source
from .test_template_parameters import candidate

if TYPE_CHECKING:
    from pydantic import JsonValue
    from pytest_mock import MockerFixture

    from .template_intake_fixtures import Case

__all__ = ("intake_case", "policy_git", "recognition_term_source")


def exact(message: str) -> str:
    """Do not accept another earlier guard as evidence for this condition."""
    return "^" + re.escape(message) + "$"


@pytest.mark.parametrize(
    ("change", "message"),
    [
        (
            "missing_recipe",
            "Formal template inventory requires its three complete recipe pins",
        ),
        (
            "duplicate_recipe",
            "Formal template inventory requires its three complete recipe pins",
        ),
        (
            "wrong_source_pin",
            "Formal template source recipes differ from the parameter recipe",
        ),
        (
            "wrong_program",
            "Recognition parameter recipe ID, program or config hash is invalid",
        ),
        (
            "bad_config_hash",
            "Recognition parameter recipe ID, program or config hash is invalid",
        ),
        (
            "extra_config",
            "Formal template recipe requires its exact legacy input and closed config",
        ),
        (
            "wrong_legacy_hash",
            "Formal template recipe requires its exact legacy input and closed config",
        ),
        ("wrong_batch", "Formal template batch differs from its recognition scope"),
        (
            "partial_policy_pin",
            "Recognition policy requires the complete five-field pin",
        ),
        ("policy_hash", "Recognition policy or approval receipt pin hash mismatch"),
    ],
)
def test_recipe_and_approval_pin_guards(  # ruff: ignore[complex-structure] -- each config mutation isolates a distinct pin refusal
    intake_case: Case, change: str, message: str
) -> None:
    pins = list(intake_case.pins)
    if change == "missing_recipe":
        pins.pop()
    elif change == "duplicate_recipe":
        pins.append(pins[-1])
    elif change == "wrong_source_pin":
        pins[0] = pins[0].model_copy(update={"code_hash": "sha256:" + "f" * 64})
    else:
        index = next(
            i for i, r in enumerate(pins) if r.id.startswith("template-parameters-")
        )
        recipe = pins[index]
        config = copy.deepcopy(recipe.config)
        updates: dict[str, object] = {}
        if change == "wrong_program":
            updates["code_path"] = (
                "carddb/src/sve_carddb/template_sources/normalizer.py"
            )
        elif change == "bad_config_hash":
            updates["config_hash"] = "sha256:" + "a" * 64
        elif change == "extra_config":
            config["status"] = "approved"
        elif change == "wrong_legacy_hash":
            config["legacy_file_hash"] = "sha256:" + "a" * 64
        elif change == "wrong_batch":
            object_value(config["source_batch"])["batch_id"] = "sha256:" + "a" * 64
        elif change == "partial_policy_pin":
            object_value(config["recognition_policy"]).pop("approval_receipt_hash")
        else:
            object_value(config["recognition_policy"])["hash"] = "sha256:" + "a" * 64
        if config != recipe.config:
            updates.update(config=config, config_hash=digest(canonical(config)))
        pins[index] = recipe.model_copy(update=updates)
    with pytest.raises(ValueError, match=exact(message)):
        intake_case.sources().reconstruct(tuple(pins))


def test_null_recognition_does_not_adopt_old_numeric_rules(intake_case: Case) -> None:
    pins = list(intake_case.pins)
    index = next(
        i for i, r in enumerate(pins) if r.id.startswith("template-parameters-")
    )
    recipe = pins[index]
    config = copy.deepcopy(recipe.config)
    config["recognition_policy"] = None
    pins[index] = recipe.model_copy(
        update={"config": config, "config_hash": digest(canonical(config))}
    )
    replay = intake_case.sources().reconstruct(tuple(pins))
    assert all(m.pending for m in replay.entries if m.entry.role == "body")


def test_reconstructed_objects_are_detached_from_the_cache(intake_case: Case) -> None:
    sources = intake_case.sources()
    before = sources.reconstruct(intake_case.pins)
    term = next(h for m in before.entries for h in m.hints if h.target is not None)
    assert term.target is not None
    term.target["id"] = "term:fake.changed"
    after = sources.reconstruct(intake_case.pins)
    assert not any(
        h.target and h.target.get("id") == "term:fake.changed"
        for m in after.entries
        for h in m.hints
    )


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("missing", "Template schema must cover each source position exactly once"),
        (
            "wrong_position",
            "Template schema must cover each source position exactly once",
        ),
        ("bounds", "Template numeric bounds differ from the recognized role"),
        ("reference_kind", "Parameter schema requires matching resolved slot types"),
    ],
)
def test_schema_positions_types_and_bounds_are_independent(
    intake_case: Case, change: str, message: str
) -> None:
    member = next(
        m
        for m in intake_case.replay.entries
        if m.entry.role == "body" and not m.pending
    )
    hint = member.hints[0]
    slot = Slot(
        name="amount",
        type="uint",
        occurrences=(hint.occurrence,),
        reference_kind=None,
        min=0,
        max=9007199254740991,
    )
    if change == "missing":
        schema = Schema(slots=())
    elif change == "wrong_position":
        occurrence = hint.occurrence.model_copy(update={"start": 0, "end": 1})
        schema = Schema(slots=(slot.model_copy(update={"occurrences": (occurrence,)}),))
    elif change == "bounds":
        schema = Schema(slots=(slot.model_copy(update={"max": 100}),))
    else:
        schema = Schema(
            slots=(
                slot.model_copy(
                    update={
                        "type": "reference",
                        "reference_kind": "card",
                        "min": None,
                        "max": None,
                    }
                ),
            )
        )
    with pytest.raises(ValueError, match=exact(message)):
        member.verify_schema(schema)


def test_ordinal_minimum_and_merged_role_protection(intake_case: Case) -> None:
    member = next(
        m
        for m in intake_case.replay.entries
        if m.entry.role == "body" and not m.pending
    )
    hint = member.hints[0]
    slot = Slot(
        name="ordinal",
        type="uint",
        occurrences=(hint.occurrence,),
        reference_kind=None,
        min=1,
        max=9007199254740991,
    )
    ordinal = replace(member, roles=("turn_ordinal",))
    ordinal.verify_schema(Schema(slots=(slot,)))
    with pytest.raises(
        ValueError,
        match=exact("Template numeric bounds differ from the recognized role"),
    ):
        ordinal.verify_schema(Schema(slots=(slot.model_copy(update={"min": 0}),)))


@pytest.mark.parametrize("version", [True, "1", 1.0])
def test_inventory_format_is_not_coerced(intake_case: Case, version: JsonValue) -> None:
    data = copy.deepcopy(intake_case.files["translations/template-sources/001.yaml"])
    object_value(data)["template_source_format"] = version
    with pytest.raises(
        ValueError, match="Template inventory format must be integer one"
    ):
        Inventory.model_validate_json(json.dumps(data))


@pytest.mark.parametrize(
    ("text", "roles", "message"),
    [
        ("２枚２枚", ("numeric", "numeric"), None),
        (
            "２枚３枚",
            ("numeric", "numeric"),
            "Repeated parameter occurrences must have identical values",
        ),
        (
            "２枚２枚",
            ("damage_amount", "numeric"),
            "Repeated template slot cannot combine distinct semantic roles",
        ),
    ],
)
def test_repeated_slots_require_equal_values_and_roles(
    intake_case: Case, text: str, roles: tuple[str, ...], message: str | None
) -> None:
    # This unit exercises downstream schema merging, not frozen-source adoption.
    baseline = next(m for m in intake_case.replay.entries if not m.pending)
    analyzed = candidate(text)
    hints = tuple(h.model_copy(update={"issues": ()}) for h in analyzed.slots)
    member = replace(
        baseline,
        field_text=text,
        normalized=partition(text)[0].normalized,
        hints=hints,
        roles=roles,
        pending=(),
    )
    schema = Schema(
        slots=(
            Slot(
                name="amount",
                type="uint",
                occurrences=tuple(h.occurrence for h in hints),
                reference_kind=None,
                min=0,
                max=9007199254740991,
            ),
        )
    )
    if message is None:
        member.verify_schema(schema)
    else:
        with pytest.raises(ValueError, match=exact(message)):
            member.verify_schema(schema)


def test_first_jp_batch_also_pins_counts(
    intake_case: Case, mocker: MockerFixture
) -> None:
    mocker.patch(
        "sve_carddb.template_translations.sources.JP_BATCH",
        object_value(intake_case.source.recipe.config["source_batch"])["batch_id"],
    )
    sources = TemplateSources(
        PinnedRepository(intake_case.repository),
        {"test-store": intake_case.source.store},
        main_revision=intake_case.source.main,
        legacy_bytes=intake_case.source.legacy,
        proposals=intake_case.source.proposals,
    )
    with pytest.raises(
        ValueError,
        match=exact(
            "Formal template first JP batch differs from its fixed baseline counts"
        ),
    ):
        sources.reconstruct(intake_case.pins)
