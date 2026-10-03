"""semantic-output-v1 hashes every observable source result without official wording."""

from dataclasses import asdict, dataclass
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_translations.flavor_models import FlavorCandidate
from sve_carddb.template_translations.replay_models import ExpectedOutputs

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.template_translations.sources import Reconstructed

STREAMS = ("entries", "fields", "members", "coverage", "checkpoint", "source_uses")


@dataclass(frozen=True)
class Output:
    streams: tuple[tuple[str, bytes], ...]
    manifest: ExpectedOutputs


def ordered(rows: list[JsonValue]) -> list[JsonValue]:
    """Only outer collection sets are sorted; all inner semantic sequences stay exact."""
    return sorted(rows, key=canonical)


def summarize(
    members: tuple[Reconstructed, ...],
    fields: list[JsonValue],
    coverage: JsonValue,
    checkpoint: JsonValue,
    uses: list[JsonValue],
) -> Output:
    """Even unadopted entries, missing fields and pending roles contribute to each root."""
    records: dict[str, list[JsonValue]] = {
        "entries": ordered([m.entry.model_dump(mode="json") for m in members]),
        "fields": ordered(fields),
        "members": ordered(
            [
                {
                    "entry_id": m.entry.id,
                    "normalized_hash": digest(m.normalized.encode()),
                    "field_hash": digest(m.field_text.encode()),
                    "candidate": (
                        {
                            "source_span": m.candidate.source_span.model_dump(
                                mode="json"
                            ),
                            "legacy_id": None,
                        }
                        if isinstance(m.candidate, FlavorCandidate)
                        else m.candidate.model_dump(mode="json")
                    ),
                    "hints": [h.model_dump(mode="json") for h in m.hints],
                    "roles": list(m.roles),
                    "pending": list(m.pending),
                    "owner": None if m.owner is None else asdict(m.owner),
                }
                for m in members
            ]
        ),
        "coverage": [coverage],
        "checkpoint": [checkpoint],
        "source_uses": ordered(uses),
    }
    streams = tuple((name, canonical(records[name])) for name in STREAMS)
    wire: dict[str, JsonValue] = {
        "format": 1,
        "recipe": "semantic-output-v1",
        "streams": {
            name: {"count": len(records[name]), "hash": digest(raw)}
            for name, raw in streams
        },
    }
    wire["root"] = digest(canonical(wire))
    return Output(streams, ExpectedOutputs.model_validate(wire))
