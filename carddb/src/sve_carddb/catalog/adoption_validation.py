"""Semantic validation over checked envelopes and independently rebuilt source content."""

import unicodedata
from typing import TYPE_CHECKING

from sve_carddb.catalog.adoption_loader import ordered
from sve_carddb.catalog.adoption_models import (
    AliasRecord,
    AuthoredText,
    DefaultRecord,
    ImageEvidence,
    LanguageRecord,
    NameRecord,
    RouteRecord,
    SymbolRecord,
    TextEvidence,
    VocabularyRecord,
)
from sve_carddb.catalog.models import Alias, NameBinding, Term
from sve_carddb.catalog.symbols import Localization, Symbol
from sve_carddb.products.models import Language
from sve_carddb.registry.records import PrintingData
from sve_carddb.routes.codec import card_path, folded_key
from sve_carddb.snapshot.values import canonical, digest, parse

if TYPE_CHECKING:
    from sve_carddb.build_db import Database
    from sve_carddb.catalog.adoption_models import Record, ReviewContext
    from sve_carddb.catalog.adoption_sources import AdoptionSources
    from sve_carddb.products.models import LocalizedText
    from sve_carddb.text_observations.models import FaceObservation
    from sve_carddb.text_observations.plan import TextPlan

_FIELD_PARTS = 4
_MIN_VARIANTS = 2
_UI = {"zh-Hant": ("ja", "en"), "ja": ("en",), "en": ("ja",)}
_ALIAS_TABLE = {
    "keyword": "keyword",
    "stamp": "stamp",
    "card": "card",
    "product_family": "product_family",
}


def target(kind: str, code: str) -> tuple[str, dict[str, str]]:
    """Keep keyword/stamp targets distinct from reserved vocabulary kinds."""
    table = _ALIAS_TABLE.get(kind)
    if table is None:
        if kind not in {
            "class",
            "type",
            "rarity",
            "trait",
            "title",
            "frame",
            "stamp_series",
        }:
            raise ValueError("Unsupported catalog alias target kind")
        return "vocabulary", {"kind": kind, "code": code}
    return table, {"id": code}


def direct_dependencies(  # ruff: ignore[complex-structure] -- exhaustive direct-reference closure for the seven supported wire kinds
    record: Record, *, source_lang: str | None = None
) -> set[bytes]:
    """Declare the complete direct stable-reference closure from the typed value."""
    result: list[tuple[str, dict[str, str]]] = []
    data = record.data
    value = data.value
    if value is None:
        return set()
    if isinstance(record, VocabularyRecord):
        vocab = record.data.value
        assert vocab is not None
        if isinstance(vocab.label, AuthoredText):
            result.append(("language", {"code": vocab.label.lang}))
        elif source_lang is not None:
            result.append(("language", {"code": source_lang}))
        result.extend(("language", {"code": m.lang}) for m in vocab.raw_mappings)
    elif isinstance(record, LanguageRecord):
        lang_value = record.data.value
        assert lang_value is not None
        result.extend(
            ("language", {"code": lang}) for lang in lang_value.fallback_order
        )
    elif isinstance(record, AliasRecord):
        s = record.data.subject
        result.extend((target(s.kind, s.code), ("language", {"code": s.lang})))
    elif isinstance(record, SymbolRecord):
        symbol_value = record.data.value
        assert symbol_value is not None
        result.extend(
            ("language", {"code": item.lang}) for item in symbol_value.spellings
        )
        result.append(("language", {"code": symbol_value.source_localization.lang}))
        if symbol_value.keyword_id is not None:
            result.append(("keyword", {"id": symbol_value.keyword_id}))
    elif isinstance(record, NameRecord):
        name_value = record.data.value
        assert name_value is not None
        result.extend(
            (
                ("face", {"id": name_value.identity_ref.face_id}),
                ("card", {"id": name_value.identity_ref.card_id}),
            )
        )
    elif isinstance(record, RouteRecord):
        route = record.data.value
        assert route is not None
        result.append(("printing", {"id": route.printing_id}))
    elif isinstance(record, DefaultRecord):
        default = record.data.value
        assert default is not None
        result.extend(
            (
                ("card", {"id": record.data.subject.card_id}),
                ("printing", {"id": default.printing_id}),
            )
        )
    return {canonical({"table": table, "key": dict(key)}) for table, key in result}


def verify_dependencies(record: Record, *, source_lang: str | None = None) -> None:
    """Do not allow callers to omit an inconvenient dependency or add unsigned ones."""
    actual = {
        canonical(dep.model_dump(mode="json")) for dep in record.data.dependencies
    }
    if actual != direct_dependencies(record, source_lang=source_lang):
        raise ValueError("Adoption direct dependency closure mismatch")


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
    record: VocabularyRecord, review: ReviewContext, sources: AdoptionSources
) -> Term | None:
    """Reconstruct a baseline label and exact source-field mappings."""
    value = record.data.value
    if value is None:
        return None
    ordered(value.raw_mappings)
    for mapping in value.raw_mappings:
        if not any(
            isinstance(e, TextEvidence) and e.source_ref == mapping.source_ref
            for e in record.evidence
        ):
            raise ValueError("Vocabulary raw mapping lacks approved source evidence")
        text, _, _ = sources.text(mapping.source_ref, review)
        if (text.lang, text.text) != (mapping.lang, mapping.raw) or mapping.lang != (
            "ja" if mapping.region == "jp" else "en"
        ):
            raise ValueError("Vocabulary mapping exact raw/region/language mismatch")
        expected = _mapping_locator(record.data.subject.kind, mapping.region)
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
        label=sources.value(value.label, record, review),
        active=value.active,
    )


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
    if kind == "trait":
        # Until the compound-trait migration is independently verified, no candidate is adopted.
        raise ValueError("Trait mapping adoption awaits compound-trait verification")
    if kind not in fields:
        raise ValueError("Source-field recipe is not enabled for this vocabulary kind")
    return {fields[kind]}


def alias(
    record: AliasRecord, review: ReviewContext, sources: AdoptionSources
) -> Alias | None:
    """Recompute normalization with pinned Unicode data and implementation."""
    value = record.data.value
    if value is None:
        return None
    pin = value.normalizer
    sources.repository.implementation(pin, review.context)
    if (
        pin.version != "nfkc-casefold-v1"
        or pin.code_path != "carddb/src/sve_carddb/routes/codec.py"
        or pin.config != {"unicode_version": unicodedata.unidata_version}
    ):
        raise ValueError("Unsupported alias normalizer pins")
    normalized = folded_key(record.data.subject.text)
    if value.normalized != normalized:
        raise ValueError("Alias normalized text cannot be reproduced")
    return Alias(**record.data.subject.model_dump(), normalized=normalized)


def symbol(
    record: SymbolRecord, review: ReviewContext, sources: AdoptionSources, decision: str
) -> Symbol | None:
    """Reuse the public finite parameter and placeholder contract."""
    value = record.data.value
    if value is None:
        return None
    ordered(value.spellings)
    base = value.source_localization
    texts = [
        sources.value(item, record, review)
        for item in (base.name, base.tooltip, base.copy_pattern)
    ]
    if any(item.lang != base.lang for item in texts):
        raise ValueError("Symbol source localization language mismatch")
    return Symbol(
        id=record.data.subject.id,
        code=value.code,
        parameter_schema=value.parameter_schema,
        keyword_id=value.keyword_id,
        spellings=value.spellings,
        localizations=(
            Localization(
                lang=base.lang,
                name=texts[0].text,
                tooltip=texts[1].text,
                copy_pattern=texts[2].text,
            ),
        ),
        decision_id=decision,
    )


def image_faces(
    record: Record, review: ReviewContext, sources: AdoptionSources
) -> None:
    """Evidence associations belong to their immutable review context."""
    for item in record.evidence:
        if isinstance(item, ImageEvidence):
            sources.image_association(item, review)


def reviewed_names(
    record: NameRecord, review: ReviewContext, sources: AdoptionSources
) -> tuple[tuple[LocalizedText, ...], tuple[LocalizedText, ...]]:
    """Replay complete reviewed source versions and the exact historical name basis."""
    value = record.data.value
    if value is None:
        return (), ()
    subject = record.data.subject
    if value.identity_ref.face_id != subject.face_id:
        raise ValueError("Special name face identity mismatch")
    ordered(value.names)
    ordered(value.observations)
    original = [sources.value(item, record, review) for item in value.names]
    expected_lang = "ja" if subject.region == "jp" else "en"
    if any(item.lang != expected_lang for item in original) or len(
        {item.text for item in original}
    ) != len(original):
        raise ValueError("Special names must be exact unique regional strings")
    for observation in value.observations:
        if observation.face_id != subject.face_id:
            raise ValueError("Special name observation face mismatch")
        sources.association(
            observation.source_ref, observation.printing_id, observation.face_id, review
        )
    reviewed = sources.registry(review)
    physical = [
        r.data
        for r in reviewed.records.values()
        if isinstance(r.data, PrintingData)
        and r.data.region == subject.region
        and any(m.face_id == subject.face_id for m in r.data.source_face_map)
    ]
    if not physical or any(p.card_id != value.identity_ref.card_id for p in physical):
        raise ValueError("Special name reviewed face/card/region mismatch")
    if {p.id for p in physical} != {o.printing_id for o in value.observations}:
        raise ValueError("Special name reviewed printing observation closure mismatch")
    if {
        (p.id, version)
        for p in physical
        for version in sources.printing_sources(p, review)
    } != {(o.printing_id, o.source_ref.source_version_id) for o in value.observations}:
        raise ValueError("Special name reviewed source version closure mismatch")
    observed = [sources.text(item.source_ref, review)[0] for item in value.observations]
    basis = sorted(
        {
            canonical(
                {"lang": item.lang, "text_hash": digest(item.text.encode())}
            ).decode()
            for item in observed
        }
    )
    if (
        digest(canonical([parse(item.encode()) for item in basis]))
        != value.name_basis_hash
    ):
        raise ValueError("Special name basis hash mismatch")
    _rule_evidence(record, review, sources)
    return tuple(original), tuple(observed)


def names(
    record: NameRecord,
    review: ReviewContext,
    sources: AdoptionSources,
    plan: TextPlan | None,
    decision: str,
) -> tuple[NameBinding, ...]:
    """Compare independently rebuilt current names with reviewed historical content."""
    value = record.data.value
    if value is None:
        return ()
    if plan is None:
        raise ValueError(
            "Special names require independently verified full text inputs"
        )
    original, observed = reviewed_names(record, review, sources)
    subject = record.data.subject
    expected_lang = "ja" if subject.region == "jp" else "en"
    current = [
        item
        for item in plan.candidates()
        if (item.face_id, item.region) == (subject.face_id, subject.region)
    ]
    current_printings = [
        r.data
        for r in plan.identity.snapshot.records.values()
        if isinstance(r.data, PrintingData)
        and r.data.region == subject.region
        and any(m.face_id == subject.face_id for m in r.data.source_face_map)
    ]
    if not current_printings or any(
        p.id in plan.unavailable for p in current_printings
    ):
        raise ValueError("Special name current source coverage is incomplete")
    if (
        not current
        or any(item.card_id != value.identity_ref.card_id for item in current)
        or any(item.printing_id in plan.unavailable for item in current)
    ):
        raise ValueError("Stale special name face/card/region")
    if {
        (item.card.faces[item.source_index].name, expected_lang) for item in current
    } != {(item.text, item.lang) for item in observed}:
        raise ValueError("Stale special name exact observed name basis")
    _name_relation(record, review, sources, current)
    return tuple(
        NameBinding(
            face_id=subject.face_id,
            region=subject.region,
            official_name=item.text,
            role=subject.role,
            decision_id=decision,
        )
        for item in original
    )


def _rule_evidence(
    record: NameRecord, review: ReviewContext, sources: AdoptionSources
) -> tuple[tuple[str, str], ...]:
    """Keep the relation's rules evidence separate from mere name co-occurrence."""
    basis = []
    for evidence in record.evidence:
        if not isinstance(evidence, TextEvidence):
            continue
        ref = evidence.source_ref
        parts = ref.locator.split("/")
        if len(parts) < _FIELD_PARTS or parts[1] != "faces" or not parts[2].isdecimal():
            continue
        if parts[3] not in {"text", "sections"}:
            continue
        old, source, _ = sources.text(ref, review)
        value = record.data.value
        assert value is not None
        reviewed = [
            o
            for o in value.observations
            if o.source_ref.source_version_id == source.id
            and o.source_ref.locator.split("/")[2] == parts[2]
        ]
        if not reviewed:
            raise ValueError(
                "Special relation rules basis is outside reviewed physical scope"
            )
        basis.append((parts[3], old.text))
    if not basis:
        raise ValueError(
            "Special construction relation requires separate rules evidence"
        )
    return tuple(basis)


def _name_relation(
    record: NameRecord,
    review: ReviewContext,
    sources: AdoptionSources,
    current: list[FaceObservation],
) -> None:
    """A distinct rules basis must still hold; exact names alone never imply a relation."""
    basis = _rule_evidence(record, review, sources)
    for observation in current:
        if not any(
            observation.content.effect == text
            if field == "text"
            else text in observation.content.sections
            for field, text in basis
        ):
            raise ValueError("Stale special relation exact rules basis")


def default_value(record: DefaultRecord, db: Database) -> str | None:
    """Rebuild the entire same-card same-region display candidate set."""
    value = record.data.value
    if value is None:
        return None
    subject = record.data.subject
    current = tuple(
        sorted(
            str(r.values["id"])
            for r in db.rows("printing")
            if (r.values["card_id"], r.values["region"])
            == (subject.card_id, subject.region)
        )
    )
    if value.candidates != current or value.candidates_hash != digest(
        canonical(list(current))
    ):
        raise ValueError("Stale default printing candidate closure/hash")
    if value.printing_id not in current:
        raise ValueError(
            "Default printing target is not displayable in its card/region"
        )
    return value.printing_id


def route_value(record: RouteRecord, db: Database) -> str | None:  # ruff: ignore[complex-structure] -- validate the complete route closure and permanent entry before projection
    """Validate official variants without changing a published target."""
    subject, value = record.data.subject, record.data.value
    card_path("official", subject.route_key)
    if value is None:
        if any(
            r.values["namespace"] == "official"
            and r.values["route_key"] == subject.route_key
            for r in db.rows("card_route")
        ):
            raise ValueError("Published route withdrawal requires identity repair")
        return None
    candidates = value.candidates
    keys = [c.printing_id for c in candidates]
    if (
        keys != sorted(set(keys))
        or digest(canonical([c.model_dump(mode="json") for c in candidates]))
        != value.candidates_hash
    ):
        raise ValueError("Route candidate ordering/hash mismatch")
    current = [
        r
        for r in db.rows("printing")
        if r.values["card_no_state"] == "official"
        and r.values["card_no"] == subject.route_key
    ]
    if len(current) < _MIN_VARIANTS or any(
        r.values["region"] != subject.region for r in current
    ):
        raise ValueError("Route override requires same-region exact official variants")
    if keys != sorted(str(r.values["id"]) for r in current):
        raise ValueError("Stale route variant candidate closure")
    for candidate in candidates:
        row = next(r for r in current if r.values["id"] == candidate.printing_id)
        if any(
            row.values[field] != getattr(candidate, field)
            for field in (
                "card_id",
                "region",
                "card_no",
                "card_no_state",
                "variant_key",
            )
        ):
            raise ValueError("Stale route candidate identity")
    if value.printing_id not in keys:
        raise ValueError("Route target is not an exact-number candidate")
    for row in db.rows("card_route"):
        if (
            row.values["namespace"] == "official"
            and row.values["route_key"] == subject.route_key
            and row.values["printing_id"] != value.printing_id
        ):
            raise ValueError("Published route target change requires identity repair")
    return value.printing_id
