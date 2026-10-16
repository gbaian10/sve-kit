"""Finite N2 parameter forms admitted only by the preparation renderer."""

from sve_carddb.contracts.four_layer import FormDefinition, LabelPart, LiteralNode

_SIGNATURES = {
    "earth_rite.display": {
        "ability": ("Concept", "ability"),
        "amount": ("Nat", "earth_rite_amount"),
    },
    "dice.interval": {
        "lower": ("Nat", "dice_lower_bound"),
        "upper": ("Nat", "dice_upper_bound"),
    },
}


def validate_parameter_form(form: FormDefinition) -> None:
    """Every argument is rendered through its typed leaf, including necessary labels."""
    signature = _SIGNATURES.get(form.id)
    if signature is None or form.rule != form.id + ".v1":
        raise ValueError("Unknown preparation parameter form")
    if {a.name: (a.type, a.role) for a in form.signature} != signature or set(
        form.cases
    ) != {"default"}:
        raise ValueError(
            "Parameter form requires its exact typed signature and default case"
        )
    parts = form.cases["default"]
    if any(not isinstance(p, (LiteralNode, LabelPart)) for p in parts) or tuple(
        p.arg for p in parts if isinstance(p, LabelPart)
    ) != tuple(signature):
        raise ValueError("Parameter form must render each typed argument once in order")
