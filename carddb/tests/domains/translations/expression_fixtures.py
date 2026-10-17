"""Assemble sentence-owned expression targets for source-family rendering cases."""

from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    ContextExpression,
    Expression,
    PredicateExpr,
    QuantitySpec,
)

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_expressions import ExpressionTargets
    from sve_carddb.domains.translations.four_layer_render import BoundTarget


def expression_plan(
    plan: tuple[BoundTarget, ...], targets: ExpressionTargets
) -> tuple[BoundTarget, ...]:
    """Fixture wording belongs to each selected sentence, not to the renderer."""
    return tuple(
        replace(
            item,
            selected=replace(
                item.selected,
                expression_targets={
                    identifier: selected.target
                    for (identifier, _lang), selected in targets.targets.items()
                    if identifier
                    in {
                        expr.expression_id
                        for value in item.binding.values.values()
                        if isinstance(
                            expr := value.expr
                            if isinstance(value, QuantitySpec)
                            else value,
                            (Expression, ContextExpression, PredicateExpr),
                        )
                    }
                },
            ),
        )
        if item.selected is not None
        else item
        for item in plan
    )
