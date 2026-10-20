"""Local syntax analysis uses real schema sites, independently of source admission."""

import json
from pathlib import Path

import pytest
from pydantic import ConfigDict, TypeAdapter

from sve_carddb.contracts.author_evidence import Ports
from sve_carddb.contracts.author_programs import AbilityHead
from sve_carddb.contracts.four_layer import Projection
from sve_carddb.domains.effects.author_bindings import (
    BindingAnalysis,
    analyze_bindings,
    validate_author_ports,
    validate_local_ports,
)
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


def test_targets_array_and_card_name_not_reference(sites: NodeSites) -> None:
    result = analyze(
        sites,
        {
            "spell": {
                "targets": [
                    {
                        "bind": "target",
                        "from": {"who": "opp", "zone": "field", "name": "notABinding"},
                        "n": 1,
                    }
                ],
                "do": [{"destroy": "target"}],
            }
        },
    )
    assert not result.violations
    assert result.declarations[0].path == ("spell", "targets", 0)
    assert result.declarations[0].type == "ObjectSet"


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


def test_replacement_and_after_are_analyzed(sites: NodeSites) -> None:
    result = analyze(
        sites,
        {
            "spell": {
                "do": [
                    {
                        "draw": {
                            "n": 1,
                            "instead": [
                                {
                                    "if": {"my_turn": True},
                                    "replace_with": {
                                        "draw": {"n": 1, "result": "inside"}
                                    },
                                    "after": [{"draw": {"n": {"count_of": "inside"}}}],
                                }
                            ],
                        }
                    },
                    {"draw": {"n": {"count_of": "inside"}}},
                ]
            }
        },
    )
    assert len(result.violations) == 1
    assert result.violations[0].path == ("spell", "do", 1, "draw", "n", "count_of")


def test_define_expands_at_use_and_does_not_depend_on_key_order(
    sites: NodeSites,
) -> None:
    result = analyze(
        sites,
        {
            "spell": {
                "define": {"B": {"bound": "A"}, "A": {"count": {"in": "picked"}}},
                "do": [choose(), {"draw": {"n": "B"}}],
            }
        },
    )
    assert not result.violations
    cycle = analyze(
        sites, {"spell": {"define": {"A": "B", "B": "A"}, "do": [{"draw": {"n": "A"}}]}}
    )
    assert "cyclic_binding_alias" in {item.reason for item in cycle.violations}


def test_exact_export_path_and_types(sites: NodeSites) -> None:
    result = analyze(sites, {"spell": {"do": [choose()]}})
    ports = Ports.model_validate_json(
        json.dumps(
            {
                "imports": {},
                "exports": {
                    "selected": {"path": ["spell", "do", 0, "choose"], "bind": "picked"}
                },
            }
        )
    )
    projection = Projection.model_validate_json(
        json.dumps(
            {
                "projection_kind": "ability_body",
                "discriminator": "synthetic",
                "scopes": [{"id": "ability_0", "parent": None, "kind": "ability"}],
                "imports": [],
                "exports": [
                    {
                        "name": "selected",
                        "type": "ObjectSet",
                        "source_role": "synthetic_role",
                        "scope": "ability_0",
                    }
                ],
            }
        )
    )
    assert not validate_author_ports(result, ports)
    assert not validate_local_ports(result, ports, projection)
    bad = Ports.model_validate_json(
        json.dumps(
            {
                "imports": {},
                "exports": {
                    "selected": {
                        "path": ["spell", "do", 0, "choose", "bind"],
                        "bind": "picked",
                    }
                },
            }
        )
    )
    assert (
        validate_author_ports(result, bad)[0].reason
        == "missing_or_ambiguous_local_export"
    )


def test_unknown_import_is_not_any_type(sites: NodeSites) -> None:
    head = _HEAD.validate_json('{"spell":{"do":[{"destroy":"external"}]}}')
    result = analyze_bindings(head, sites, {"external": "Unknown"})
    assert result.violations[0].reason == "unsupported_import_binding_type"


@pytest.mark.parametrize(
    "head",
    [
        {
            "fanfare": {
                "define": {
                    "X": {"minus": [{"count": {"who": "self", "zone": "deck"}}, 5]}
                },
                "do": [{"draw": {"n": "X"}}],
            }
        },
        {
            "evolve_time": {
                "ub": True,
                "do": [
                    {
                        "search": {
                            "sel": {
                                "who": "self",
                                "zone": "deck",
                                "trait": "ドラゴンズネスト",
                                "not_same_name_as_this": True,
                            },
                            "n": 1,
                            "to": "ex",
                            "result": "searched",
                        }
                    },
                    {
                        "apply": {
                            "on": {"follow": "searched"},
                            "effect": {
                                "cost_mod": {
                                    "when_playing": {"in": "affected"},
                                    "delta": -2,
                                }
                            },
                            "until": "turn",
                        }
                    },
                ],
            }
        },
    ],
    ids=["bp08-038-kernel-only", "cp04-060-kernel-only"],
)
def test_existing_golden_binding_patterns(sites: NodeSites, head: object) -> None:
    assert not analyze(sites, head).violations


def test_unused_alias_cycles_are_rejected(sites: NodeSites) -> None:
    result = analyze(sites, {"spell": {"define": {"A": "B", "B": "A"}, "do": []}})
    assert "cyclic_binding_alias" in {item.reason for item in result.violations}


@pytest.mark.parametrize(
    "expression", [{"bound": "missing"}, {"count": {"in": "missing"}}]
)
def test_unused_alias_cannot_hide_a_dangling_reference(
    sites: NodeSites, expression: object
) -> None:
    result = analyze(sites, {"spell": {"define": {"X": expression}, "do": []}})
    assert [item.reason for item in result.violations] == ["binding_not_visible"]


def test_unused_alias_sees_completed_execution_bindings(sites: NodeSites) -> None:
    result = analyze(
        sites,
        {"spell": {"define": {"X": {"count": {"in": "picked"}}}, "do": [choose()]}},
    )
    assert not result.violations


@pytest.mark.parametrize(
    "control",
    [
        {"optional": {"do": [choose()], "then": [{"destroy": "picked"}]}},
        {"pay": {"cost": {"pp": 1}, "do": [choose()], "then": [{"destroy": "picked"}]}},
    ],
)
def test_guarded_sequence_shares_locals_but_does_not_export(
    sites: NodeSites, control: object
) -> None:
    result = analyze(sites, {"spell": {"do": [control, {"destroy": "picked"}]}})
    assert len(result.violations) == 1
    assert result.violations[0].path == ("spell", "do", 1, "destroy")


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


def test_branch_export_and_import_alias_duplicates_are_rejected(
    sites: NodeSites,
) -> None:
    result = analyze(sites, {"spell": {"do": [{"optional": {"do": [choose()]}}]}})
    ports = Ports.model_validate_json(
        json.dumps(
            {
                "imports": {
                    "a": {"path": [], "bind": "external"},
                    "b": {"path": [], "bind": "external"},
                },
                "exports": {
                    "out": {
                        "path": ["spell", "do", 0, "optional", "do", 0, "choose"],
                        "bind": "picked",
                    }
                },
            }
        )
    )
    assert {item.reason for item in validate_author_ports(result, ports)} == {
        "duplicate_import_binding",
        "unsupported_local_export_scope",
    }
