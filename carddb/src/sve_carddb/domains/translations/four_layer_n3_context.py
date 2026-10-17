"""Closed #749 signatures; operation interfaces remain projection ports."""

from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    ChoiceContext,
    ContextExpression,
    Expression,
    GlossaryReference,
    QuantitySpec,
    VocabularyReference,
)
from sve_carddb.contracts.source_binding import QuantityDomain
from sve_carddb.domains.translations.four_layer_context import (
    Descriptor,
    HistorySignature,
    ImportSignature,
    Parameter,
    typed_import,
)

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding

_DIE = Parameter(
    "die",
    "Concept",
    "rule_term",
    "concept.rule_term.v1",
    (GlossaryReference(kind="glossary", key="term:object.die"),),
)
_ZONE_ARITY = 4

CHOICE_DOMAIN = "quantity.choice_mode_count.context.v1"
CHOICE_ZONE_EXPRESSION = "count.current_zone.owner.zone.card.trait.v1"
DOMAINS = {
    CHOICE_DOMAIN: QuantityDomain(
        type="QuantitySpec",
        modes=("up_to",),
        imports=(),
        expressions=(CHOICE_ZONE_EXPRESSION,),
        context_expressions=("read.parameter.chosen_x.v1",),
    )
}


def verify_context_choice(
    frame: Frame, binding: SourceBinding, name: str, quantity: QuantitySpec
) -> None:
    """Only the two sealed providers may supply an inline context mode count."""
    expr = quantity.expr
    if quantity.mode != "up_to" or not isinstance(
        expr, (Expression, ContextExpression)
    ):
        raise ValueError("Context choice requires an up_to typed expression")
    if isinstance(expr, Expression):
        if (
            expr.expression_id != CHOICE_ZONE_EXPRESSION
            or len(expr.leaves) != _ZONE_ARITY
        ):
            raise ValueError("Context choice requires the sealed current-zone provider")
        owner, zone, card, _ = tuple(binding.values.get(n) for n in expr.leaves)
        if (owner, zone, card) != (
            "self",
            ("battlefield",),
            GlossaryReference(kind="glossary", key="term:object.card"),
        ):
            raise ValueError("Context choice counts self battlefield trait cards")
    elif expr.expression_id != "read.parameter.chosen_x.v1":
        raise ValueError("Context choice cannot consume history, offset or arithmetic")
    choices = [
        p
        for p in frame.projection.imports
        if p.type == "ChoiceContext" and p.source_role == "choice_context"
    ]
    if len(choices) != 1:
        raise ValueError("Context choice requires exactly one source ChoiceContext")
    port = typed_import(
        frame,
        binding,
        choices[0].name,
        expr.scope if isinstance(expr, ContextExpression) else choices[0].scope,
        ImportSignature("choice", "ChoiceContext", "play_parameter"),
    )
    assert port.evidence is not None
    value = port.evidence.value
    spans = tuple(o.raw_spans for o in binding.occurrences if o.slot == name)
    if (
        not isinstance(value, ChoiceContext)
        or value.modes != ("mode_1", "mode_2", "mode_3")
        or (spans != (port.evidence.source.segments,))
    ):
        raise ValueError(
            "Context choice requires its exact intro and ordered three modes"
        )


DESCRIPTORS = (
    Descriptor(
        "history.dice.face.exists.v1",
        "predicate.history.current_turn.v1",
        "history_condition",
        "Bool",
        "truth",
        (
            Parameter(
                "owner", "Player", "history_owner", "player.relative.v1", ("self",)
            ),
            Parameter("face", "Nat", "dice_face", "", tuple(range(1, 7))),
            _DIE,
        ),
        (ImportSignature("actor", "PlayerRef", "current_turn"),),
        "U12",
        ("H.query", "H.window", "V.history"),
        history=HistorySignature("dice", "exists"),
    ),
    Descriptor(
        "predicate.dice.matches_declaration.v1",
        "predicate.dice.declaration_match.v1",
        "declaration_match",
        "Bool",
        "truth",
        (
            Parameter("lower", "Nat", "declaration_lower_bound", "", (1,)),
            Parameter("upper", "Nat", "declaration_upper_bound", "", (6,)),
            Parameter(
                "selection",
                "QuantitySpec",
                "selection_count",
                "quantity.selection_count.constant.v1",
            ),
            _DIE,
        ),
        (
            ImportSignature(
                "declared",
                "CapturedValue",
                "declaration",
                result_type="Nat",
                measure="declared_dice_face",
            ),
            ImportSignature("event", "EventRef", "at_event", event_kind="DiceRoll"),
            ImportSignature(
                "result",
                "CapturedValue",
                "receipt",
                result_type="Nat",
                measure="die_face",
            ),
        ),
        "U12",
        ("V.binding", "V.event-field", "X.predicate"),
    ),
    Descriptor(
        "read.result.destroyed_follower_by_player.v1",
        "quantity.read.context.v1",
        "read_value",
        "Nat",
        "card_count",
        (
            Parameter(
                "kind",
                "CardKind",
                "read_kind",
                "card_kind.selection.v1",
                (VocabularyReference(kind="vocabulary", key=("type", "follower")),),
            ),
        ),
        (
            ImportSignature(
                "receipt", "ReceiptId", "receipt", receipt_result="objects"
            ),
            ImportSignature("player", "PlayerRef", "receipt"),
        ),
        "U07",
        ("V.receipt-field", "X.aggregate"),
    ),
    Descriptor(
        "read.event.damage_amount.v1",
        "quantity.read.context.v1",
        "read_value",
        "Nat",
        "damage",
        (),
        (
            ImportSignature("subject", "ObjectSet", "at_event", "one", "event_subject"),
            ImportSignature(
                "event",
                "EventRef",
                "at_event",
                event_kind="Damage",
                subject_import="subject",
            ),
        ),
        "U14",
        ("V.event-field",),
    ),
    Descriptor(
        "condition.quantity.at_most.v1",
        "predicate.quantity.compare.v1",
        "quantity_condition",
        "Bool",
        "truth",
        (
            Parameter(
                "value",
                "QuantityExpr",
                "read_value",
                "",
                alternate_roles=(
                    "history_read_value",
                    "distinct_name_count",
                    "count_expression",
                ),
            ),
            Parameter("threshold", "Nat", "comparison_threshold", ""),
        ),
        (),
        "U13",
        ("X.predicate",),
    ),
)
