"""Public Advance printings with their admitted Fanfare and nonbody frames."""

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


def destroy_fixture(
    store: Path, *, preparation: bytes | CurrentSourceInputs | None = None
) -> CurrentSourceFixture:
    authored = REPOSITORY / "authored"
    definitions = four_layer_definitions.from_files(
        four_layer_definitions.read_files(authored)
    )
    files = []
    for name in (
        "translations/glossary/concepts/001.yaml",
        "translations/glossary/concepts/004.yaml",
        "translations/templates/definitions/022.yaml",
        "translations/templates/definitions/068.yaml",
        "translations/templates/definitions/109.yaml",
    ):
        shard = Shard.model_validate_json(
            definitions.expand_shard(read(authored / name, root=authored)[1])
        )
        records: list[JsonValue] = [
            record.model_dump(mode="json", exclude_computed_fields=True)
            for record in shard.records
            if (
                isinstance(record, TermRecord)
                and record.data.id
                in {"term:ability.fanfare", "term:keyword.ward", "term:keyword.aura"}
            )
            or (
                isinstance(record, FrameRecord)
                and record.data.semantic_variant.key
                in {
                    "card_keywords.v2",
                    "metadata.layout.v1",
                    "destroy_fanfare_opponent_all_field_followers.v1",
                }
            )
        ]
        raw = canonical({"format": 3, "kind": "translation_shard", "records": records})
        files.append((name, raw, raw))
    snapshot = Snapshot(
        tuple(f for f in files if f[0].startswith("translations/glossary/")),
        with_definitions(tuple(files)),
    )
    validate_snapshot(snapshot)
    return current_source_fixture(
        store,
        card_numbers=frozenset({"BP19-P32", "BP19-114"}),
        snapshot=snapshot,
        preparation=preparation,
    )
