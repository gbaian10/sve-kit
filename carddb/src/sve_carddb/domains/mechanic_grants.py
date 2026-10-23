"""Recognize bounded other-card grants in exact current Japanese main fields."""

import re
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.mechanic_coverage import FieldCheck, Inspection, store_evidence
from sve_carddb.domains.translations.four_layer_sources import (
    card_source,
    descriptor,
    empty_field,
)

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.domains.mechanics import Inputs, Settings

ADAPTER = "manual-keywords-grants-bp01-v1"
KEYWORDS = frozenset(
    "kw:" + code
    for code in (
        "ward",
        "storm",
        "rush",
        "bane",
        "assail",
        "intimidate",
        "aura",
        "drain",
        "ambush",
        "barrier",
    )
)
_PREFIX = r"(?:\{ファンファーレ\}|\{ラストワード\}|\{起動\}\{コスト[0-9]+\}これを\{アクト\}：)?"
_NAME = r"『[^『』「」【】。\n]+』"
_TARGET = (
    r"(?:自分の(?:他の)?フォロワー1体を選ぶ。"
    r"|自分の墓場のコスト[0-9]+以下のフォロワー1枚を選ぶ。それを場に出す。|"
    + _NAME
    + r"[1-9][0-9]*体を出す。)"
)
_GRANT = r"【(?P<keyword>[^【】]+)】を持つ。"
_SELECTED = re.compile(
    _PREFIX
    + _TARGET
    + r"それは(?:\{攻撃力\}\+[0-9]+/\{体力\}\+[0-9]+して、)?"
    + _GRANT
    + r"(?:それを好きな枚数アクトしてよい。)?"
)
_CONTINUOUS = re.compile(r"これがある限り、自分の" + _NAME + "すべては" + _GRANT)


def apply(
    db: Database,
    inputs: Inputs,
    settings: Settings,
    hashes: Mapping[str, str],
) -> dict[str, JsonValue]:
    """Only whole recognized lines prove positive grants, never full-field coverage."""
    terms = {
        record.id: settings.classifier.terms[record.term_id].source_ja
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
                    "cards": list[JsonValue](sorted(settings.first_batch_cards)),
                    "keywords": [
                        [record.id, record.term_id, terms[record.id]]
                        for record in sorted(
                            inputs.registry.records, key=lambda item: item.id
                        )
                        if record.id in terms
                    ],
                }
            )
        )
    )
    faces = {
        str(row.values["id"]): str(row.values["card_id"]) for row in db.rows("face")
    }
    selected = settings.first_batch_cards & {
        str(row.values["id"])
        for row in db.rows("card")
        if row.values["identity_state"] == "confirmed"
    }
    positives: list[dict[str, Value]] = []
    checks: dict[tuple[str, str], set[FieldCheck]] = {}
    for row in db.rows("face_current"):
        current = row.values
        cid = faces[str(current["face_id"])]
        if current["region"] != "jp" or cid not in selected:
            continue
        field = OwnerField(
            owner=FaceRevisionOwner(
                kind="face_revision", revision_id=str(current["revision_id"])
            ),
            field="effect",
            ordinal=None,
        )
        if empty_field(db, field):
            continue
        source = descriptor(db, settings.sources, field, settings.batch_id)
        original = card_source(db, settings.sources, source)
        if original.card_id != cid or original.face_id != current["face_id"]:
            raise ValueError("Grant source differs from its selected current owner")
        # Quoted abilities may span lines; no line inside them owns a parent grant.
        if "「" in original.text or "」" in original.text:
            continue
        for ordinal, line in enumerate(original.text.splitlines()):
            match = _SELECTED.fullmatch(line) or _CONTINUOUS.fullmatch(line)
            if match is None:
                continue
            matched = [
                kid for kid, spelling in terms.items() if spelling == match["keyword"]
            ]
            if len(matched) != 1:
                continue
            keyword = matched[0]
            identifier = (
                "mechanic:"
                + digest(
                    canonical(
                        {
                            "source": source.model_dump(mode="json"),
                            "line": ordinal,
                            "card_id": cid,
                            "keyword_id": keyword,
                            "scope": "shared",
                            "relation": "grants",
                            "source_text_hash": hashes[cid],
                            "producer_version": version,
                        }
                    )
                )[7:]
            )
            positives.append(
                {
                    "id": identifier,
                    "card_id": cid,
                    "keyword_id": keyword,
                    "relation": "grants",
                    "action": None,
                    "scope": "shared",
                    "source_kind": "manual",
                    "source_text_hash": hashes[cid],
                    "producer_version": version,
                    "dsl_id": None,
                    "source_id": original.source_id,
                    "decision_id": inputs.registry.decision.id,
                }
            )
            checks.setdefault((cid, keyword), set()).add(
                FieldCheck(
                    face_id=original.face_id,
                    revision_id=str(current["revision_id"]),
                    field="effect",
                    ordinal=None,
                    relation="grants",
                    whole_field=False,
                )
            )
    store_evidence(
        db,
        positives,
        tuple(
            Inspection(
                card_id=cid,
                keyword_id=keyword,
                scope="shared",
                source_kind="manual",
                source_text_hash=hashes[cid],
                producer_version=version,
                decision_id=inputs.registry.decision.id,
                dsl_id=None,
                checks=tuple(sorted(checked, key=lambda item: item.model_dump_json())),
            )
            for (cid, keyword), checked in sorted(checks.items())
        ),
    )
    return {
        "adapter": ADAPTER,
        "producer_version": version,
        "batch_cards": len(settings.first_batch_cards),
        "positive_tags": len(positives),
    }
