"""Reuse the public correction-value contract for build-time before/after values."""

from pydantic import JsonValue

from sve_carddb.contracts.snapshot import definition


def schemas() -> dict[str, JsonValue]:
    """Bundle local references without changing the public field/value whitelist."""
    definitions: dict[str, JsonValue] = {
        name: definition(name) for name in ("Text", "Int", "ID", "CorrectionValue")
    }
    return {
        "correction_value": {"$defs": definitions, **definition("CorrectionValue")},
        "change_values": {"$defs": definitions, **definition("ErrataChange")},
    }
