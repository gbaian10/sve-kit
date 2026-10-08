"""Reproducible differences without copying official effect text into reports."""

from difflib import SequenceMatcher
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import canonical, digest
from sve_carddb.products.official import date_fields

if TYPE_CHECKING:
    from sve_carddb.text_observations.models import FaceObservation


def value_summary(value: JsonValue) -> dict[str, JsonValue]:
    """Retain exact content fingerprints and null versus empty distinctions."""
    content = canonical(value)
    return {"sha256": digest(content), "bytes": len(content), "is_null": value is None}


def difference(before: JsonValue, after: JsonValue) -> dict[str, JsonValue]:
    """Expose both ends and edit offsets without leaking effect/name/type strings."""
    result: dict[str, JsonValue] = {
        "before": value_summary(before),
        "after": value_summary(after),
    }
    if isinstance(before, str) and isinstance(after, str):
        result["character_edits"] = [
            {"operation": tag, "before_range": [i, j], "after_range": [k, end]}
            for tag, i, j, k, end in SequenceMatcher(
                None, before, after, autojunk=False
            ).get_opcodes()
            if tag != "equal"
        ]
    elif isinstance(before, list) and isinstance(after, list):
        result["before_length"] = len(before)
        result["after_length"] = len(after)
        result["elements"] = [
            {"ordinal": index, "change": difference(old, new)}
            for index in range(max(len(before), len(after)))
            if (old := before[index] if index < len(before) else None)
            != (new := after[index] if index < len(after) else None)
        ]
    return result


def observation_report(item: FaceObservation) -> dict[str, JsonValue]:
    """Report physical identity, original source index and source-date precision."""
    date, precision = date_fields(item.card.date_raw)
    return {
        "card_id": item.card_id,
        "printing_id": item.printing_id,
        "face_id": item.face_id,
        "region": item.region,
        "card_no": item.card_no,
        "source_index": item.source_index,
        "source_id": item.card.source.id,
        "source_url": item.card.source.url,
        "raw_hash": item.card.source.sha256,
        "observed_at": item.card.source.fetched_at,
        "source_date": date,
        "source_date_precision": precision,
        "source_date_raw": item.card.date_raw,
        "has_errata_link": item.card.has_errata_link,
        "content_hash": item.content.fingerprint(),
        "raw_face_hash": item.card.faces[item.source_index].fingerprint(),
        "effect_presence": item.card.effect_presence[item.source_index].value()
        if item.card.effect_presence
        else None,
        "fields": {
            key: value_summary(value) for key, value in item.content.fields().items()
        },
        "stats_raw": list[JsonValue](item.content.stats),
        "missing_effect": item.content.effect is None,
        "possible_no_effect_follower": item.content.possible_no_effect(),
    }


def comparisons(items: tuple[FaceObservation, ...]) -> list[JsonValue]:
    """Compare distinct current-bearing variants; the baseline is not a selection."""
    representatives: dict[str, FaceObservation] = {}
    for item in items:
        representatives.setdefault(item.content.fingerprint(), item)
    ordered = [representatives[key] for key in sorted(representatives)]
    before = ordered[0]
    return [
        {
            "before_printing_id": before.printing_id,
            "before_source_id": before.card.source.id,
            "after_printing_id": after.printing_id,
            "after_source_id": after.card.source.id,
            "fields": {
                key: difference(value, after.content.fields()[key])
                for key, value in before.content.fields().items()
                if value != after.content.fields()[key]
            },
        }
        for after in ordered[1:]
    ]
