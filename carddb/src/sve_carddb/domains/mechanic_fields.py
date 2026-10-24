"""Prove bounded whole fields without treating positive fragments as completeness."""

import re
from collections import Counter
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.four_layer import (
    FaceRevisionOwner,
    GlossaryReference,
    OwnerField,
)
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.catalog.adoption_models import SourceRef
from sve_carddb.domains.mechanic_coverage import (
    RELATIONS,
    FieldCheck,
    Inspection,
    required_fields,
    store_evidence,
)
from sve_carddb.domains.mechanic_grants import KEYWORDS
from sve_carddb.domains.translations.four_layer_analysis import analyze_source
from sve_carddb.domains.translations.four_layer_sources import (
    card_field,
    card_source,
    descriptor,
)
from sve_carddb.domains.translations.sources import pointer

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database
    from sve_carddb.contracts.source_binding import SourceDescriptor
    from sve_carddb.domains.mechanic_coverage import FieldKey
    from sve_carddb.domains.mechanics import Inputs, Settings

ADAPTER = "manual-keywords-fields-bp01-v4"
_EVOLUTION = re.compile(r"\{進化\}\{コスト[0-9]+\}：これは進化する。")
_FIXED_EFFECT = re.compile(
    r"(?:\{ファンファーレ\}|\{ラストワード\}|【進化時】|"
    r"自分のエンドフェイズが来たとき、|これが場を離れたとき、|"
    r"\{起動\}\{コスト[0-9]+\}これを\{アクト\}：|"
    r"\{起動\}(?:\{コスト[0-9]+\})?これを\{アクト\}墓場に置く：)?"
    r"(?:"
    r"(?:相手の(?:リーダー1人か相手の)?(?:場の)?フォロワー1体を選ぶ。それに|"
    r"(?:相手の(?:場の)?|自分の他の)?フォロワーすべてに|"
    r"(?:相手の)?リーダーすべてに|自分のリーダーに)"
    r"[0-9]+ダメージ。(?:[0-9]+枚引く。)?|"
    r"[0-9]+枚引く。|"
    r"自分のリーダーは\{体力\}\+[0-9]+する。(?:[0-9]+枚引く。)?"
    r")"
)


def _main_field(
    raw: str,
    source: SourceDescriptor,
    settings: Settings,
    keywords: Mapping[str, str],
) -> bool:
    """Prove bounded fixed effects independently of H1 declarations."""
    if "「" in raw or "」" in raw:
        return False
    derived = analyze_source(raw, source, settings.classifier)
    bodies = 0
    for part, found in zip(derived.field.parts, derived.recognized, strict=True):
        if part.source_span.role == "layout":
            continue
        if part.source_span.role != "body":
            return False
        spans = part.source_span.segments
        if len(spans) == 1 and (
            _EVOLUTION.fullmatch(raw[spans[0].start : spans[0].end])
            or _FIXED_EFFECT.fullmatch(raw[spans[0].start : spans[0].end])
        ):
            bodies += 1
            continue
        if (
            found.issues
            or found.semantics is None
            or found.semantics.key != "card_keywords.v2"
        ):
            return False
        frame, binding = found.bind(source, part)
        part.verify(raw, frame, binding, settings.classifier.domains)
        if not frame.leaf_schema.slots:
            return False
        for slot in frame.leaf_schema.slots:
            concept = binding.values[slot.name]
            if (
                slot.type != "Concept"
                or slot.role != "keyword"
                or not isinstance(concept, GlossaryReference)
                or concept.key not in keywords
            ):
                return False
        bodies += 1
    return bodies > 0


def _sections_closed(
    db: Database, source: SourceDescriptor, settings: Settings
) -> bool:
    if not isinstance(source.owner, FaceRevisionOwner):
        raise TypeError("Whole-field checks require a current revision owner")
    ref = source.source_ref
    _, document, _ = settings.sources.document(
        SourceRef(
            batch_id=ref.batch_id,
            source_version_id=ref.source_version_id,
            parser=ref.parser,
            locator=ref.locator,
            text_hash="sha256:" + ref.text_hash,
        )
    )
    frozen = pointer(document, ref.locator.rsplit("/", 1)[0] + "/sections")
    rows = db.select(
        "face_text_section",
        db.columns("face_text_section"),
        where={"revision_id": source.owner.revision_id},
    )
    actual = [
        card_field(
            db,
            OwnerField(owner=source.owner, field="section", ordinal=ordinal),
        ).text
        for ordinal, row in enumerate(rows)
        if row.values["ordinal"] == ordinal
    ]
    return len(actual) == len(rows) and frozen == actual


def _proved(
    raw: str,
    source: SourceDescriptor,
    settings: Settings,
    keywords: Mapping[str, str],
    reasons: Counter[str],
) -> bool:
    if not raw:
        return True
    if source.field == "section":
        reasons["nonempty_section"] += 1
        return False
    if not _main_field(raw, source, settings, keywords):
        reasons["unproved_main_field"] += 1
        return False
    return True


def _checks(
    db: Database,
    cid: str,
    required: set[FieldKey],
    settings: Settings,
    keywords: Mapping[str, str],
    reasons: Counter[str],
) -> tuple[FieldCheck, ...]:
    checks: list[FieldCheck] = []
    closed: dict[str, bool] = {}
    for face, revision, field, ordinal, relation in sorted(required):
        if relation != "has":
            continue
        if revision is None:
            reasons["missing_current_face"] += 1
            continue
        owner = OwnerField(
            owner=FaceRevisionOwner(kind="face_revision", revision_id=revision),
            field="effect" if field == "effect" else "section",
            ordinal=ordinal,
        )
        if field == "section" and not closed[face]:
            continue
        source = descriptor(db, settings.sources, owner, settings.batch_id)
        original = card_source(db, settings.sources, source)
        if original.card_id != cid or original.face_id != face:
            raise ValueError("Whole-field source differs from selected current owner")
        if field == "effect":
            closed[face] = _sections_closed(db, source, settings)
            if not closed[face]:
                reasons["source_section_gap"] += 1
                continue
        if not _proved(original.text, source, settings, keywords, reasons):
            continue
        checks.extend(
            FieldCheck(
                face_id=face,
                revision_id=revision,
                field="effect" if field == "effect" else "section",
                ordinal=ordinal,
                relation=relation,
                whole_field=True,
            )
            for relation in RELATIONS
        )
    return tuple(checks)


def apply(
    db: Database,
    inputs: Inputs,
    settings: Settings,
    hashes: Mapping[str, str],
    h1_version: str,
) -> dict[str, JsonValue]:
    """Only closed fields prove all four relations for this batch."""
    keywords = {
        record.term_id: record.id
        for record in inputs.registry.records
        if record.id in KEYWORDS
    }
    version = (
        ADAPTER
        + ":"
        + digest(
            canonical(
                {
                    "adapter": ADAPTER,
                    "h1_producer": h1_version,
                    "cards": list[JsonValue](sorted(settings.first_batch_cards)),
                }
            )
        )
    )
    confirmed = {
        str(row.values["id"])
        for row in db.rows("card")
        if row.values["identity_state"] == "confirmed"
    }
    inspections: list[Inspection] = []
    reasons: Counter[str] = Counter()
    for cid, required in sorted(required_fields(db).items()):
        if cid not in settings.first_batch_cards or cid not in confirmed:
            continue
        checks = _checks(db, cid, required, settings, keywords, reasons)
        if checks:
            inspections.extend(
                Inspection(
                    card_id=cid,
                    keyword_id=keyword,
                    scope="shared",
                    source_kind="manual",
                    source_text_hash=hashes[cid],
                    producer_version=version,
                    decision_id=inputs.registry.decision.id,
                    dsl_id=None,
                    checks=tuple(checks),
                )
                for keyword in sorted(keywords.values())
            )
    store_evidence(db, (), inspections)
    return {
        "adapter": ADAPTER,
        "producer_version": version,
        "batch_cards": len(settings.first_batch_cards),
        "inspections": len(inspections),
        "reasons": dict[str, JsonValue](sorted(reasons.items())),
    }
