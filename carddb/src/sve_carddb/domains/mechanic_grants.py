"""Recognize bounded self-keyword acquisitions and other-card grants in exact current Japanese main fields."""

import re
from typing import TYPE_CHECKING, Literal

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

ADAPTER = "manual-keywords-grants-bp01-v2"
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


# Exact fields keep the final expansion within its explicitly admitted owners.
_EXACT_FIELDS: dict[str, tuple[str, str, str, Literal["has", "grants"]]] = {
    # BP01-141
    "c:0354bd9a205250be958580f4b74b9c44": (
        "自分のターンごとに1回、自分の他のアミュレットが場を離れたとき、『ホーリータイガー』1体を出す。それは【守護】を持つ。",
        "kw:ward",
        "守護",
        "grants",
    ),
    # BP01-005
    "c:1c1df3db02625a6688c3a79f1f4628bb": (
        "【進化時】下記から1つチョイスする。【1】これは【疾走】を持つ。【2】相手のフォロワー1体を選ぶ。それに「これの攻撃力」と同じダメージ。",
        "kw:storm",
        "疾走",
        "has",
    ),
    # BP01-112
    "c:21fe3e24746457e88fd5f9cef0039c3e": (
        "{ファンファーレ}【真紅】状態なら、これは{攻撃力}+1/{体力}+1して、【突進】を持つ。",
        "kw:rush",
        "突進",
        "has",
    ),
    # BP01-028
    "c:3853b3eeb8845ecab213a36f94376fe6": (
        "【突進】【指定攻撃】【守護】\n{ファンファーレ}相手のリーダー1人を選ぶ。それの場のカードが3枚以上なら、これは{攻撃力}+2/{体力}+2して、【オーラ】を持つ。",
        "kw:aura",
        "オーラ",
        "has",
    ),
    # BP01-113
    "c:53097053c3f652b290299c681f47f346": (
        "{ファンファーレ}【真紅】状態なら、これは【疾走】を持つ。",
        "kw:storm",
        "疾走",
        "has",
    ),
    # BP01-119
    "c:bcf2ca13d83252188db0e7494b818c61": (
        "【必殺】\n{ファンファーレ}リーダーの{体力}-2：これは【突進】を持つ。自分のデッキの上1枚を墓場に置く。",
        "kw:rush",
        "突進",
        "has",
    ),
    # BP01-078
    "c:d680c6d777c853a5a32e0ca6d2ab2f76": (
        "{ファンファーレ}『ドラゴン』1枚をEXエリアに置く。\n{起動}{コスト2}これを{アクト}：自分のEXエリアの『ドラゴン』1枚を選ぶ。それを場に出す。それは【突進】を持つ。【覚醒】状態なら、自分のPPを2回復する。",
        "kw:rush",
        "突進",
        "grants",
    ),
}


def field_fact(
    card_id: str, raw: str, terms: Mapping[str, str]
) -> tuple[Literal["has", "grants"], str] | None:
    """Admit only exact whole fields and the unique adopted keyword mapping."""
    rule = _EXACT_FIELDS.get(card_id)
    if rule is None:
        return None
    text, keyword, spelling, relation = rule
    if raw != text or [key for key, value in terms.items() if value == spelling] != [
        keyword
    ]:
        return None
    return relation, keyword


def granted_keyword(line: str, terms: Mapping[str, str]) -> str | None:
    """Share the exact positive grant boundary with whole-field proofs."""
    match = _SELECTED.fullmatch(line) or _CONTINUOUS.fullmatch(line)
    if match is None:
        return None
    matched = [key for key, spelling in terms.items() if spelling == match["keyword"]]
    return matched[0] if len(matched) == 1 else None


def apply(
    db: Database,
    inputs: Inputs,
    settings: Settings,
    hashes: Mapping[str, str],
) -> dict[str, JsonValue]:
    """Store admitted positive facts separately from whole-field coverage proofs."""
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
                    "exact_fields": {
                        key: list[JsonValue](rule)
                        for key, rule in sorted(_EXACT_FIELDS.items())
                    },
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
        exact = field_fact(cid, original.text, terms)
        facts: list[tuple[int | None, Literal["has", "grants"], str]] = []
        if exact is not None:
            facts.append((None, *exact))
        # Quoted abilities may span lines; only an exact admitted field may bypass this.
        if "「" not in original.text and "」" not in original.text:
            facts.extend(
                (ordinal, "grants", keyword)
                for ordinal, line in enumerate(original.text.splitlines())
                if (keyword := granted_keyword(line, terms)) is not None
            )
        for ordinal, relation, keyword in facts:
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
                            "relation": relation,
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
                    "relation": relation,
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
                    relation=relation,
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
