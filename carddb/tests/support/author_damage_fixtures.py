"""Real adopted Lastword frame and concept, with public printing identities."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.authored import read
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations import four_layer_definitions
from sve_carddb.domains.translations.four_layer_authored import FrameRecord, Shard
from sve_carddb.domains.translations.glossary.records import TermRecord
from sve_carddb.domains.translations.inputs import Snapshot, validate_snapshot

from .author_current_fixtures import (
    REPOSITORY,
    CurrentSourceFixture,
    current_source_fixture,
)
from .semantic_definition_fixtures import with_definitions

if TYPE_CHECKING:
    from pathlib import Path

FRAME = "frame:40c2ea2338dd3fd85116c967fd193c7d9d3b96ff2ddb0633de44bed2b62f7f5d"
CARD = "c:efb62b3ef53e576797625688d75164d9"
FACE = "f:90c74424f2435907a035f70ce37463d4"
HASH = "3aeefc3819be4412d9e8891136fd813bbb27ce55011343f51d38289d4ee1d196"


def damage_fixture(
    store: Path,
    *,
    card_numbers: frozenset[str] = frozenset({"BP01-P25", "BP01-121"}),
    parser_version: str = "author-current-fixture-v1",
) -> CurrentSourceFixture:
    authored = REPOSITORY / "authored"
    definitions = four_layer_definitions.from_files(
        four_layer_definitions.read_files(authored)
    )
    files = []
    for name, identifier in (
        ("translations/glossary/concepts/001.yaml", "term:ability.last_words"),
        ("translations/templates/definitions/029.yaml", FRAME),
    ):
        shard = Shard.model_validate_json(
            definitions.expand_shard(read(authored / name, root=authored)[1])
        )
        records: list[JsonValue] = [
            record.model_dump(mode="json", exclude_computed_fields=True)
            for record in shard.records
            if isinstance(record, FrameRecord | TermRecord)
            and record.data.id == identifier
        ]
        assert len(records) == 1
        raw = canonical({"format": 3, "kind": "translation_shard", "records": records})
        files.append((name, raw, raw))
    snapshot = Snapshot((files[0],), with_definitions(tuple(files)))
    validate_snapshot(snapshot)
    return current_source_fixture(
        store,
        card_numbers=card_numbers,
        snapshot=snapshot,
        parser_version=parser_version,
    )
