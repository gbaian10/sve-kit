"""Numeric bounds and quantity roles shared by N0 recognition and N1 derivation."""

SAFE_INTEGER = 9007199254740991
QUANTITY_ROLES = frozenset(
    {
        "selection_count",
        "choice_mode_count",
        "existence_count",
        "counter_amount",
        "repeat_count",
    }
)
