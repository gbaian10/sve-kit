"""Generate Rust's ephemeral card input and selection from one public workspace."""

import sys
from pathlib import Path

from sve_carddb.core.json import canonical, object_value, parse, string
from sve_carddb.domains.parsed_cards.engine_fixture import project
from sve_carddb.domains.parsed_cards.reader import read
from sve_carddb.workflows import authoring

REPOSITORY = Path(__file__).resolve().parents[3]


def generate(cards: Path, selection: Path) -> None:
    """Share admitted source data for both outputs; every call reads fresh inputs."""
    if not cards.is_absolute() or not selection.is_absolute():
        raise ValueError("Repository inputs require absolute output paths")
    batch = string(
        object_value(parse((REPOSITORY / "tests/engine/card-input.json").read_bytes()))[
            "batch_id"
        ]
    )
    parsed = read(REPOSITORY / "card-data")
    card_input = project(parsed, batch)
    inputs = authoring.Inputs(
        profile="public",
        repo=REPOSITORY,
        jp_batch_id=batch,
        revision="1036aa89bb1ce52cf4a964df8461663ab644b614",
    )
    with authoring.prepare(inputs, parsed=parsed) as workspace:
        selected = canonical(authoring.check(workspace)["selection"])
    with cards.open("xb") as output:
        output.write(card_input)
    with selection.open("xb") as output:
        output.write(selected)


if __name__ == "__main__":
    try:
        generate(Path(sys.argv[1]), Path(sys.argv[2]))
    except OSError, ValueError:
        raise SystemExit("Engine test input generation failed.") from None
