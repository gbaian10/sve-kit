"""Source-derived synthetic ports stay pending while diagnostics explain each gap."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import PlayerRef, Port, ReceiptRef
from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_port_rejections import unsupported_port_reason

from ..translations import test_four_layer_749 as source_cases
from .author_entry_cases import source_use

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.domains.effects.author_port_rejections import Direction
    from sve_carddb.domains.translations.four_layer_classification import Classifier

engine = source_cases.engine


@pytest.mark.parametrize(
    ("case", "reasons"),
    [
        ("P01", {"pool"}),
        ("P02", {"pool", "object"}),
        ("P03", {"pool", "player"}),
        ("P04", {"receipt", "player"}),
        ("H01", {"object"}),
        ("D01", {"player"}),
        ("D02", {"capture", "event", "player"}),
        ("X01", {"capture", "payment"}),
        ("X02", {"choice"}),
        ("R02", {"object", "event"}),
        ("A01", {"ability"}),
    ],
)
def test_derived_interfaces_never_become_producer_proofs(
    case: str, reasons: set[str], engine: Classifier
) -> None:
    _, raw, ordinal = next(item for item in source_cases.CASES if item[0] == case)
    found = source_cases.derive(raw, engine, case)
    frame, binding = found.bind(ordinal)
    found.verify_binding(raw, engine, frame, binding)
    assert frame.semantic_variant.state == "pending"
    interfaces: tuple[tuple[Direction, tuple[Port, ...]], ...] = (
        ("imports", frame.projection.imports),
        ("exports", frame.projection.exports),
    )
    actual = {
        unsupported_port_reason(port, direction=direction)
        for direction, ports in interfaces
        for port in ports
    }
    assert actual == {f"unsupported_{reason}_producer_mapping" for reason in reasons}
    assert frame.semantic_variant.state == "pending"
    for direction, ports in interfaces:
        for item in ports:
            assert item.evidence is not None
            evidence = item.evidence
            wrong_value = (
                ReceiptRef(
                    kind="ReceiptId", receipt_id="fixture:receipt", result="objects"
                )
                if item.type == "PlayerRef"
                else PlayerRef(kind="PlayerRef", player="self")
            )
            for changes, reason in (
                ({"evidence": None}, "evidence"),
                ({"source_role": "future_role"}, "role"),
                (
                    {
                        "evidence": evidence.model_copy(
                            update={
                                "capture": "live"
                                if evidence.capture == "declaration"
                                else "declaration"
                            }
                        )
                    },
                    "signature",
                ),
                (
                    {"evidence": evidence.model_copy(update={"value": wrong_value})},
                    "signature",
                ),
            ):
                assert (
                    unsupported_port_reason(
                        item.model_copy(update=changes), direction=direction
                    )
                    == f"unsupported_port_{reason}"
                )


def port(role: str, capture: str, value: dict[str, JsonValue]) -> Port:
    return Port.model_validate_json(
        canonical(
            {
                "name": "fixture_port",
                "type": value["kind"],
                "source_role": role,
                "scope": "ability_0",
                "evidence": {
                    "source": source_use()["occurrence"],
                    "capture": capture,
                    "value": value,
                },
            }
        )
    )


def receipt(*, result: str = "objects", role: str = "read_receipt") -> Port:
    return port(
        role,
        "receipt",
        {"kind": "ReceiptId", "receipt_id": "fixture:receipt", "result": result},
    )


def test_evidence_precedes_unknown_role() -> None:
    value = receipt().model_copy(
        update={"evidence": None, "source_role": "future_role"}
    )
    assert (
        unsupported_port_reason(value, direction="imports")
        == "unsupported_port_evidence"
    )


@pytest.mark.parametrize("direction", ["imports", "exports"])
def test_future_roles_are_closed(direction: Direction) -> None:
    value = receipt().model_copy(update={"source_role": "future_role"})
    assert (
        unsupported_port_reason(value, direction=direction) == "unsupported_port_role"
    )


def test_same_role_on_another_type_or_direction_does_not_borrow_signature() -> None:
    value = receipt()
    assert (
        unsupported_port_reason(value, direction="exports") == "unsupported_port_role"
    )
    assert (
        unsupported_port_reason(
            value.model_copy(update={"type": "ObjectSet"}), direction="imports"
        )
        == "unsupported_port_role"
    )


def test_value_variant_and_capture_are_both_required() -> None:
    value = receipt()
    assert value.evidence is not None
    for evidence in (
        value.evidence.model_copy(update={"capture": "live"}),
        value.evidence.model_copy(
            update={"value": PlayerRef(kind="PlayerRef", player="self")}
        ),
    ):
        assert (
            unsupported_port_reason(
                value.model_copy(update={"evidence": evidence}), direction="imports"
            )
            == "unsupported_port_signature"
        )


def test_payment_completion_cannot_stand_for_object_or_amount_receipt() -> None:
    for result in ("objects", "amount"):
        assert (
            unsupported_port_reason(receipt(result=result), direction="imports")
            == "unsupported_receipt_producer_mapping"
        )
    assert (
        unsupported_port_reason(receipt(result="payment_complete"), direction="imports")
        == "unsupported_port_signature"
    )
    assert (
        unsupported_port_reason(receipt(role="payment_complete"), direction="imports")
        == "unsupported_port_signature"
    )


@pytest.mark.parametrize(
    ("result_type", "measure"), [("Bool", "die_face"), ("Nat", "declared_x")]
)
def test_captured_value_keeps_result_type_and_measure(
    result_type: str, measure: str
) -> None:
    value = port(
        "dice_result",
        "receipt",
        {
            "kind": "CapturedValue",
            "binding_id": "fixture:value",
            "result_type": result_type,
            "measure": measure,
        },
    )
    assert (
        unsupported_port_reason(value, direction="imports")
        == "unsupported_port_signature"
    )


def test_holder_and_event_subject_do_not_share_capture_rules() -> None:
    value = port(
        "entry_ability_source",
        "at_event",
        {"kind": "ObjectSet", "objects": [], "origin": "holder"},
    )
    assert (
        unsupported_port_reason(value, direction="imports")
        == "unsupported_port_signature"
    )
    value = port(
        "name_source",
        "lki",
        {"kind": "ObjectSet", "objects": [], "origin": "event_subject"},
    )
    assert (
        unsupported_port_reason(value, direction="imports")
        == "unsupported_object_producer_mapping"
    )


@pytest.mark.parametrize("event_kind", ["EnterField", "Play", "Damage"])
def test_event_kind_is_not_interchangeable(event_kind: str) -> None:
    value = port(
        "dice_event",
        "at_event",
        {
            "kind": "EventRef",
            "event_kind": event_kind,
            "event_id": "fixture:event",
            "subject": {"object_id": "fixture:object", "generation": 0},
        },
    )
    assert (
        unsupported_port_reason(value, direction="imports")
        == "unsupported_port_signature"
    )


def test_legacy_port_without_evidence_remains_unsupported() -> None:
    value = Port(
        name="mode_count",
        type="QuantityExpr",
        source_role="choice_mode_count",
        scope="ability_0",
    )
    assert (
        unsupported_port_reason(value, direction="imports")
        == "unsupported_port_evidence"
    )


def test_pool_source_kind_remains_specific_to_the_observed_operation() -> None:
    value = port(
        "pool_fill_battlefield",
        "selection",
        {
            "kind": "PoolContext",
            "pool_id": "fixture:pool",
            "source_kind": "search_result",
            "receipt": "search_receipt",
            "scope": "ability_0",
        },
    )
    assert (
        unsupported_port_reason(value, direction="imports")
        == "unsupported_port_signature"
    )
