"""Public contracts reject semantic guesses while preserving normal presentation evidence."""

from dataclasses import replace
from typing import Literal

import pytest

from sve_carddb.contracts.four_layer import (
    ContextExpression,
    Domain,
    EventRef,
    Expression,
    GlossaryReference,
    LeafRef,
    LeafSchema,
    LeafSlot,
    LiteralNode,
    ModeCountRule,
    ObjectGeneration,
    ObjectSetRef,
    PendingCondition,
    Port,
    PortEvidence,
    PredicateExpr,
    ReceiptRef,
    Scope,
    Span,
    Target,
)
from sve_carddb.contracts.source_binding import QuantityDomain, TypedValue, verify_value
from sve_carddb.domains.translations.four_layer_context import (
    DEFAULT_REGISTRY,
    Descriptor,
    ImportSignature,
    typed_import,
    verify_mode_count,
)
from sve_carddb.domains.translations.four_layer_expressions import (
    ExpressionTargets,
    target_dependencies,
    verify_quantities,
)
from sve_carddb.domains.translations.four_layer_render import SelectedTarget

from .context_fixtures import ContractCase, contract_case
from .expression_fixtures import expression_plan
from .test_four_layer_expressions import EXPRESSION_ID, sample


@pytest.mark.parametrize("family", ["A1", "A2", "A3", "A11"])
def test_cross_family_contracts_render_and_satisfy_required_closure(
    family: Literal["A1", "A2", "A3", "A11"],
) -> None:
    case = contract_case(family)
    item = case.item
    verify_quantities(item.frame, item.binding, scopes={})
    verify_mode_count(case.mode_rule, item.frame, item.binding)
    rendered = case.renderer.render(
        "synthetic", "zh-Hant", expression_plan((item,), case.targets)
    )
    assert rendered.rendered is not None
    assert rendered.rendered.text.startswith("測試") or "測試" in rendered.rendered.text
    assert {s.name for s in item.frame.leaf_schema.slots} <= {
        leaf.slot for leaf in rendered.rendered.leaves
    }
    if family in {"A1", "A3", "A11"}:
        assert rendered.rendered.annotation.occurrences
    assert rendered.issues == ()
    assert item.selected is not None
    with pytest.raises(ValueError, match="required"):
        item.selected.target.verify(item.frame.leaf_schema, "zh-Hant", {})
    item.selected.target.verify(
        item.frame.leaf_schema,
        "zh-Hant",
        {},
        dependencies=target_dependencies(item.frame, item.binding),
    )


@pytest.mark.parametrize(
    "change",
    [
        "missing_provider",
        "wrong_port_type",
        "wrong_event",
        "generation",
        "cardinality",
        "capture",
        "owner",
        "branch",
        "unassigned",
    ],
)
def test_context_ports_reject_invalid_connections(change: str) -> None:
    case = contract_case("A11")
    frame, binding = case.item.frame, case.item.binding
    ports = list(frame.projection.imports)
    index = next(i for i, p in enumerate(ports) if p.name == "entry")
    port = ports[index]
    assert port.evidence is not None
    assert isinstance(port.evidence.value, EventRef)
    if change == "missing_provider":
        ports.pop(index)
    elif change == "wrong_port_type":
        ports[index] = port.model_copy(update={"type": "ReceiptId"})
    elif change == "unassigned":
        ports[index] = port.model_copy(update={"evidence": None})
    elif change == "branch":
        scopes = (
            Scope(id="ability", parent=None, kind="ability"),
            Scope(id="left", parent="ability", kind="branch"),
        )
        frame = frame.model_copy(
            update={
                "projection": frame.projection.model_copy(update={"scopes": scopes})
            }
        )
        ports[index] = port.model_copy(update={"scope": "left"})
    elif change == "cardinality":
        index = next(i for i, p in enumerate(ports) if p.name == "subject")
        port = ports[index]
        assert port.evidence is not None
        ports[index] = port.model_copy(
            update={
                "evidence": port.evidence.model_copy(
                    update={
                        "value": port.evidence.value.model_copy(update={"objects": ()})
                    }
                )
            }
        )
    else:
        evidence = port.evidence
        assert evidence is not None
        if change == "capture":
            evidence = evidence.model_copy(update={"capture": "live"})
        elif change == "owner":
            evidence = evidence.model_copy(
                update={
                    "source": evidence.source.model_copy(
                        update={"source_hash": "f" * 64}
                    )
                }
            )
        else:
            update = (
                {"event_kind": "Damage"}
                if change == "wrong_event"
                else {
                    "subject": ObjectGeneration(
                        object_id="synthetic-holder", generation=2
                    )
                }
            )
            evidence = evidence.model_copy(
                update={"value": evidence.value.model_copy(update=update)}
            )
        ports[index] = port.model_copy(update={"evidence": evidence})
    frame = frame.model_copy(
        update={
            "projection": frame.projection.model_copy(update={"imports": tuple(ports)})
        }
    )
    with pytest.raises(ValueError, match=r"[Ee]xpression|[Ee]ntry"):
        verify_quantities(frame, binding, scopes={})


@pytest.mark.parametrize(
    "change", ["unknown", "boolean", "missing_actual", "cycle", "schema"]
)
def test_context_values_reject_unregistered_or_invalid_dependencies(
    change: str,
) -> None:
    case = contract_case("A11")
    frame, binding = case.item.frame, case.item.binding
    values = dict(binding.values)
    condition = values["condition"]
    assert isinstance(condition, PredicateExpr)
    if change == "unknown":
        values["condition"] = condition.model_copy(
            update={"expression_id": "unsealed.condition.v1"}
        )
    elif change == "boolean":
        values["condition"] = ContextExpression(
            kind="context_expression",
            expression_id=condition.expression_id,
            leaves=condition.leaves,
            imports=condition.imports,
            scope=condition.scope,
        )
    elif change == "missing_actual":
        values.pop("from")
    elif change == "cycle":
        values["from"] = condition
    else:
        frame = frame.model_copy(
            update={
                "leaf_schema": frame.leaf_schema.model_copy(
                    update={
                        "slots": tuple(
                            s.model_copy(update={"expression": None})
                            if s.name == "condition"
                            else s
                            for s in frame.leaf_schema.slots
                        )
                    }
                )
            }
        )
    with pytest.raises(ValueError, match=r"[Ee]xpression|[Cc]yclic"):
        verify_quantities(
            frame, binding.model_copy(update={"values": values}), scopes={}
        )


def test_unsealed_entry_operands_and_old_quantity_domains_are_rejected() -> None:
    case = contract_case("A11")
    values = dict(case.item.binding.values)
    values["from"] = ("deck",)
    with pytest.raises(ValueError, match="sealed"):
        verify_quantities(
            case.item.frame,
            case.item.binding.model_copy(update={"values": values}),
            scopes={},
        )
    slot = case.item.frame.leaf_schema.slots[-1]
    with pytest.raises(ValueError, match=r"outside|Unknown"):
        verify_value(slot, case.item.binding.values["condition"], {})


def test_pending_conditions_preserve_source_and_do_not_become_boolean_values() -> None:
    case = contract_case("A11")
    for reason in ("missing_provider", "unsealed_source"):
        pending = PendingCondition(
            state="pending",
            source_descriptor="synthetic.condition.v1",
            source_span=Span(start=0, end=4),
            raw_sha256="a" * 64,
            reason=reason,
        )
        rule = case.mode_rule.model_copy(update={"condition": pending})
        verify_mode_count(rule, case.item.frame, case.item.binding)
        assert rule.condition.model_dump(mode="json")["reason"] == reason
    with pytest.raises(ValueError, match="condition"):
        ModeCountRule.model_validate(
            case.mode_rule.model_dump(mode="json") | {"condition": False}
        )
    values = case.item.binding.values | {"condition": False}
    with pytest.raises(ValueError, match="typed predicate"):
        verify_mode_count(
            case.mode_rule,
            case.item.frame,
            case.item.binding.model_copy(update={"values": values}),
        )


def test_expression_targets_reject_unused_formals_and_missing_required_source_imports() -> (
    None
):
    case = contract_case("A3")
    targets = dict(case.targets.targets)
    targets["name.distinct_count.objects.v1", "zh-Hant"] = SelectedTarget(
        Target(format=1, nodes=(LiteralNode(kind="Literal", text="藏起來源"),))
    )
    with pytest.raises(ValueError, match="required"):
        case.renderer.render(
            "synthetic",
            "zh-Hant",
            expression_plan((case.item,), ExpressionTargets(targets)),
        )
    item = replace(
        case.item,
        selected=SelectedTarget(
            Target(format=1, nodes=(LeafRef(kind="LeafRef", slot="name"),))
        ),
    )
    with pytest.raises(ValueError, match="required"):
        case.renderer.render(
            "synthetic", "zh-Hant", expression_plan((item,), case.targets)
        )


def test_required_closure_rejects_cycles_and_unknown_actuals() -> None:
    schema = LeafSchema(
        format=2, slots=contract_case("A11").item.frame.leaf_schema.slots
    )
    target = Target(format=1, nodes=(LeafRef(kind="LeafRef", slot="condition"),))
    for dependencies in (
        {"condition": ("from",), "from": ("condition",)},
        {"condition": ("missing",)},
    ):
        with pytest.raises(ValueError, match=r"Cyclic|unknown"):
            target.verify(schema, "zh-Hant", {}, dependencies=dependencies)


def _zone_count_case() -> ContractCase:
    """Place a sealed B11 current-zone count beside the A1 contract."""
    case = contract_case("A1")
    base, _, targets = sample()
    values: dict[str, TypedValue] = {
        "count_owner": "self",
        "count_zone": ("hand",),
        "count": Expression(
            kind="expression",
            expression_id=EXPRESSION_ID,
            leaves=("count_owner", "count_zone"),
        ),
    }
    slots = tuple(
        slot.model_copy(update={"name": name})
        for name, slot in zip(values, base.frame.leaf_schema.slots, strict=True)
    )
    item = replace(
        case.item,
        frame=case.item.frame.model_copy(
            update={
                "leaf_schema": LeafSchema(
                    format=2, slots=(*case.item.frame.leaf_schema.slots, *slots)
                )
            }
        ),
        binding=case.item.binding.model_copy(
            update={"values": case.item.binding.values | values}
        ),
    )
    return replace(
        case,
        item=item,
        targets=ExpressionTargets(case.targets.targets | targets.targets),
    )


@pytest.mark.parametrize("family", ["A1", "A2", "B11"])
def test_single_multiplier_is_shared_by_current_and_history_reads(
    family: Literal["A1", "A2", "B11"],
) -> None:
    case = _zone_count_case() if family == "B11" else contract_case(family)
    item = case.item
    if family == "A2":
        history = item.binding.values["history"]
        assert isinstance(history, ContextExpression)
        history = history.model_copy(
            update={"expression_id": "history.play.count_subjects.v1"}
        )
        schema = item.frame.leaf_schema.model_copy(
            update={
                "slots": tuple(
                    slot.model_copy(update={"expression": history})
                    if slot.name == "history"
                    else slot
                    for slot in item.frame.leaf_schema.slots
                )
            }
        )
        item = replace(
            item,
            frame=item.frame.model_copy(update={"leaf_schema": schema}),
            binding=item.binding.model_copy(
                update={"values": item.binding.values | {"history": history}}
            ),
        )
        selected = SelectedTarget(
            Target(
                format=1,
                nodes=(LiteralNode(kind="Literal", text="本回合測試打出張數"),),
            )
        )
        case = replace(
            case,
            item=item,
            targets=ExpressionTargets(
                case.targets.targets | {(history.expression_id, "zh-Hant"): selected}
            ),
        )
    base_name = {"A1": "read", "A2": "history", "B11": "count"}[family]
    scale = ContextExpression(
        kind="context_expression",
        expression_id="quantity.scale.once.v1",
        leaves=(base_name, "factor"),
        imports={},
        scope="ability",
    )
    descriptor = DEFAULT_REGISTRY.definition(scale)
    slots = (
        *item.frame.leaf_schema.slots,
        LeafSlot(
            name="factor",
            type="Nat",
            role="constant_multiplier",
            domain=Domain(values=(), min=0, max=9007199254740991),
            required=True,
            occurrences=(),
        ),
        LeafSlot(
            name="scaled",
            type="QuantityExpr",
            role="read_value",
            domain=Domain(values=(descriptor.domain,), min=None, max=None),
            required=True,
            occurrences=(),
            expression=scale,
        ),
    )
    frame = item.frame.model_copy(
        update={"leaf_schema": LeafSchema(format=2, slots=slots)}
    )
    binding = item.binding.model_copy(
        update={"values": item.binding.values | {"factor": 2, "scaled": scale}}
    )
    verify_quantities(frame, binding, scopes={})
    assert DEFAULT_REGISTRY.result_measure(scale, binding) == "card_count"
    selected = SelectedTarget(
        Target(
            format=1,
            nodes=(
                LeafRef(kind="LeafRef", slot="value"),
                LiteralNode(kind="Literal", text="乘"),
                LeafRef(kind="LeafRef", slot="factor"),
            ),
        )
    )
    targets = ExpressionTargets(
        case.targets.targets | {(scale.expression_id, "zh-Hant"): selected}
    )
    item = replace(
        item,
        frame=frame,
        binding=binding,
        selected=SelectedTarget(
            Target(
                format=1,
                nodes=(
                    LeafRef(kind="LeafRef", slot="condition"),
                    LeafRef(kind="LeafRef", slot="scaled"),
                ),
            )
        ),
    )
    result = case.renderer.render(
        "synthetic", "zh-Hant", expression_plan((item,), targets)
    )
    assert result.rendered is not None
    assert result.rendered.text.endswith("乘2")
    for operand in ("scaled", "condition"):
        nested = scale.model_copy(update={"leaves": (operand, "factor")})
        values = binding.values | {"scaled": nested}
        schema = frame.leaf_schema.model_copy(
            update={
                "slots": (
                    *slots[:-1],
                    slots[-1].model_copy(update={"expression": nested}),
                )
            }
        )
        with pytest.raises(ValueError, match=r"nested|type|role|domain"):
            verify_quantities(
                frame.model_copy(update={"leaf_schema": schema}),
                binding.model_copy(update={"values": values}),
                scopes={},
            )
    if family == "B11":
        # A B11 offset already spends the single arithmetic layer.
        schema = frame.leaf_schema.model_copy(
            update={
                "slots": (
                    *slots,
                    slots[-2].model_copy(
                        update={"name": "offset", "role": "arithmetic_offset"}
                    ),
                )
            }
        )
        values = binding.values | {
            "count": Expression(
                kind="expression",
                expression_id="count.current_zone.owner.zone.subtract_offset.v1",
                leaves=("count_owner", "count_zone", "offset"),
            ),
            "offset": 1,
        }
        with pytest.raises(ValueError, match="nested"):
            verify_quantities(
                frame.model_copy(update={"leaf_schema": schema}),
                binding.model_copy(update={"values": values}),
                scopes={},
            )


@pytest.mark.parametrize(
    "descriptor",
    [
        "entry.self.from_zone.ne.v1",
        "entry.event_subject.from_zone.eq.v1",
        "entry.self.method.ability.v1",
        "entry.self.method.ability_source_trait.v1",
    ],
)
def test_all_entry_descriptor_signatures_and_direct_source_trait(
    descriptor: str,
) -> None:
    case = contract_case("A11")
    item = case.item
    rule = DEFAULT_REGISTRY.descriptors[descriptor]
    expr = PredicateExpr(
        kind="predicate",
        expression_id=descriptor,
        leaves=("operand",) if rule.parameters else (),
        imports={
            p.name: {
                "entry_subject": "subject",
                "entry_fact": "entry",
                "ability_source": "direct",
            }[p.name]
            for p in rule.imports
        },
        scope="ability",
    )
    slots = []
    values: dict[str, TypedValue] = {"condition": expr}
    if rule.parameters:
        param = rule.parameters[0]
        value = param.values[0]
        values["operand"] = value
        slots.append(
            LeafSlot(
                name="operand",
                type=param.type,
                role=param.role,
                domain=Domain(values=(param.domain,), min=None, max=None),
                required=True,
                occurrences=(),
            )
        )
    slots.append(
        item.frame.leaf_schema.slots[-1].model_copy(update={"expression": expr})
    )
    ports = []
    for original_port in item.frame.projection.imports:
        port = original_port
        if port.name == "subject" and descriptor.startswith("entry.event_subject"):
            assert port.evidence is not None
            port = port.model_copy(
                update={
                    "evidence": port.evidence.model_copy(
                        update={
                            "value": port.evidence.value.model_copy(
                                update={"origin": "event_subject"}
                            )
                        }
                    )
                }
            )
        if port.name == "entry" and "trait" in descriptor:
            assert port.evidence is not None
            source = ObjectGeneration(object_id="direct-card", generation=3)
            port = port.model_copy(
                update={
                    "evidence": port.evidence.model_copy(
                        update={
                            "value": port.evidence.value.model_copy(
                                update={"direct_ability_source": source}
                            )
                        }
                    )
                }
            )
            ports.append(
                Port(
                    name="direct",
                    type="ObjectSet",
                    source_role="ability_source",
                    scope="ability",
                    evidence=PortEvidence(
                        source=item.binding.occurrence_key(),
                        capture="lki",
                        value=ObjectSetRef(
                            kind="ObjectSet",
                            objects=(source,),
                            origin="direct_ability_source",
                        ),
                    ),
                )
            )
        ports.append(port)
    frame = item.frame.model_copy(
        update={
            "leaf_schema": LeafSchema(format=2, slots=tuple(slots)),
            "projection": item.frame.projection.model_copy(
                update={"imports": tuple(sorted(ports, key=lambda p: p.name))}
            ),
        }
    )
    binding = item.binding.model_copy(update={"values": values})
    verify_quantities(frame, binding, scopes={})
    if "trait" in descriptor:
        values["operand"] = GlossaryReference(
            kind="glossary", key="term:trait.synthetic-other"
        )
        with pytest.raises(ValueError, match="sealed"):
            verify_quantities(
                frame, binding.model_copy(update={"values": values}), scopes={}
            )


def test_family_owned_descriptor_registration_is_explicit_and_closed() -> None:
    own = Descriptor(
        "synthetic.family.predicate.v1",
        "predicate.synthetic.v1",
        "synthetic_condition",
        "Bool",
        "truth",
        (),
        (),
        "P5",
        ("X.predicate",),
    )
    registry = DEFAULT_REGISTRY.extend((own,))
    assert own.id in registry.descriptors
    assert own.id not in DEFAULT_REGISTRY.descriptors
    assert registry.domains[own.domain].type == "PredicateExpr"
    for invalid in ((own, own), (replace(own, result_type="Nat"), own)):
        with pytest.raises(ValueError, match="Duplicate"):
            DEFAULT_REGISTRY.extend(invalid)
    with pytest.raises(ValueError, match="truth"):
        DEFAULT_REGISTRY.extend((replace(own, measure="card_count"),))


def test_play_event_contract_is_separate_from_entry_and_checks_subject_generation() -> (
    None
):
    case = contract_case("A11")
    item = case.item
    entry = DEFAULT_REGISTRY.descriptors["entry.self.from_zone.eq.v1"]
    imports = tuple(
        replace(p, event_kind="Play") if p.type == "EventRef" else p
        for p in entry.imports
    )
    own = replace(
        entry,
        id="synthetic.played_from.v1",
        domain="predicate.synthetic.play.v1",
        source_family="played_from",
        imports=imports,
    )
    registry = DEFAULT_REGISTRY.extend((own,))
    predicate = item.binding.values["condition"]
    assert isinstance(predicate, PredicateExpr)
    predicate = predicate.model_copy(update={"expression_id": own.id})
    slots = (
        *item.frame.leaf_schema.slots[:-1],
        item.frame.leaf_schema.slots[-1].model_copy(
            update={
                "expression": predicate,
                "domain": Domain(values=(own.domain,), min=None, max=None),
            }
        ),
    )
    ports = tuple(
        p.model_copy(
            update={
                "evidence": p.evidence.model_copy(
                    update={
                        "value": p.evidence.value.model_copy(
                            update={"event_kind": "Play"}
                        )
                    }
                )
            }
        )
        if p.name == "entry" and p.evidence
        else p
        for p in item.frame.projection.imports
    )
    frame = item.frame.model_copy(
        update={
            "leaf_schema": LeafSchema(format=2, slots=slots),
            "projection": item.frame.projection.model_copy(update={"imports": ports}),
        }
    )
    binding = item.binding.model_copy(
        update={"values": item.binding.values | {"condition": predicate}}
    )
    verify_quantities(frame, binding, scopes={}, registry=registry)
    for kind, generation in (("EnterField", 1), ("Play", 2)):
        wrong = tuple(
            p.model_copy(
                update={
                    "evidence": p.evidence.model_copy(
                        update={
                            "value": p.evidence.value.model_copy(
                                update={
                                    "event_kind": kind,
                                    "subject": ObjectGeneration(
                                        object_id="synthetic-holder",
                                        generation=generation,
                                    ),
                                }
                            )
                        }
                    )
                }
            )
            if p.name == "entry" and p.evidence
            else p
            for p in ports
        )
        with pytest.raises(ValueError, match=r"event kind|generation"):
            verify_quantities(
                frame.model_copy(
                    update={
                        "projection": frame.projection.model_copy(
                            update={"imports": wrong}
                        )
                    }
                ),
                binding,
                scopes={},
                registry=registry,
            )


def test_preparation_structure_changes_identity_and_requires_v9() -> None:
    case = contract_case("A11")
    frame = case.item.frame
    changed = frame.model_copy(update={"mode_rules": (case.mode_rule,)})
    assert frame.payload("synthetic") != changed.payload("synthetic")
    document = changed.model_dump()
    for version in ("four-layer-jp-v2", "four-layer-jp-v7", "four-layer-jp-v8"):
        document["source"]["normalizer_version"] = version
        with pytest.raises(ValueError, match="requires a v9"):
            type(frame).model_validate(document)
    read = contract_case("A1").item.binding.values["read"]
    slot = next(
        s for s in contract_case("A1").item.frame.leaf_schema.slots if s.name == "read"
    )
    old = slot.model_copy(
        update={
            "domain": Domain(values=("quantity.read.context.v1",), min=None, max=None)
        }
    )
    domain = QuantityDomain(
        type="QuantityExpr",
        modes=(),
        imports=(),
        expressions=("read.faceup_evolve.count.v1",),
    )
    with pytest.raises(ValueError, match="outside"):
        verify_value(old, read, {"quantity.read.context.v1": domain})


def test_receipt_imports_declare_their_result_kind() -> None:
    receipt = ImportSignature("receipt", "ReceiptId", "receipt")
    own = replace(
        DEFAULT_REGISTRY.descriptors["read.receipt.amount.v1"],
        id="synthetic.receipt.read.v1",
        imports=(receipt,),
    )
    with pytest.raises(ValueError, match="result kind"):
        DEFAULT_REGISTRY.extend((own,))
    case = contract_case("A1")
    frame, binding = case.item.frame, case.item.binding
    port = Port(
        name="receipt",
        type="ReceiptId",
        source_role="receipt",
        scope="ability",
        evidence=PortEvidence(
            source=binding.occurrence_key(),
            capture="receipt",
            value=ReceiptRef(kind="ReceiptId", receipt_id="cost", result="objects"),
        ),
    )
    frame = frame.model_copy(
        update={
            "projection": frame.projection.model_copy(
                update={"imports": (*frame.projection.imports, port)}
            )
        }
    )
    typed_import(
        frame, binding, "receipt", "ability", replace(receipt, receipt_result="objects")
    )
    with pytest.raises(ValueError, match="result kind"):
        typed_import(
            frame,
            binding,
            "receipt",
            "ability",
            replace(receipt, receipt_result="amount"),
        )
