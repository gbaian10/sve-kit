"""Generate Rust's repository selection through the public authoring check."""

import sys
from pathlib import Path

from pydantic import JsonValue

from sve_carddb.core.json import canonical, object_value, parse, string
from sve_carddb.workflows import authoring

REPOSITORY = Path(__file__).resolve().parents[3]


def selection_input(root: Path) -> JsonValue:
    """Keep the existing runner adapter while sharing the production check."""
    if not root.is_absolute():
        raise ValueError("Repository selection requires an absolute output root")
    batch = string(
        object_value(parse((REPOSITORY / "tests/engine/card-input.json").read_bytes()))[
            "batch_id"
        ]
    )
    inputs = authoring.Inputs(
        profile="public",
        repo=REPOSITORY,
        jp_batch_id=batch,
        revision="1036aa89bb1ce52cf4a964df8461663ab644b614",
    )
    with authoring.prepare(inputs) as workspace:
        return authoring.check(workspace)["selection"]


if __name__ == "__main__":
    output = Path(sys.argv[1])
    output.write_bytes(canonical(selection_input(output.parent)))
