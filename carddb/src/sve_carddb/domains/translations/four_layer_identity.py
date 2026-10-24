"""Seal only the definitions used by a source frame, independently of release labels."""

from functools import cache
from typing import TYPE_CHECKING

from pydantic import JsonValue, TypeAdapter

from sve_carddb.contracts.four_layer import (
    ContextExpression,
    Expression,
    PredicateExpr,
    QuantitySpec,
    hash_payload,
)
from sve_carddb.contracts.source_binding import (
    PreparationDomain,
    QuantityDomain,
    ReferenceDomain,
)
from sve_carddb.core.json import canonical, object_value, parse
from sve_carddb.domains.translations.four_layer_expressions import DEFINITIONS
from sve_carddb.domains.translations.four_layer_units import unit_meaning

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import ClosedDomain, SourceBinding
    from sve_carddb.domains.translations.four_layer_context import Registry

_DEFINITION = TypeAdapter(object)

# These describe source meaning, not grammar coverage or interpreter implementation.
RESOLVED: dict[str, JsonValue] = {
    "damage_uniform_lastword_all_leaders.v1": {
        "kind": "damage",
        "trigger": {
            "kind": "printed_lastword",
            "holder": "source_holder",
            "event": "actual_field_to_card_owner_cemetery",
            "view": "before_exit_field",
            "multiplicity": "per_fact",
            "resolution": "pending_until_checkpoint_play",
            "master": "source_master_before_exit",
        },
        "recipients": "all_leaders_including_self",
        "amount": {"mode": "uniform", "slot": "leaf_1"},
        "damage_kind": "ability",
        "batch": "single_simultaneous_instruction",
        "source": "original_ability_holder_snapshot_or_lki",
        "ignores_prevention": False,
        "source_override": None,
        "zero": "no_damage_event",
        "ability_scope": "complete_single_line_normal_follower_effect",
        "fixed_head": {"slot": "leaf_0", "glossary": "term:ability.last_words"},
    },
    "metadata.name.v1": {"kind": "metadata", "field": "name"},
    "metadata.label.v1": {"kind": "metadata", "field": "label"},
    "metadata.layout.v1": {"kind": "metadata", "field": "layout"},
    "pure_reminder.v1": {"kind": "reminder", "adds_ability": False},
    "token_header.v1": {"kind": "token_declaration", "scope": "section"},
    "card_keywords.v1": {"kind": "card_keywords", "scope": "card"},
    "card_keywords.v2": {"kind": "card_keywords", "scope": "card"},
    "quick_card_field.v1": {"kind": "quick", "scope": "card"},
    **{
        name + "_entry.v1": {
            "kind": "entry",
            "actor": "self",
            "cost": "source_cost",
            "action": action,
        }
        for name, action in (("evolve", "evolve"), ("feed", "race"), ("ride", "drive"))
    },
    **{
        "evolve_" + name + "_entries.v1": {
            "kind": "separate_entry_abilities",
            "actor": "self",
            "actions": ["evolve", action],
            "costs": "separate_source_costs",
        }
        for name, action in (("feed", "race"), ("ride", "drive"))
    },
    **{
        "draw_exact_" + timing + ".v1": {
            "kind": "draw",
            "player": "self",
            "count": {"mode": "exact", "slot": "leaf_0"},
            "trigger": trigger,
            "ability_scope": "independent_whole_line",
        }
        for timing, trigger in (
            ("spell", "spell_resolution"),
            ("attack_time", "when_this_attacks"),
            ("evolve_time", "when_this_evolves"),
        )
    },
}

OMISSIONS: dict[str, JsonValue] = {
    "owner.actor_hand_cost.v1": {"owner": "self", "reason": "actor_pays_hand_cost"},
    "owner.self_hand_destination.v1": {
        "owner": "self",
        "reason": "actor_hand_destination",
    },
    "owner.return_inspected_deck.v1": {
        "owner": "self",
        "reason": "return_to_inspected_deck",
    },
    "earth_rite.omitted_amount.v1": {"amount": 1, "measure": "earth_sigil"},
    "aggregate.selector.count.v1": {"measure": "selected_objects", "mode": "all"},
}

KIND_SELECTIONS = {
    "card_kind.selection.v1": ("amulet", "follower", "spell"),
    "card_kind.follower.v1": ("follower",),
    "card_kind.amulet.v1": ("amulet",),
    "card_kind.spell.v1": ("spell",),
}


def _domain(value: ClosedDomain, name: str) -> JsonValue:
    if isinstance(value, ReferenceDomain):
        return {
            "type": value.type,
            "category": value.category,
            "selection": list(KIND_SELECTIONS.get(name, ())),
            "membership": "active_catalog_at_binding",
        }
    if isinstance(value, PreparationDomain):
        # Each source leaf carries its particular expression; unrelated additions do not change it.
        return {"type": value.type}
    if isinstance(value, QuantityDomain):
        data = value.model_dump(
            mode="json", exclude={"expressions", "context_expressions"}
        )
        data["modes"] = sorted(value.modes)
        data["imports"] = sorted(value.imports)
        return data
    return value.model_dump(mode="json")


def _expressions(
    frame: Frame, binding: SourceBinding, registry: Registry
) -> tuple[dict[str, JsonValue], set[str]]:
    expressions: dict[str, JsonValue] = {}
    domain_names: set[str] = set()
    values = [*binding.values.values()]
    values.extend(s.expression for s in frame.leaf_schema.slots if s.expression)
    for value in values:
        expr = value.expr if isinstance(value, QuantitySpec) else value
        if not isinstance(expr, (Expression, ContextExpression, PredicateExpr)):
            continue
        identifier = expr.expression_id
        if identifier in registry.descriptors:
            descriptor = registry.descriptors[identifier]
            data = object_value(parse(_DEFINITION.dump_json(descriptor)))
            data.pop("ir_bridge")
            data["imports"] = [
                object_value(parse(_DEFINITION.dump_json(item)))
                for item in sorted(descriptor.imports, key=lambda item: item.name)
            ]
            expressions[identifier] = data
            domain_names.update(p.domain for p in descriptor.parameters if p.domain)
        elif identifier in DEFINITIONS:
            expressions[identifier] = object_value(
                parse(_DEFINITION.dump_json(DEFINITIONS[identifier]))
            )
        else:
            raise ValueError("Unclosed frame semantic expression: " + identifier)
    return expressions, domain_names


def _omissions(binding: SourceBinding) -> dict[str, JsonValue]:
    result: dict[str, JsonValue] = {}
    for occurrence in binding.occurrences:
        rule = occurrence.resolution_rule
        if rule is not None:
            if rule not in OMISSIONS:
                raise ValueError("Unclosed frame omission definition: " + rule)
            definition = OMISSIONS[rule]
            if not isinstance(definition, dict):
                raise ValueError("Unclosed frame omission definition: " + rule)
            value = binding.values.get(occurrence.slot)
            if "owner" in definition:
                matches = value == definition["owner"]
            elif "amount" in definition:
                matches = (
                    type(value) is int
                    and type(definition["amount"]) is int
                    and value == definition["amount"]
                )
            else:
                matches = (
                    isinstance(value, ContextExpression)
                    and value.expression_id.startswith("aggregate.count.")
                    and definition.get("mode") == "all"
                    and definition.get("measure") == "selected_objects"
                )
            if not matches:
                raise ValueError(
                    "Source value contradicts omission definition: " + rule
                )
            result[rule] = definition
    return result


def _units(frame: Frame, binding: SourceBinding) -> dict[str, JsonValue]:
    slots = {slot.name: slot for slot in frame.leaf_schema.slots}
    result: dict[str, JsonValue] = {}
    for occurrence in binding.occurrences:
        if occurrence.source_unit is None:
            continue
        slot = slots[occurrence.slot]
        meaning = unit_meaning(occurrence.source_unit, slot.role)
        if slot.name in result and result[slot.name] != meaning:
            raise ValueError("Repeated source leaf has incompatible unit meanings")
        result[slot.name] = meaning
    return result


def _source_links(frame: Frame) -> JsonValue:
    # Only evidence-bearing ports express source relationships; adapter-only ports remain separate.
    ports = [
        p
        for p in (*frame.projection.imports, *frame.projection.exports)
        if p.evidence is not None
    ]
    ancestors = set()
    scopes = {s.id: s for s in frame.projection.scopes}
    for port in ports:
        scope: str | None = port.scope
        while scope is not None:
            if scope in ancestors:
                break
            ancestors.add(scope)
            scope = scopes[scope].parent
    return {
        "ports": [p.model_dump(mode="json") for p in ports],
        "scopes": [
            s.model_dump(mode="json")
            for s in frame.projection.scopes
            if s.id in ancestors
        ],
    }


def definitions(
    frame: Frame,
    binding: SourceBinding,
    domains: Mapping[str, ClosedDomain],
    registry: Registry,
) -> dict[str, JsonValue]:
    """Catalog members remain binding values; selectors and closed domains define the frame."""
    expressions, names = _expressions(frame, binding, registry)
    names.update(
        name
        for slot in frame.leaf_schema.slots
        for name in slot.domain.values
        if isinstance(name, str)
    )
    missing = names - domains.keys()
    if missing:
        raise ValueError("Unclosed frame semantic domain: " + ",".join(sorted(missing)))
    key = frame.semantic_variant.key
    if key is not None and key not in RESOLVED:
        raise ValueError("Unclosed resolved frame semantics: " + key)
    result: dict[str, JsonValue] = {
        "domains": {name: _domain(domains[name], name) for name in sorted(names)},
        "expressions": expressions,
        "omissions": _omissions(binding),
        "units": _units(frame, binding),
        "resolved": None if key is None else RESOLVED[key],
        "source_links": _source_links(frame),
    }
    return object_value(parse(canonical(result)))


def seal(
    frame: Frame,
    binding: SourceBinding,
    canonical_source: str,
    domains: Mapping[str, ClosedDomain],
    registry: Registry,
) -> tuple[Frame, SourceBinding]:
    """A release label records provenance; only a closed semantic contract addresses content."""
    source = frame.source.model_copy(
        update={"semantic_definitions": definitions(frame, binding, domains, registry)}
    )
    frame = frame.model_copy(update={"source": source})
    checksum = hash_payload(frame.payload(canonical_source))
    frame = frame.model_copy(
        update={"id": "frame:" + checksum, "content_hash": checksum}
    )
    binding = binding.model_copy(update={"frame_id": frame.id})
    binding = binding.model_copy(
        update={"id": "bind:" + hash_payload(binding.payload())}
    )
    return frame, binding


@cache
def lexical_catalog() -> tuple[Mapping[str, ClosedDomain], Registry]:
    """A lexical binding needs domain definitions, but never a fabricated catalog member."""
    from sve_carddb.domains.translations.four_layer_classification import (  # ruff: ignore[import-outside-top-level] -- lexical source construction calls this after classifier initialization
        Classifier,
        SourceReferences,
    )
    from sve_carddb.domains.translations.four_layer_context import DEFAULT_REGISTRY  # ruff: ignore[import-outside-top-level] -- families depend on source construction
    from sve_carddb.domains.translations.four_layer_derivation import source_families  # ruff: ignore[import-outside-top-level] -- families depend on source construction
    from sve_carddb.domains.translations.recognition.rules import Rules  # ruff: ignore[import-outside-top-level] -- source classification supplies the catalog shapes

    classifier = Classifier(
        (),
        SourceReferences(),
        Rules(format=2, kind="template_parameter_rules", rules=()),
    )
    registry = DEFAULT_REGISTRY.extend(
        tuple(d for family in source_families() for d in family.descriptors)
    )
    return classifier.domains, registry
