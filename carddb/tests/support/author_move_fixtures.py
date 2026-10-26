"""Adopted move frame and permanent token identity from public parsed cards."""

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
    CurrentSourceInputs,
    current_source_fixture,
)
from .semantic_definition_fixtures import with_definitions

if TYPE_CHECKING:
    from pathlib import Path

FRAME = "frame:3685e2f120d9a679e483ccccc28ef3f37d93bd9e0892b130807e55c5c2b461a9"
NUMBERS = ("BP07-T03", "BP12-T12", "BP17-T19", "PR-195", "SP01-T03")


def move_fixture(
    store: Path, *, preparation: bytes | CurrentSourceInputs | None = None
) -> CurrentSourceFixture:
    authored = REPOSITORY / "authored"
    definitions = four_layer_definitions.from_files(
        four_layer_definitions.read_files(authored)
    )
    files = []
    for name, identifier in (
        ("translations/glossary/concepts/001.yaml", "term:ability.activation"),
        ("translations/templates/definitions/056.yaml", FRAME),
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
        card_numbers=frozenset(NUMBERS),
        snapshot=snapshot,
        preparation=preparation,
    )
