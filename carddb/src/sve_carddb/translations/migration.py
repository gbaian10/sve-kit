"""Reproducible extraction of legacy terminal values into editable current shards."""

from collections import defaultdict
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.registry.storage import MAX_BYTES
from sve_carddb.snapshot.values import canonical
from sve_carddb.translations.current_models import ChoiceRecord

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_loader import AdoptionSnapshot
    from sve_carddb.digital_name_policies.current_models import Override
    from sve_carddb.digital_name_policies.loader import LoadedPolicy
    from sve_carddb.translations.loader import Snapshot

AREAS = {
    "glossary_term": "glossary/concepts",
    "glossary_choice": "glossary/choices",
    "glossary_emphasis_choice": "glossary/emphasis",
    "vocabulary_choice": "glossary/vocabulary",
    "context_assignment": "overrides/names",
    "card_name_concept": "overrides/concepts",
}


def _chunks(
    area: str, rows: list[dict[str, JsonValue]], *, catalog: bool = False
) -> dict[str, bytes]:
    prefix = "catalog-adoptions" if catalog else "translations"
    field = "catalog_adoption_format" if catalog else "translation_authored_format"
    kind = "catalog_adoption_shard" if catalog else "translation_shard"
    ordered = sorted(rows, key=lambda row: str(row["record_key"]))
    result = {}
    for start in range(0, len(ordered), 24):
        path = f"{prefix}/{area}/{start // 24 + 1:03}.yaml"
        raw = canonical(
            {
                field: 2,
                "kind": kind,
                "records": list[JsonValue](ordered[start : start + 24]),
            }
        )
        if len(raw) >= MAX_BYTES:
            raise ValueError("Converted current shard exceeds size limit")
        result[path] = raw
    return result


def glossary(
    snapshot: Snapshot, low_terms: frozenset[str] = frozenset()
) -> dict[str, bytes]:
    """Keep effective values and permanent IDs, with caller-supplied real quality flags."""
    groups: dict[str, list[dict[str, JsonValue]]] = defaultdict(list)
    for original in snapshot.current_records():
        record = original
        if isinstance(record, ChoiceRecord) and record.data.term_id in low_terms:
            record = record.model_copy(update={"low_confidence": True})
        groups[AREAS[record.kind]].append(record.model_dump(mode="json"))
    result = {}
    for area, rows in sorted(groups.items()):
        result.update(_chunks(area, rows))
    return result


def catalog(snapshot: AdoptionSnapshot) -> dict[str, bytes]:
    """Vocabulary/languages only; unrelated catalog adoption entries remain untouched."""
    groups: dict[str, list[dict[str, JsonValue]]] = defaultdict(list)
    for record, _ in snapshot.effective():
        if record.kind not in {"vocabulary_adoption", "language_adoption"}:
            continue
        raw = record.model_dump(mode="json")
        data = record.data.model_dump(mode="json")
        value = data["value"]
        origin = "project"
        if record.kind == "vocabulary_adoption" and isinstance(value, dict):
            label = value.get("label")
            if isinstance(label, dict) and label.get("kind") == "source":
                origin = "official"
        row: dict[str, JsonValue] = {
            "record_key": canonical([record.kind, data["subject"]]).decode(),
            "kind": record.kind,
            "data": {
                "subject": data["subject"],
                "value": value,
                "evidence": raw["evidence"],
            },
            "origin": origin,
            "low_confidence": False,
            "note": "",
        }
        area = (
            "vocabulary/current"
            if record.kind == "vocabulary_adoption"
            else "languages/current"
        )
        groups[area].append(row)
    return {
        path: raw
        for area, rows in groups.items()
        for path, raw in _chunks(area, rows, catalog=True).items()
    }


def name_policy(loaded: LoadedPolicy, overrides: tuple[Override, ...] = ()) -> bytes:
    """Extract business conditions; neither receipts nor private event evidence survive."""
    from sve_carddb.digital_name_policies.current_models import Policy  # ruff: ignore[import-outside-top-level] -- migration only, outside the current evaluator dependency closure
    from sve_carddb.snapshot.values import object_value  # ruff: ignore[import-outside-top-level] -- operate on the closed legacy JSON model

    document = loaded.document()
    content = document.content
    minimum = object_value(content["target_minimum_check"])
    payload: dict[str, JsonValue] = {
        "digital_name_policy_format": 2,
        "kind": "digital_name_policy",
        "policy_id": document.policy_id,
        "purpose": "names",
        "content": {
            "scope": content["scope"],
            "game_priority": content["game_priority"],
            "target_minimum_check": {
                k: minimum[k]
                for k in ("kana_ranges", "whitespace_codepoints", "trim_or_normalize")
            },
            "excluded_names": [
                e.model_dump(mode="json") for e in loaded.excluded().entries
            ],
            "name_overrides": [o.model_dump(mode="json") for o in overrides],
        },
        "origin": "project",
        "low_confidence": False,
        "note": "",
    }
    checked = Policy.model_validate_json(canonical(payload))
    return canonical(checked.model_dump(mode="json"))
