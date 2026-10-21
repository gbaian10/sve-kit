"""Synthetic graphs verify closure only, never provenance or admission."""

import json
from dataclasses import replace

from sve_carddb.contracts.author_evidence import (
    ImportUse,
    InterfaceDependency,
    SourceUse,
)
from sve_carddb.contracts.four_layer import Port
from sve_carddb.domains.effects.author_imports import ImportNode, analyze_imports


def node(name: str, imports: dict[str, ImportUse] | None = None) -> ImportNode:
    source = SourceUse.model_validate_json(
        json.dumps(
            {
                "binding_id": name,
                "ability_scope": "ability_0",
                "occurrence": {
                    "owner": {"kind": "face_revision", "revision_id": name},
                    "field": "effect",
                    "ordinal": None,
                    "source_hash": "0" * 64,
                    "line_ordinal": 0,
                    "role": "body",
                    "segments": [{"start": 0, "end": 1}],
                },
            }
        )
    )
    interface = InterfaceDependency(
        frame_id="frame:" + name * 64, interface_key=name * 64
    )
    export = Port(
        name="out", type="ReceiptId", source_role="synthetic", scope="ability_0"
    )
    return ImportNode(source, interface, imports or {}, (export,))


def use(name: str) -> ImportUse:
    return ImportUse(binding_id=name, ability_scope="ability_0", port="out")


def test_transitive_closure_is_independent_of_input_order_and_owner() -> None:
    a, b, c = node("a"), node("b", {"in": use("a")}), node("c", {"in": use("b")})
    result = analyze_imports([c, a, b])
    assert not result.violations
    assert result.order == (a.key, b.key, c.key)
    assert len(result.closures[-1].interfaces) == 3
    assert result == analyze_imports([b, c, a])


def test_cycle_missing_and_ambiguous_uses_have_no_successful_closure() -> None:
    a = node("a", {"in": use("b")})
    b = node("b", {"in": use("a")})
    for graph, reason in (
        ([a, b], "cyclic_author_import"),
        ([a], "missing_import_use"),
        ([a, a], "ambiguous_import_use"),
    ):
        result = analyze_imports(graph)
        assert not result.order
        assert not result.closures
        assert reason in {item.reason for item in result.violations}


def test_exact_scope_port_and_interface_are_checked() -> None:
    a = node("a")
    b = node(
        "b", {"in": ImportUse(binding_id="a", ability_scope="ability_1", port="out")}
    )
    assert analyze_imports([a, b]).violations[0].reason == "missing_import_use"
    b = node(
        "b", {"in": ImportUse(binding_id="a", ability_scope="ability_0", port="other")}
    )
    assert analyze_imports([a, b]).violations[0].reason == "missing_or_ambiguous_export"
    b = node("b", {"in": use("a")})
    b = replace(
        b,
        interface=InterfaceDependency(
            frame_id=a.interface.frame_id, interface_key="f" * 64
        ),
    )
    assert (
        analyze_imports([a, b]).violations[0].reason == "conflicting_import_interface"
    )
