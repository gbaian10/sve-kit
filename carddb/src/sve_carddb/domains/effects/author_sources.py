"""Reconstructed source candidates and conservative author operand compatibility."""

from dataclasses import dataclass, field
from operator import itemgetter
from typing import TYPE_CHECKING, Literal

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import ParamDecl
from sve_carddb.contracts.four_layer import (
    CardNameReference,
    Constant,
    Domain,
    Expression,
    FaceRevisionOwner,
    Frame,
    GlossaryReference,
    LeafSchema,
    LeafSlot,
    QuantitySpec,
    Span,
)
from sve_carddb.contracts.n0 import SAFE_INTEGER
from sve_carddb.contracts.source_binding import SourceBinding
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.translations.four_layer_damage import KEY as DAMAGE_KEY
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_destroy import KEY as DESTROY_KEY
from sve_carddb.domains.translations.four_layer_expressions import definition
from sve_carddb.domains.translations.four_layer_fixed_actions import MOVE, SEARCH

if TYPE_CHECKING:
    from collections.abc import Mapping

    from pydantic import JsonValue

    from sve_carddb.contracts.source_binding import ClosedDomain, SourceDescriptor
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_semantics import CardContext


PLAYER_VALUES = {"self": "self", "opponent": "opp"}
ZONE_VALUES = {
    "battlefield": "field",
    "hand": "hand",
    "deck": "deck",
    "evolve_deck": "evolve_deck",
    "graveyard": "cemetery",
    "ex": "ex",
}
# These descriptors retain every supported filter and operand in the sibling count.
OFFSET_DESCRIPTORS = {
    "count.current_zone.owner.zone.subtract_offset.v1": "minus",
    "count.current_zone.owner.zone.name.add_offset.v1": "plus",
}
SOURCE_SITES: dict[tuple[str, tuple[str | int, ...]], tuple[str, str, str, str]] = {
    ("A.act.draw", ("n",)): ("Nat", "Nat", "count", "value"),
    ("A.act.discard.exact", ("n",)): ("Nat", "Nat", "count", "value"),
    ("A.act.create", ("n",)): ("Nat", "Nat", "count", "value"),
    ("A.act.damage", ("n",)): ("Nat", "Nat", "damage_amount", "value"),
    ("A.act.life", ("delta",)): ("Nat", "Nat", "recovery_amount", "value"),
    ("A.effect.cost-mod", ("set",)): ("Nat", "Nat", "cost_value", "value"),
    ("A.act.draw", ("who",)): ("Player", "Player", "destination_owner", "value"),
    ("A.expr.count-objects", ("who",)): (
        "Player",
        "Player",
        "counted_owner",
        "value",
    ),
    ("A.expr.count-objects", ("zone",)): (
        "Zone",
        "ZoneSet",
        "counted_zone",
        "value",
    ),
    ("A.sel.zone-objects", ("who",)): ("Player", "Player", "source_owner", "value"),
    ("A.sel.zone-objects", ("zone",)): ("Zone", "ZoneSet", "source_zone", "value"),
    **dict.fromkeys(
        (
            ("A.expr.minus", ("minus", 1)),
            ("A.expr.plus", ("plus", 0)),
            ("A.expr.plus", ("plus", 1)),
        ),
        ("Int", "Nat", "arithmetic_offset", "value"),
    ),
    **{
        (owner, (path,)): ("Nat", "QuantitySpec", "selection_count", "constant")
        for owner, path in (
            ("A.act.choose.exact", "n"),
            ("A.act.search.exact", "n"),
            ("A.target.exact", "n"),
            ("A.act.choose.up-to", "upto"),
            ("A.act.search.up-to", "upto"),
            ("A.target.up-to", "upto"),
        )
    },
    **{
        (owner, ("to",)): ("Zone", "ZoneSet", "destination_zone", "value")
        for owner in (
            "A.act.create",
            "A.act.move.ref",
            "A.act.move.top",
            "A.act.search.exact",
            "A.act.search.up-to",
        )
    },
    **{
        (owner, ("name",)): ("CardName", "CardName", "name_filter", "value")
        for owner in ("A.sel.in-objects", "A.sel.zone-objects")
    },
    ("A.act.create", ("token",)): ("CardName", "CardName", "created_name", "value"),
    ("A.act.create", ("token", "*")): ("CardName", "CardName", "created_name", "value"),
}
BINARY_ARITY = 2
ENTRY_HEADS = {
    DAMAGE_KEY: ("lastword",),
    DESTROY_KEY: ("fanfare",),
    MOVE: ("activate",),
    SEARCH: ("lastword",),
    "draw_exact_spell.v1": ("spell",),
    "draw_exact_attack_time.v1": ("attack_time",),
    "draw_exact_evolve_time.v1": ("evolve_time",),
    "evolve_entry.v1": ("evolve",),
    "feed_entry.v1": ("feed",),
    "ride_entry.v1": ("ride",),
    "evolve_feed_entries.v1": ("evolve", "feed"),
    "evolve_ride_entries.v1": ("evolve", "ride"),
}
NO_TOKEN_METADATA = (
    "metadata.name.v1",
    "metadata.label.v1",
    "metadata.layout.v1",
    "pure_reminder.v1",
)

ADAPTER_RECIPE = digest(
    canonical(
        {
            "recipe": "author-source-adapter-v1",
            "players": dict(PLAYER_VALUES),
            "zones": dict(ZONE_VALUES),
            "offset_descriptors": dict(OFFSET_DESCRIPTORS),
            "names": "classifier-card-name-source-ja-v1",
            "other_operand_mappings": "unsupported",
            "scalar_sites": [
                {"owner": owner, "path": list(path), "signature": list(signature)}
                for (owner, path), signature in sorted(SOURCE_SITES.items())
            ],
            "ports": "empty-only;unsupported-source-ports",
            "entry_heads": {key: list(value) for key, value in ENTRY_HEADS.items()},
            "entry_payload": "exact-head.cost.pp;lastword-uniform-all-leaders-v1;spell.draw.self.n;attack_time.draw.self.n;evolve_time.draw.self.n;lastword.search.self.deck.shikigami.hand.exact.n;empty-ports;no-imports",
            "entry_operands": "Nat-cost_value-occurrence-order-or-single-Nat-count;QuantitySpec-selection_count-constant-exact-v3",
            "token_closure": "mapped-ability-heads-only-no-token-v2",
            "token_free_metadata": list(NO_TOKEN_METADATA),
            "current_owner": "current-jp-face-revision;card-membership;exact-face-container",
            "known_null_attributes": "spell.attack;spell.defense;evolve-follower.cost;rules-proven-absence-v2",
            "prototype": "all-jp-printing-identities;shared-overlap;no-execution",
            "inventory": "authored-root-complete-files-v1",
            "resolved_nonentry": "unsupported_source_projection",
            "destroy_payload": "fixed-advance-printed-fanfare;opponent-field-all-followers;no-params;exact-Concept-and-source-location-v1",
            "damage_operand": "fixed-lastword-concept;Nat-damage_amount-leaf_1;exact-body-v1",
            "draw_operand": "draw_exact_spell.v1;draw_exact_attack_time.v1;draw_exact_evolve_time.v1;required-Nat-count-value;single-source-leaf",
            "draw_payload": "complete-head-body-canonical-equality-v1",
            "move_payload": "fixed-activation-PP1;SourceHolder-current-generation;Put-card-owner-cemetery;exact-three-leaves;no-params;complete-head-body-v1",
            "search_operand": "fixed-lastword-self-deck-shikigami-hand;required-QuantitySpec-selection_count-leaf_4-constant-exact-v1",
            "search_payload": "complete-head-body-canonical-equality-v1",
        }
    )
)


class SourceError(ValueError):
    """Diagnostics expose reason codes without copying candidate source wording."""

    def __init__(self, reason: str) -> None:
        self.reason = reason
        super().__init__(reason)


@dataclass(frozen=True)
class SourceCandidate:
    raw: str = field(repr=False)
    descriptor: SourceDescriptor
    classifier: Classifier = field(repr=False)
    context: CardContext | None = None


@dataclass(frozen=True)
class SourceContext:
    frame: Frame
    binding: SourceBinding
    canonical_source: str = field(repr=False)
    classifier: Classifier | None = field(default=None, repr=False)

    source_proof: Literal["none", "derived_self_consistent"] = field(
        default="none", init=False
    )


def entry_heads(frame: Frame) -> tuple[str, ...]:
    """Only complete registered ability projections have a proven author head."""
    variant = frame.semantic_variant
    if variant.state != "resolved" or variant.key is None:
        raise SourceError("source_pending")
    projection = frame.projection
    heads = ENTRY_HEADS.get(variant.key)
    if heads is None or projection.discriminator != variant.key:
        raise SourceError("unsupported_source_ability_mapping")
    if frame.role != "body" or projection.projection_kind != "ability_body":
        raise SourceError("source_not_ability_body")
    _ports(frame)
    expected = tuple(
        (f"ability_{index}", "ability", None) for index in range(len(heads))
    )
    if (
        tuple((scope.id, scope.kind, scope.parent) for scope in projection.scopes)
        != expected
    ):
        raise SourceError("source_ability_scope_mismatch")
    return heads


def entry_payload(source: SourceContext, scope: str) -> dict[str, JsonValue]:
    """Use source occurrence order, including repeated values, instead of value lookup."""
    heads = entry_heads(source.frame)
    scopes = tuple(f"ability_{index}" for index in range(len(heads)))
    if scope not in scopes:
        raise SourceError("source_ability_scope_mismatch")
    if source.frame.semantic_variant.key == DESTROY_KEY:
        _destroy_contract(source)
        return {
            "fanfare": {
                "do": [
                    {
                        "destroy": {
                            "ref": {"who": "opp", "zone": "field", "kind": "follower"}
                        }
                    }
                ]
            },
            "ports": {"imports": {}, "exports": {}},
        }
    if source.frame.semantic_variant.key == MOVE:
        _move_contract(source)
        return {
            "activate": {
                "zone": "field",
                "cost": {"pp": 1},
                "do": [{"move": {"ref": "this", "to": "cemetery"}}],
            },
            "ports": {"imports": {}, "exports": {}},
        }

    if source.frame.semantic_variant.key == SEARCH:
        return {
            "lastword": {
                "do": [
                    {
                        "search": {
                            "sel": {
                                "who": "self",
                                "zone": "deck",
                                "name": "式神の使役",
                            },
                            "n": _search_count(source),
                            "to": "hand",
                        }
                    }
                ]
            },
            "ports": {"imports": {}, "exports": {}},
        }
    if source.frame.semantic_variant.key == DAMAGE_KEY:
        return {
            "lastword": {
                "do": [
                    {
                        "damage": {
                            "to": {"leaders": "all"},
                            "n": _lastword_amount(source),
                        }
                    }
                ]
            },
            "ports": {"imports": {}, "exports": {}},
        }
    if source.frame.semantic_variant.key in {
        "draw_exact_spell.v1",
        "draw_exact_attack_time.v1",
        "draw_exact_evolve_time.v1",
    }:
        return {
            heads[0]: {"do": [{"draw": {"who": "self", "n": _draw_count(source)}}]},
            "ports": {"imports": {}, "exports": {}},
        }
    return _entry_cost_payload(source, heads, scopes.index(scope))


def _entry_cost_payload(
    source: SourceContext, heads: tuple[str, ...], index: int
) -> dict[str, JsonValue]:
    slots = {slot.name: slot for slot in source.frame.leaf_schema.slots}
    operands = []
    for occurrence in source.binding.occurrences:
        slot = slots[occurrence.slot]
        if slot.role != "cost_value":
            continue
        if slot.type != "Nat" or len(occurrence.canonical_spans) != 1:
            raise SourceError("unsupported_source_cost_operand")
        value = source.binding.values[slot.name]
        if type(value) is not int or not 0 <= value <= SAFE_INTEGER:
            raise SourceError("unsupported_source_cost_operand")
        operands.append((occurrence.canonical_spans[0].start, value))
    operands.sort()
    if len(operands) != len(heads) or len(
        {position for position, _ in operands}
    ) != len(heads):
        raise SourceError("source_cost_operand_mismatch")
    return {
        heads[index]: {"cost": {"pp": operands[index][1]}},
        "ports": {"imports": {}, "exports": {}},
    }


def _destroy_contract(source: SourceContext) -> None:
    expected = LeafSchema(
        format=2,
        slots=(
            LeafSlot(
                name="leaf_0",
                type="Concept",
                role="ability",
                required=True,
                domain=Domain(min=None, max=None, values=("concept.ability.v1",)),
                occurrences=(Span(start=1, end=8),),
            ),
        ),
    )
    if (
        source.canonical_source
        != "{ファンファーレ}相手の場のフォロワーすべてを破壊する。"
        or source.frame.leaf_schema != expected
        or source.binding.frame_id != source.frame.id
    ):
        raise SourceError("source_destroy_contract_mismatch")
    if (
        (source.binding.source.field, source.binding.source.ordinal) != ("effect", None)
        or source.binding.source.owner.kind != "face_revision"
        or source.binding.line_ordinal != 1
        or source.binding.source_span.role != "body"
        or source.binding.source_span.segments != (Span(start=10, end=38),)
    ):
        raise SourceError("source_destroy_contract_mismatch")
    if source.binding.values != {
        "leaf_0": GlossaryReference(kind="glossary", key="term:ability.fanfare")
    } or tuple(
        (o.slot, o.canonical_spans) for o in source.binding.occurrences
    ) != tuple((s.name, s.occurrences) for s in expected.slots):
        raise SourceError("source_destroy_contract_mismatch")


def _move_contract(source: SourceContext) -> None:
    # All three leaves are fixed; neither PP nor destination is a parameter here.
    expected = LeafSchema(
        format=2,
        slots=(
            LeafSlot(
                name="leaf_0",
                type="Concept",
                role="ability",
                required=True,
                domain=Domain(min=None, max=None, values=("concept.ability.v1",)),
                occurrences=(Span(start=1, end=3),),
            ),
            LeafSlot(
                name="leaf_1",
                type="Nat",
                role="cost_value",
                required=True,
                domain=Domain(min=0, max=SAFE_INTEGER, values=()),
                occurrences=(Span(start=8, end=9),),
            ),
            LeafSlot(
                name="leaf_2",
                type="ZoneSet",
                role="destination_zone",
                required=True,
                domain=Domain(min=None, max=None, values=("zone.single.v1",)),
                occurrences=(Span(start=14, end=22),),
            ),
        ),
    )
    occurrence = source.binding.occurrence_key()
    if (
        source.canonical_source != "{起動}{コストN}:これを{leaf_2}に置く。"
        or source.frame.leaf_schema != expected
        or not isinstance(occurrence.owner, FaceRevisionOwner)
        or occurrence.field != "effect"
        or occurrence.ordinal is not None
    ):
        raise SourceError("source_move_contract_mismatch")
    if (
        occurrence.line_ordinal != 1
        or occurrence.role != "body"
        or occurrence.segments != (Span(start=29, end=49),)
        or source.binding.frame_id != source.frame.id
    ):
        raise SourceError("source_move_contract_mismatch")
    if (
        source.binding.values
        != {
            "leaf_0": GlossaryReference(kind="glossary", key="term:ability.activation"),
            "leaf_1": 1,
            "leaf_2": ("graveyard",),
        }
        or type(source.binding.values.get("leaf_1")) is not int
        or tuple((o.slot, o.canonical_spans) for o in source.binding.occurrences)
        != tuple((s.name, s.occurrences) for s in expected.slots)
    ):
        raise SourceError("source_move_contract_mismatch")


def _search_count(source: SourceContext) -> int:
    expected = LeafSchema.model_validate(
        {
            "format": 2,
            "slots": tuple(
                {
                    "name": f"leaf_{index}",
                    "type": kind,
                    "role": role,
                    "domain": {"min": None, "max": None, "values": (domain,)},
                    "required": start != end,
                    "occurrences": ()
                    if start == end
                    else (Span(start=start, end=end),),
                }
                for index, (kind, role, domain, start, end) in enumerate(
                    (
                        ("Concept", "ability", "concept.ability.v1", 1, 7),
                        ("Player", "source_owner", "player.relative.v1", 8, 16),
                        ("ZoneSet", "source_zone", "zone.single.v1", 17, 25),
                        ("CardName", "name_reference", "card_name.any.v1", 28, 29),
                        (
                            "QuantitySpec",
                            "selection_count",
                            "quantity.selection_count.constant.v1",
                            30,
                            31,
                        ),
                        ("ZoneSet", "destination_zone", "zone.hand.v1", 36, 44),
                        ("Player", "destination_owner", "player.self.v1", 0, 0),
                    )
                )
            ),
        }
    )
    binding = source.binding
    if (
        source.canonical_source
        != "{ラストワード}{leaf_1}の{leaf_2}から『X』N枚を探し、{leaf_5}に加える。"
        or source.frame.leaf_schema != expected
    ):
        raise SourceError("source_search_contract_mismatch")
    if (
        not isinstance(binding.source.owner, FaceRevisionOwner)
        or binding.source.field != "effect"
        or binding.source.ordinal is not None
        or binding.line_ordinal != 0
    ):
        raise SourceError("source_search_contract_mismatch")
    if (
        binding.source_span.role != "body"
        or binding.frame_id != source.frame.id
        or tuple(
            sorted(
                ((o.slot, o.canonical_spans) for o in binding.occurrences),
                key=itemgetter(0),
            )
        )
        != tuple(sorted((s.name, s.occurrences) for s in expected.slots))
        or set(binding.values) != {s.name for s in expected.slots}
        or {key: value for key, value in binding.values.items() if key != "leaf_4"}
        != {
            "leaf_0": GlossaryReference(kind="glossary", key="term:ability.last_words"),
            "leaf_1": "self",
            "leaf_2": ("deck",),
            "leaf_3": CardNameReference(
                kind="card_name", term_id="term:name.shikigami_employment"
            ),
            "leaf_5": ("hand",),
            "leaf_6": "self",
        }
    ):
        raise SourceError("source_search_contract_mismatch")
    if _name(source, binding.values["leaf_3"]) != "式神の使役":
        raise SourceError("source_search_contract_mismatch")
    count = binding.values["leaf_4"]
    if (
        not isinstance(count, QuantitySpec)
        or count.mode != "exact"
        or not isinstance(count.expr, Constant)
        or type(count.expr.value) is not int
        or not 0 <= count.expr.value <= SAFE_INTEGER
    ):
        raise SourceError("source_search_operand_mismatch")
    return count.expr.value


def _lastword_amount(source: SourceContext) -> int:
    # The ability Concept is a fixed printed head, never a generic parameter.
    expected = LeafSchema(
        format=2,
        slots=(
            LeafSlot(
                name="leaf_0",
                type="Concept",
                role="ability",
                required=True,
                domain=Domain(min=None, max=None, values=("concept.ability.v1",)),
                occurrences=(Span(start=1, end=7),),
            ),
            LeafSlot(
                name="leaf_1",
                type="Nat",
                role="damage_amount",
                required=True,
                domain=Domain(min=0, max=SAFE_INTEGER, values=()),
                occurrences=(Span(start=16, end=17),),
            ),
        ),
    )
    if (
        source.canonical_source != "{ラストワード}リーダーすべてにNダメージ。"
        or source.frame.leaf_schema != expected
        or source.binding.source.field != "effect"
        or source.binding.source.ordinal is not None
        or source.binding.occurrence_key().role != "body"
    ):
        raise SourceError("source_lastword_contract_mismatch")
    if (
        set(source.binding.values) != {"leaf_0", "leaf_1"}
        or source.binding.frame_id != source.frame.id
        or source.binding.values["leaf_0"]
        != GlossaryReference(kind="glossary", key="term:ability.last_words")
        or tuple((o.slot, o.canonical_spans) for o in source.binding.occurrences)
        != tuple((s.name, s.occurrences) for s in expected.slots)
    ):
        raise SourceError("source_lastword_contract_mismatch")
    amount = source.binding.values["leaf_1"]
    if type(amount) is not int or not 0 <= amount <= SAFE_INTEGER:
        raise SourceError("source_damage_operand_mismatch")
    return amount


def _draw_count(source: SourceContext) -> int:
    slots = source.frame.leaf_schema.slots
    if len(slots) != 1:
        raise SourceError("source_draw_operand_mismatch")
    slot = slots[0]
    if slot.type != "Nat" or slot.role != "count" or not slot.required:
        raise SourceError("unsupported_source_draw_operand")
    if (
        set(source.binding.values) != {slot.name}
        or len(source.binding.occurrences) != 1
        or source.binding.occurrences[0].slot != slot.name
        or len(source.binding.occurrences[0].canonical_spans) != 1
    ):
        raise SourceError("source_draw_operand_mismatch")
    value = source.binding.values[slot.name]
    if type(value) is not int or not 0 <= value <= SAFE_INTEGER:
        raise SourceError("unsupported_source_draw_operand")
    return value


def _structure(frame: Frame, binding: SourceBinding) -> None:
    if frame.semantic_variant.state != "resolved":
        raise SourceError("source_pending")
    if frame.role != "body" or frame.projection.projection_kind != "ability_body":
        raise SourceError("source_not_ability_body")
    if not any(
        scope.kind == "ability" and scope.parent is None
        for scope in frame.projection.scopes
    ):
        raise SourceError("source_ability_scope_missing")
    if binding.frame_id != frame.id:
        raise SourceError("source_frame_mismatch")
    _ports(frame)


def _ports(frame: Frame) -> None:
    if frame.projection.imports or frame.projection.exports:
        raise SourceError("unsupported_source_ports")


def check_source_compatibility(
    frame: Frame,
    binding: SourceBinding,
    canonical_source: str,
    domains: Mapping[str, ClosedDomain],
    *,
    classifier: Classifier | None = None,
) -> SourceContext:
    """Self-consistent synthetic contracts never acquire reconstructed-source proof."""
    try:
        frame = Frame.model_validate(frame.model_dump(mode="python"))
        binding = SourceBinding.model_validate(binding.model_dump(mode="python"))
        frame.verify(canonical_source)
        binding.verify(frame, domains)
    except ValueError:
        raise SourceError("source_compatibility_mismatch") from None
    _structure(frame, binding)
    return SourceContext(frame, binding, canonical_source, classifier)


def resolve_source(candidate: SourceCandidate, use: SourceUse) -> SourceContext:
    """Rebuild candidate evidence; registry currentness remains a separate boundary."""
    try:
        use = SourceUse.model_validate(use.model_dump(mode="python"))
        derived = derive_source(
            candidate.raw,
            candidate.descriptor,
            candidate.classifier,
            context=candidate.context,
        )
    except ValueError:
        raise SourceError("source_reconstruction_failed") from None
    indices = [
        index
        for index, part in enumerate(derived.field.parts)
        if part.line_ordinal == use.occurrence.line_ordinal
        and part.source_span.role == use.occurrence.role
        and part.source_span.segments == use.occurrence.segments
    ]
    if len(indices) != 1:
        raise SourceError("source_occurrence_mismatch")
    index = indices[0]
    if derived.recognized[index].issues:
        raise SourceError("source_unresolved_issue")
    if derived.pending_causes[index]:
        raise SourceError("source_pending")
    try:
        frame, binding = derived.bind(index)
        derived.verify_binding(candidate.raw, candidate.classifier, frame, binding)
    except ValueError:
        raise SourceError("source_reconstruction_failed") from None
    _structure(frame, binding)
    if binding.id != use.binding_id or binding.occurrence_key() != use.occurrence:
        raise SourceError("source_occurrence_mismatch")
    if not any(
        scope.id == use.ability_scope
        and scope.kind == "ability"
        and scope.parent is None
        for scope in frame.projection.scopes
    ):
        raise SourceError("source_ability_scope_mismatch")
    result = SourceContext(
        frame,
        binding,
        derived.field.parts[index].canonical_source,
        candidate.classifier,
    )
    object.__setattr__(result, "source_proof", "derived_self_consistent")  # ruff: ignore[unnecessary-dunder-call] -- only reconstruction sets frozen proof; dataclass replacement must reset it
    return result


def _slot(source: SourceContext, declaration: ParamDecl) -> LeafSlot:
    slots = {slot.name: slot for slot in source.frame.leaf_schema.slots}
    name = declaration.source.slot
    if name not in slots or name not in source.binding.values:
        raise SourceError("parameter_source_slot_missing")
    return slots[name]


def _name(source: SourceContext, value: object) -> str:
    if not isinstance(value, CardNameReference) or source.classifier is None:
        raise SourceError("unsupported_source_construct")
    term = source.classifier.terms.get(value.term_id)
    if term is None or term.category != "card_name":
        raise SourceError("parameter_source_name_mismatch")
    return term.source_ja


def _zone(value: object) -> str:
    if (
        not isinstance(value, tuple)
        or len(value) != 1
        or not isinstance(value[0], str)
        or value[0] not in ZONE_VALUES
    ):
        raise SourceError("unsupported_source_construct")
    return ZONE_VALUES[value[0]]


def _value(source: SourceContext, declaration: ParamDecl, slot: LeafSlot) -> int | str:
    value: object = source.binding.values[slot.name]
    component = declaration.source.component
    if component == "negated_magnitude" or declaration.type in {
        "Concept",
        "CardKind",
        "Phase",
    }:
        raise SourceError("unsupported_source_construct")
    if declaration.type in {"Int", "Nat"}:
        if component == "constant" and isinstance(value, QuantitySpec):
            value = value.expr
        if component == "constant" and isinstance(value, Constant):
            value = value.value
        if type(value) is not int or not 0 <= value <= SAFE_INTEGER:
            raise SourceError("parameter_source_value_mismatch")
        return value
    if component != "value":
        raise SourceError("parameter_source_component_mismatch")
    if declaration.type == "CardName":
        return _name(source, value)
    if declaration.type == "Zone":
        return _zone(value)
    if (
        declaration.type == "Player"
        and isinstance(value, str)
        and value in PLAYER_VALUES
    ):
        return PLAYER_VALUES[value]
    raise SourceError("unsupported_source_construct")


def _site(owner: str, path: tuple[str | int, ...]) -> tuple[str, str, str, str] | None:
    if owner == "A.act.create" and _token_path(path) and len(path) == BINARY_ARITY:
        path = ("token", "*")
    return SOURCE_SITES.get((owner, path))


def _token_path(path: tuple[str | int, ...]) -> bool:
    match path:
        case ("token",):
            return True
        case ("token", int(index)) if type(index) is int and index >= 0:
            return True
        case _:
            return False


def _offset_arguments(
    source: SourceContext, slot: LeafSlot
) -> tuple[str, dict[str, LeafSlot]]:
    matches = []
    for value in source.binding.values.values():
        expression = value.expr if isinstance(value, QuantitySpec) else value
        if not isinstance(expression, Expression) or slot.name not in expression.leaves:
            continue
        try:
            descriptor = definition(expression)
            arguments = descriptor.arguments(expression, source.frame.leaf_schema)
        except ValueError:
            raise SourceError("parameter_source_operand_mismatch") from None
        if arguments.get("offset") == slot:
            matches.append((descriptor, arguments))
    if len(matches) != 1:
        raise SourceError("parameter_source_operand_mismatch")
    descriptor, arguments = matches[0]
    operator = OFFSET_DESCRIPTORS.get(descriptor.id)
    if operator is None:
        raise SourceError("unsupported_source_construct")
    return operator, arguments


def _offset(
    source: SourceContext,
    slot: LeafSlot,
    owner: str,
    path: tuple[str | int, ...],
    fragment: JsonValue,
) -> None:
    operator, arguments = _offset_arguments(source, slot)
    if owner != "A.expr." + operator or path[0] != operator:
        raise SourceError("parameter_source_operand_mismatch")
    if not isinstance(fragment, dict) or set(fragment) != {operator}:
        raise SourceError("parameter_source_operand_mismatch")
    operands = fragment[operator]
    if (
        not isinstance(operands, list)
        or len(operands) != BINARY_ARITY
        or type(path[1]) is not int
    ):
        raise SourceError("parameter_source_operand_mismatch")
    target = operands[path[1]]
    if (
        not isinstance(target, dict)
        or set(target) != {"param"}
        or not isinstance(target["param"], str)
    ):
        raise SourceError("parameter_source_operand_mismatch")
    values = source.binding.values
    who = values[arguments["owner"].name]
    if not isinstance(who, str) or who not in PLAYER_VALUES:
        raise SourceError("unsupported_source_construct")
    count: dict[str, JsonValue] = {
        "who": PLAYER_VALUES[who],
        "zone": _zone(values[arguments["zone"].name]),
    }
    if "name" in arguments:
        count["name"] = _name(source, values[arguments["name"].name])
    if operands[1 - path[1]] != {"count": count}:
        raise SourceError("parameter_source_operand_mismatch")


def parameter_expected(
    source: SourceContext,
    declaration: ParamDecl,
    owner: str,
    relative_path: tuple[str | int, ...],
    fragment: JsonValue,
) -> int | str:
    """A matching scalar is insufficient without a registered operand construction."""
    if source.frame.semantic_variant.key == MOVE:
        raise SourceError("unsupported_source_construct")
    try:
        declaration = ParamDecl.model_validate(declaration.model_dump(mode="python"))
    except ValueError:
        raise SourceError("parameter_source_declaration_invalid") from None
    slot = _slot(source, declaration)
    if declaration.source.component == "negated_magnitude" or declaration.type in {
        "Concept",
        "CardKind",
        "Phase",
    }:
        raise SourceError("unsupported_source_construct")
    site = _site(owner, relative_path)
    if site is None or site != (
        declaration.type,
        slot.type,
        slot.role,
        declaration.source.component,
    ):
        raise SourceError("parameter_source_role_mismatch")
    value = source.binding.values[slot.name]
    if isinstance(value, QuantitySpec) and value.mode != (
        "exact" if relative_path == ("n",) else "up_to"
    ):
        raise SourceError("parameter_source_mode_mismatch")
    expected = _value(source, declaration, slot)
    if source.frame.semantic_variant.key in {DAMAGE_KEY, SEARCH}:
        entry_payload(source, "ability_0")
        operand = (
            ("leaf_1", "A.act.damage", ("n",))
            if source.frame.semantic_variant.key == DAMAGE_KEY
            else ("leaf_4", "A.act.search.exact", ("n",))
        )
        if (declaration.source.slot, owner, relative_path) != operand:
            raise SourceError("parameter_source_operand_mismatch")
        return expected
    if (
        source.frame.semantic_variant.key
        in {
            "draw_exact_spell.v1",
            "draw_exact_attack_time.v1",
            "draw_exact_evolve_time.v1",
        }
        and owner == "A.act.draw"
        and relative_path == ("n",)
    ):
        entry_payload(source, "ability_0")
        return expected
    if slot.role != "arithmetic_offset":
        raise SourceError("unsupported_source_construct")
    _offset(source, slot, owner, relative_path, fragment)
    return expected
