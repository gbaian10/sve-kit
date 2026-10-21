"""Local syntax analysis uses real schema sites, independently of source admission."""

import json
from pathlib import Path

import pytest
from pydantic import ConfigDict, TypeAdapter

from sve_carddb.contracts.author_programs import AbilityHead
from sve_carddb.domains.effects.author_bindings import BindingAnalysis, analyze_bindings
from sve_carddb.domains.effects.author_sites import NodeSites

_HEAD = TypeAdapter(
    AbilityHead, config=ConfigDict(strict=True, regex_engine="python-re")
)


@pytest.fixture(scope="module")
def sites() -> NodeSites:
    return NodeSites(Path(__file__).resolve().parents[4] / "dsl")


def analyze(sites: NodeSites, value: object) -> BindingAnalysis:
    return analyze_bindings(_HEAD.validate_json(json.dumps(value)), sites)


def choose(name: str = "picked") -> dict[str, object]:
    return {"choose": {"bind": name, "from": {"who": "self", "zone": "field"}, "n": 1}}


def test_sequential_outputs_and_receipt_types(sites: NodeSites) -> None:
    result = analyze(
        sites,
        {
            "spell": {
                "do": [
                    choose(),
                    {"move": {"ref": "picked", "to": "hand", "result": "moved"}},
                    {"banish": {"ref": {"follow": "moved"}}},
                ]
            }
        },
    )
    assert not result.violations
    assert [(item.name, item.type) for item in result.declarations] == [
        ("picked", "ObjectSet"),
        ("moved", "ReceiptId"),
    ]


@pytest.mark.parametrize(
    ("steps", "reason"),
    [
        ([{"banish": {"ref": "picked"}}, choose()], "binding_not_visible"),
        ([choose(), choose()], "duplicate_local_binding"),
        (
            [choose(), {"banish": {"ref": {"follow": "picked"}}}],
            "binding_type_mismatch",
        ),
        (
            [{"draw": {"n": {"count_of": "moved"}, "result": "moved"}}],
            "binding_not_visible",
        ),
    ],
)
def test_local_failures(sites: NodeSites, steps: object, reason: str) -> None:
    result = analyze(sites, {"spell": {"do": steps}})
    assert reason in {item.reason for item in result.violations}


def test_branch_local_and_unsupported_join(sites: NodeSites) -> None:
    result = analyze(
        sites,
        {
            "spell": {
                "do": [
                    {
                        "if": {
                            "cond": {"my_turn": True},
                            "then": [choose()],
                            "else": [choose()],
                        }
                    },
                    {"destroy": "picked"},
                ]
            }
        },
    )
    assert {item.reason for item in result.violations} == {"unsupported_binding_merge"}


def test_simultaneous_children_read_input_then_publish_outputs(
    sites: NodeSites,
) -> None:
    good = analyze(
        sites,
        {
            "spell": {
                "do": [
                    {
                        "simultaneous": {
                            "do": [
                                {"draw": {"n": 1, "result": "first"}},
                                {"draw": {"n": 1, "result": "second"}},
                            ]
                        }
                    },
                    {"draw": {"n": {"count_of": "first"}}},
                ]
            }
        },
    )
    assert not good.violations
    bad = analyze(
        sites,
        {
            "spell": {
                "do": [
                    {
                        "simultaneous": {
                            "do": [
                                {"draw": {"n": 1, "result": "first"}},
                                {"draw": {"n": {"count_of": "first"}}},
                            ]
                        }
                    }
                ]
            }
        },
    )
    assert {item.reason for item in bad.violations} == {"binding_not_visible"}


@pytest.mark.parametrize(
    "nested",
    [
        {
            "grant": {
                "on": "this",
                "abilities": [{"fanfare": {"do": [{"destroy": "picked"}]}}],
                "until": "turn",
            }
        },
        {
            "delayed": {
                "on": {"phase": {"which": "end", "whose": "self"}},
                "do": [{"destroy": "picked"}],
            }
        },
    ],
)
def test_new_execution_cannot_capture_local(sites: NodeSites, nested: object) -> None:
    result = analyze(sites, {"spell": {"do": [choose(), nested]}})
    assert "binding_not_visible" in {item.reason for item in result.violations}


def test_unused_alias_cycles_are_rejected(sites: NodeSites) -> None:
    result = analyze(sites, {"spell": {"define": {"A": "B", "B": "A"}, "do": []}})
    assert "cyclic_binding_alias" in {item.reason for item in result.violations}


def test_foreach_and_by_kind_have_typed_local_references(sites: NodeSites) -> None:
    result = analyze(
        sites,
        {
            "spell": {
                "do": [
                    choose(),
                    {
                        "foreach": {
                            "in": "picked",
                            "as": "member",
                            "do": [{"destroy": "member"}],
                        }
                    },
                    {
                        "by_kind": {
                            "ref": "this",
                            "leader": [{"life": {"who": "item", "delta": 1}}],
                            "follower": [{"destroy": "item"}],
                        }
                    },
                ]
            }
        },
    )
    assert not result.violations
