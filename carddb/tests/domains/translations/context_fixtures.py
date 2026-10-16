"""Synthetic typed producer contracts reusable by the five preparation families."""

from dataclasses import dataclass
from typing import Literal

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    ChoiceContext,
    ContextExpression,
    Domain,
    EventRef,
    LeafRef,
    LeafSchema,
    LeafSlot,
    LiteralNode,
    ModeCountRule,
    ObjectGeneration,
    ObjectSetRef,
    PlayerRef,
    Port,
    PortEvidence,
    PredicateExpr,
    Projection,
    Scope,
    Target,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_context import DEFAULT_REGISTRY
from sve_carddb.domains.translations.four_layer_expressions import ExpressionTargets
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Label,
    Renderer,
    SelectedTarget,
)

from .test_four_layer_expressions import sample


@dataclass(frozen=True)
class ContractCase:
    item: BoundTarget
    renderer: Renderer
    targets: ExpressionTargets
    mode_rule: ModeCountRule


def contract_case(family: Literal["A1", "A2", "A3", "A11"]) -> ContractCase:
    """Provide source-assigned ports, nested read/condition targets and a mode consumer."""
    base, renderer, _ = sample()
    occurrence = base.binding.occurrence_key()
    subject = ObjectGeneration(object_id="synthetic-holder", generation=1)
    slots: list[LeafSlot] = []
    values = {}
    ports = [
        Port(
            name="choice",
            type="ChoiceContext",
            source_role="choice_context",
            scope="ability",
            evidence=PortEvidence(
                source=occurrence,
                capture="play_parameter",
                value=ChoiceContext(
                    kind="ChoiceContext",
                    ability_instance="synthetic-ability",
                    turn="turn-1",
                    modes=("one", "two"),
                ),
            ),
        ),
    ]
    targets = {}

    def scalar(name: str, type_: str, role: str, domain: str, value: object) -> None:
        slot = LeafSlot.model_validate(
            {
                "name": name,
                "type": type_,
                "role": role,
                "domain": {
                    "values": (domain,) if domain else (),
                    "min": None if domain else 0,
                    "max": None if domain else 9007199254740991,
                },
                "required": True,
                "occurrences": (),
            }
        )
        slots.append(slot)
        values[name] = value

    def expression(
        name: str, expr: ContextExpression | PredicateExpr, text: str
    ) -> None:
        rule = DEFAULT_REGISTRY.definition(expr)
        slots.append(
            LeafSlot(
                name=name,
                type="QuantityExpr" if rule.result_type == "Nat" else "PredicateExpr",
                role=rule.role,
                domain=Domain(values=(rule.domain,), min=None, max=None),
                required=True,
                occurrences=(),
                expression=expr,
            )
        )
        values[name] = expr
        formals = tuple(LeafRef(kind="LeafRef", slot=p.name) for p in rule.parameters)
        if family == "A3" and name == "distinct":
            formals = (LeafRef(kind="LeafRef", slot="objects.0"),)
        targets[expr.expression_id, "zh-Hant"] = SelectedTarget(
            Target(format=1, nodes=(LiteralNode(kind="Literal", text=text), *formals))
        )

    if family == "A11":
        scalar("from", "ZoneSet", "entry_from_zone", "zone.single.v1", ("hand",))
        ports.extend(
            (
                Port(
                    name="subject",
                    type="ObjectSet",
                    source_role="entry_subject",
                    scope="ability",
                    evidence=PortEvidence(
                        source=occurrence,
                        capture="at_event",
                        value=ObjectSetRef(
                            kind="ObjectSet", objects=(subject,), origin="holder"
                        ),
                    ),
                ),
                Port(
                    name="entry",
                    type="EventRef",
                    source_role="entry_fact",
                    scope="ability",
                    evidence=PortEvidence(
                        source=occurrence,
                        capture="at_event",
                        value=EventRef(
                            kind="EventRef",
                            event_kind="EnterField",
                            event_id="synthetic-entry",
                            subject=subject,
                        ),
                    ),
                ),
            )
        )
        expression(
            "condition",
            PredicateExpr(
                kind="predicate",
                expression_id="entry.self.from_zone.eq.v1",
                leaves=("from",),
                imports={"entry_subject": "subject", "entry_fact": "entry"},
                scope="ability",
            ),
            "測試入場自",
        )
    else:
        if family == "A1":
            scalar("owner", "Player", "read_owner", "player.relative.v1", "self")
            scalar("zone", "ZoneSet", "read_zone", "zone.single.v1", ("evolve_deck",))
            scalar(
                "name",
                "CardName",
                "read_name",
                "card_name.any.v1",
                CardNameReference(kind="card_name", term_id="term:name.synthetic"),
            )
            expression(
                "read",
                ContextExpression(
                    kind="context_expression",
                    expression_id="read.faceup_evolve.count.v1",
                    leaves=("owner", "zone", "name"),
                    imports={},
                    scope="ability",
                ),
                "表向測試數",
            )
        elif family == "A2":
            ports.append(
                Port(
                    name="actor",
                    type="PlayerRef",
                    source_role="history_actor",
                    scope="ability",
                    evidence=PortEvidence(
                        source=occurrence,
                        capture="current_turn",
                        value=PlayerRef(kind="PlayerRef", player="self"),
                    ),
                )
            )
            expression(
                "history",
                ContextExpression(
                    kind="context_expression",
                    expression_id="history.evolve.count_facts.v1",
                    leaves=(),
                    imports={"actor": "actor"},
                    scope="ability",
                ),
                "本回合測試進化次數",
            )
            expression(
                "read",
                ContextExpression(
                    kind="context_expression",
                    expression_id="read.quantity.v1",
                    leaves=("history",),
                    imports={},
                    scope="ability",
                ),
                "讀取",
            )
        else:
            scalar(
                "name",
                "CardName",
                "read_name",
                "card_name.any.v1",
                CardNameReference(kind="card_name", term_id="term:name.synthetic"),
            )
            ports.append(
                Port(
                    name="objects",
                    type="ObjectSet",
                    source_role="selected_objects",
                    scope="ability",
                    evidence=PortEvidence(
                        source=occurrence,
                        capture="selection",
                        value=ObjectSetRef(
                            kind="ObjectSet", objects=(subject,), origin="selection"
                        ),
                        source_leaves=("name",),
                    ),
                )
            )
            expression(
                "distinct",
                ContextExpression(
                    kind="context_expression",
                    expression_id="name.distinct_count.objects.v1",
                    leaves=(),
                    imports={"objects": "objects"},
                    scope="ability",
                ),
                "測試名稱種類數",
            )
            expression(
                "read",
                ContextExpression(
                    kind="context_expression",
                    expression_id="read.quantity.v1",
                    leaves=("distinct",),
                    imports={},
                    scope="ability",
                ),
                "讀取",
            )
        scalar("threshold", "Nat", "comparison_threshold", "", 2)
        expression(
            "condition",
            PredicateExpr(
                kind="predicate",
                expression_id="condition.quantity.at_least.v1",
                leaves=("read", "threshold"),
                imports={},
                scope="ability",
            ),
            "測試達標",
        )
    frame = base.frame.model_copy(
        update={
            "leaf_schema": LeafSchema(format=2, slots=tuple(slots)),
            "projection": Projection(
                projection_kind="ability_body",
                discriminator="synthetic.context.v1",
                scopes=(Scope(id="ability", parent=None, kind="ability"),),
                imports=tuple(sorted(ports, key=lambda p: p.name)),
                exports=(),
            ),
        }
    )
    binding = base.binding.model_copy(update={"values": values})
    item = BoundTarget(
        frame,
        binding,
        SelectedTarget(
            Target(format=1, nodes=(LeafRef(kind="LeafRef", slot="condition"),))
        ),
    )
    labels = dict(renderer.labels)
    for ref, text in (
        (renderer.code_references["ZoneSet", "evolve_deck"], "進化牌堆"),
        (
            CardNameReference(kind="card_name", term_id="term:name.synthetic"),
            "測試名稱",
        ),
    ):
        label = Label(ref, "zh-Hant", text, "project", False, True)
        labels[canonical(ref.model_dump(mode="json")), "zh-Hant"] = label
    renderer = Renderer(
        labels, {}, {}, domains={**renderer.domains, **DEFAULT_REGISTRY.domains}
    )
    rule = ModeCountRule.model_validate(
        {
            "base": {"mode": "exact", "expr": {"kind": "constant", "value": 1}},
            "alternative": {"mode": "up_to", "expr": {"kind": "constant", "value": 2}},
            "condition": {"state": "resolved", "predicate": "condition"},
            "choice_context": "choice",
            "scope": "ability",
        }
    )
    return ContractCase(item, renderer, ExpressionTargets(targets), rule)
