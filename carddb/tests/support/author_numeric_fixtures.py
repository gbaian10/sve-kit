"""Real adopted fixed Fanfare buff frame and concepts, with public printing identities."""

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

FRAME = "frame:df3e447f70b99e27658e92037da38c02a10b8e4366cda137d93e4e75e77b2a49"


def numeric_fixture(
    store: Path,
    *,
    card_numbers: frozenset[str] = frozenset({"BP01-P09", "BP01-037"}),
    parser_version: str = "author-current-fixture-v1",
) -> CurrentSourceFixture:
    authored = REPOSITORY / "authored"
    definitions = four_layer_definitions.from_files(
        four_layer_definitions.read_files(authored)
    )
    files = []
    for name, identifiers in (
        (
            "translations/glossary/concepts/001.yaml",
            frozenset({"term:ability.fanfare"}),
        ),
        ("translations/templates/definitions/110.yaml", frozenset({FRAME})),
        (
            "translations/glossary/concepts/046.yaml",
            frozenset({"term:stat.attack", "term:stat.health"}),
        ),
    ):
        shard = Shard.model_validate_json(
            definitions.expand_shard(read(authored / name, root=authored)[1])
        )
        records: list[JsonValue] = [
            record.model_dump(mode="json", exclude_computed_fields=True)
            for record in shard.records
            if isinstance(record, FrameRecord | TermRecord)
            and record.data.id in identifiers
        ]
        assert len(records) == len(identifiers)
        raw = canonical({"format": 3, "kind": "translation_shard", "records": records})
        files.append((name, raw, raw))
    shards = tuple(
        file for file in files if file[0].startswith("translations/glossary/")
    )
    snapshot = Snapshot(
        shards,
        with_definitions(tuple(files)),
    )
    validate_snapshot(snapshot)
    return current_source_fixture(
        store,
        card_numbers=card_numbers,
        snapshot=snapshot,
        parser_version=parser_version,
    )
