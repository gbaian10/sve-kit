"""Reuse the public correction-value contract for build-time before/after values."""

from copy import deepcopy

from pydantic import JsonValue

from sve_carddb.contracts.snapshot import definition
from sve_carddb.core.json import array


def schemas() -> dict[str, JsonValue]:
    """Bundle local references without changing the public field/value whitelist."""
    definitions: dict[str, JsonValue] = {
        name: definition(name) for name in ("Text", "Int", "ID", "CorrectionValue")
    }
    change = deepcopy(definition("ErrataChange"))
    # The raw-value CHECK also serves source corrections; translations have their own table.
    change["prefixItems"] = array(change["prefixItems"])[:4]
    change["minItems"] = change["maxItems"] = 4
    return {
        "correction_value": {"$defs": definitions, **definition("CorrectionValue")},
        "change_values": {"$defs": definitions, **change},
    }
