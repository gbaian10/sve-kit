"""Generate Rust's ephemeral card input and selection from one public workspace."""

import sys
from pathlib import Path

from sve_carddb.core.json import canonical, object_value, parse, string
from sve_carddb.domains.effects.author_current import english_only_cards
from sve_carddb.domains.parsed_cards.engine_fixture import English, project
from sve_carddb.domains.parsed_cards.reader import read
from sve_carddb.domains.registry.records import PrintingData
from sve_carddb.domains.registry.transitions.renewal import load_identity_registry
from sve_carddb.workflows import authoring
from tests.support.battle_projection import generate as generate_battle_projection

REPOSITORY = Path(__file__).resolve().parents[3]


def repository_inputs(repository: Path) -> authoring.Inputs:
    """Read the source pins shared by the Rust generator and the public profile tests."""
    pins = object_value(
        parse((repository / "tests/engine/card-input.json").read_bytes())
    )
    return authoring.Inputs(
        profile="public",
        repo=repository,
        jp_batch_id=string(pins["batch_id"]),
        en_batch_id=string(pins["en_batch_id"]) if "en_batch_id" in pins else None,
        revision="1036aa89bb1ce52cf4a964df8461663ab644b614",
    )


def _english_numbers() -> frozenset[str]:
    identity = load_identity_registry(
        REPOSITORY / "authored", repository=REPOSITORY, include_mapping_reviews=True
    )
    eligible = english_only_cards(identity)
    return frozenset(
        record.data.card_no
        for record in identity.records.values()
        if isinstance(record.data, PrintingData)
        and record.data.region == "en"
        and record.data.card_id in eligible
    )


def generate(cards: Path, selection: Path, battle_projection: Path) -> None:
    """Share admitted source data for both outputs; every call reads fresh inputs."""
    if not all(path.is_absolute() for path in (cards, selection, battle_projection)):
        raise ValueError("Repository inputs require absolute output paths")
    inputs = repository_inputs(REPOSITORY)
    parsed = read(REPOSITORY / "card-data")
    with authoring.prepare(inputs, parsed=parsed) as workspace:
        selected = canonical(authoring.check(workspace)["selection"])
        english = None
        if inputs.en_batch_id is not None:
            assert workspace.vocabulary is not None
            english = English(
                inputs.en_batch_id, _english_numbers(), workspace.vocabulary
            )
        card_input = project(parsed, inputs.jp_batch_id, english=english)
    with cards.open("xb") as output:
        output.write(card_input)
    with selection.open("xb") as output:
        output.write(selected)
    with battle_projection.open("xb") as output:
        output.write(generate_battle_projection())


if __name__ == "__main__":
    try:
        generate(
            Path(sys.argv[1]),
            Path(sys.argv[2]),
            Path(sys.argv[3]),
        )
    except OSError, ValueError:
        raise SystemExit("Engine test input generation failed.") from None
