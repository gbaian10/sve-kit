"""Semantic validation over checked envelopes and independently rebuilt source content."""

import re
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.catalog.adoption_loader import ordered
from sve_carddb.catalog.models import Term
from sve_carddb.products.models import Language

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_models import RawMapping, ReviewContext
    from sve_carddb.catalog.adoption_sources import AdoptionSources
    from sve_carddb.catalog.current_models import LanguageRecord, VocabularyRecord

_FIELD_PARTS = 4
_UI = {"zh-Hant": ("ja", "en"), "ja": ("en",), "en": ("ja",)}


def language(record: LanguageRecord, registered: set[str]) -> Language | None:
    """Validate a complete UI fallback list against the approved policy."""
    value = record.data.value
    if value is None:
        return None
    code, fallback = record.data.subject.code, value.fallback_order
    if (
        code in fallback
        or len(fallback) != len(set(fallback))
        or set(fallback) - registered
    ):
        raise ValueError("Adopted language has invalid fallback closure")
    if code in {"ja", "en"} and "zh-Hant" in fallback:
        raise ValueError("Japanese/English UI cannot fall back to Traditional Chinese")
    if code in _UI and fallback != _UI[code]:
        raise ValueError("Adopted language violates approved UI fallback order")
    return Language(code=code, display_name=value.display_name, fallback_order=fallback)


def term(
    record: VocabularyRecord,
    review: ReviewContext,
    sources: AdoptionSources,
) -> Term | None:
    """Reconstruct a baseline label and exact source-field mappings."""
    value = record.data.value
    if value is None:
        return None
    ordered(value.raw_mappings)
    kind = record.data.subject.kind
    if kind == "special_kind" and record.data.subject.code not in {
        "evolve",
        "advance",
        "token",
    }:
        raise ValueError("Unsupported adopted special-kind code")
    if kind == "special_kind" and value.raw_mappings:
        raise ValueError("Special-kind definitions cannot declare raw mappings")
    for mapping in value.raw_mappings:
        _mapping_metadata(kind, mapping)
        if (
            kind == "trait"
            and re.fullmatch(
                r"/faces/(0|[1-9][0-9]*)/traits/(0|[1-9][0-9]*)",
                mapping.source_ref.locator,
            )
            is None
        ):
            raise ValueError("Vocabulary mapping source field does not match kind")
        text, _, projection = sources.text(mapping.source_ref, review)
        if (text.lang, text.text) != (mapping.lang, mapping.raw) or mapping.lang != (
            "ja" if mapping.region == "jp" else "en"
        ):
            raise ValueError("Vocabulary mapping exact raw/region/language mismatch")
        if kind == "trait":
            _trait_components(mapping, projection)
            continue
        expected = _mapping_locator(kind, mapping.region)
        parts = mapping.source_ref.locator.split("/")
        if (
            len(parts) < _FIELD_PARTS
            or parts[1] != "faces"
            or not parts[2].isdecimal()
            or "/".join(parts[3:]) not in expected
        ):
            raise ValueError("Vocabulary mapping source field does not match kind")
    return Term(
        kind=record.data.subject.kind,
        code=record.data.subject.code,
        label=sources.value(value.label, review),
        active=value.active,
    )


def _trait_components(mapping: RawMapping, projection: JsonValue) -> None:
    """Reject split or malformed components even when their exact hash is valid."""
    parts = mapping.source_ref.locator.split("/")
    faces = projection.get("faces") if isinstance(projection, dict) else None
    face_index = int(parts[2])
    if (
        not isinstance(faces, list)
        or face_index >= len(faces)
        or not isinstance(face := faces[face_index], dict)
    ):
        raise ValueError("Trait source projection lacks the mapped face")
    traits, raw = face.get("traits"), face.get("trait_raw")
    if not isinstance(traits, list):
        raise ValueError(  # ruff: ignore[type-check-without-type-error] -- recipe shape mismatches are domain refusals at this adoption boundary
            "Trait source components cannot reconstruct the exact raw field"
        )
    components = [part for part in traits if isinstance(part, str) and part]
    separator = "・" if mapping.region == "jp" else " / "
    if (
        not components
        or len(components) != len(traits)
        or separator.join(components) != raw
    ):
        raise ValueError(
            "Trait source components cannot reconstruct the exact raw field"
        )
    for component in components:
        bracketed = "〈" in component or "〉" in component
        if bracketed and not (
            component.startswith("〈")
            and component.endswith("〉")
            and component.count("〈") == 1
            and component.count("〉") == 1
        ):
            raise ValueError("Trait source component has incomplete enclosing brackets")
        if separator in component and (mapping.region == "en" or not bracketed):
            raise ValueError("Trait source component contains an unprotected separator")


def _mapping_metadata(kind: str, mapping: RawMapping) -> None:
    if kind != "type" and mapping.special_kinds:
        raise ValueError("Only type raw mappings can declare special kinds")
    if mapping.special_kinds != tuple(sorted(set(mapping.special_kinds))):
        raise ValueError("Adopted special kinds must be sorted and unique")
    if set(mapping.special_kinds) - {"evolve", "advance", "token"}:
        raise ValueError("Unsupported adopted type special kind")
    if kind == "class" and mapping.raw == "-":
        raise ValueError("Missing class value cannot be adopted as a code")
    if kind == "type" and mapping.raw == "-":
        raise ValueError("Missing type value cannot be adopted as a code")


def _mapping_locator(kind: str, region: str) -> set[str]:
    fields = (
        {
            "class": "card_class",
            "type": "card_type",
            "rarity": "rarity",
            "title": "title",
        }
        if region == "jp"
        else {
            "class": "info/Class",
            "type": "info/Card Type",
            "rarity": "info/Rarity",
            "title": "info/Universe",
        }
    )
    if kind not in fields:
        raise ValueError("Source-field recipe is not enabled for this vocabulary kind")
    return {fields[kind]}
