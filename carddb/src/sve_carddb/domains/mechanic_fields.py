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
    from sve_carddb.domains.translations.four_layer_classification import Recognized
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart

ADAPTER = "manual-keywords-fields-bp01-v11"
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
_STAT_GAIN = r"(?:\{攻撃力\}\+[0-9]+(?:/\{体力\}\+[0-9]+)?|\{体力\}\+[0-9]+)する。"
_FIXED_STATS = re.compile(
    r"(?:"
    r"(?:\{ファンファーレ\}|\{ラストワード\}|自分のエンドフェイズが来たとき、)?"
    r"(?:自分の(?:他の|\{ロイヤル\})?フォロワーすべては"
    + _STAT_GAIN
    + r"|自分の(?:他の)?フォロワー1体を選ぶ。それは"
    + _STAT_GAIN
    + r"(?:[0-9]+枚引く。)?)|"
    r"自分の場に他のフォロワーが出たとき、これは"
    + _STAT_GAIN
    + r"|自分の場に\{ロイヤル\}フォロワーが出たとき、それは"
    + _STAT_GAIN
    + r"|\{ファンファーレ\}自分の場の他の妖精・フォロワーすべてと、"
    r"自分のEXエリアの妖精・フォロワーすべては"
    + _STAT_GAIN
    + r"|【進化時】相手のフォロワーすべてに[0-9]+ダメージ。"
    r"自分の他のフォロワーすべては" + _STAT_GAIN + r")"
)
_CONDITIONAL_STATS = re.compile(
    r"\{ファンファーレ\}【(?:覚醒|真紅)】状態なら、これは" + _STAT_GAIN
)
_QUICK = re.compile(r"\{クイック\}")
_REMOVAL = re.compile(
    r"(?:"
    r"【進化時】アクト状態の相手のフォロワー1体を選ぶ。それを破壊する。|"
    r"自分のフォロワー1体を選ぶ。それを破壊する。[0-9]+枚引く。|"
    r"場のフォロワーすべてを破壊する。|"
    r"体力[0-9]+以下の相手のフォロワー1体を選ぶ。それを消滅させる。|"
    r"自分の場のカード1枚を選ぶ。それを破壊する。"
    r"自分のリーダーは\{体力\}\+[0-9]+する。[0-9]+枚引く。|"
    r"【進化時】相手のEXエリアのカード1枚を選ぶ。それを消滅させる。|"
    r"相手の場のカード1枚を選ぶ。それを破壊する。"
    r")"
)
_DRAW_HAND_MOVE = re.compile(
    r"(?:(?:【攻撃時】|\{ファンファーレ\})[0-9]+枚引く。"
    r"自分の手札[0-9]+枚を捨てる。|"
    r"\{ファンファーレ\}[0-9]+枚引く。自分の手札[0-9]+枚をデッキの下に置く。)"
)
_DECK_SEARCH = re.compile(
    r"(?:【進化時】自分のデッキから(?:\{進化\}を持つフォロワー|アミュレット)|"
    r"\{ファンファーレ\}自分のデッキから(?:\{ロイヤル\}フォロワー|忍者・カード|スペル))"
    r"1枚を探し、手札に加える。"
)
_TRIGGER_DAMAGE = re.compile(
    r"(?:"
    r"これが場から手札に戻ったとき、相手のフォロワー1体を選ぶ。それに[0-9]+ダメージ。|"
    r"自分の場に他のフォロワーが出たとき、相手のフォロワー(?:1体を|2体まで)選ぶ。"
    r"それに[0-9]+ダメージ。|"
    r"自分の妖精・フォロワーが攻撃するとき、"
    r"相手のリーダー1人か相手のフォロワー1体を選ぶ。それに[0-9]+ダメージ。|"
    r"相手のフォロワーが破壊されたとき、それのリーダーに[0-9]+ダメージ。"
    r"自分のリーダーは\{体力\}\+[0-9]+する。"
    r")"
)
_GRAVE_MOVE = re.compile(
    r"(?:"
    r"\{起動\}これを\{アクト\}墓場に置く：自分の墓場のコスト[0-9]+以上のフォロワー"
    r"1枚を選ぶ。それを手札に加える。|"
    r"\{ファンファーレ\}自分の墓場のフォロワー2枚まで選ぶ。それを手札に加える。|"
    r"自分の墓場のコスト[0-9]+以下のアミュレット3枚まで選ぶ。それを場に出す。|"
    r"\{ファンファーレ\}自分の墓場のコスト[0-9]+以下のアミュレット1枚を選ぶ。それを場に出す。|"
    r"\{ファンファーレ\}自分のデッキの上[0-9]+枚を墓場に置く。"
    r")"
)
_ACT_REMOVAL = re.compile(
    r"(?:"
    r"(?:\{ファンファーレ\}|\{起動\}これを\{アクト\}：)"
    r"相手のフォロワー1体を選ぶ。それをアクトする。|"
    r"これはアクト状態で場に出る。|"
    r"\{起動\}これを\{アクト\}墓場に置く：相手のフォロワー1体を選ぶ。それを破壊する。|"
    r"自分のエンドフェイズが来たとき、これは消滅する。|"
    r"【進化時】フォロワー1体を墓場に置く：相手のフォロワー1体を選ぶ。それを破壊する。"
    r")"
)
_CONDITIONAL_EFFECT = re.compile(
    r"(?:"
    r"【進化時】【覚醒】状態なら、[0-9]+枚引く。|"
    r"相手のフォロワー1体を選ぶ。それに[0-9]+ダメージ。"
    r"【覚醒】状態なら、[0-9]+枚引く。|"
    r"フォロワーすべてに[0-9]+ダメージ。【覚醒】状態なら、代わりに[0-9]+ダメージ。|"
    r"\{ファンファーレ\}相手のフォロワー1体を選ぶ。それに[0-9]+ダメージ。"
    r"【コンボ_[0-9]+】代わりに[0-9]+ダメージ。|"
    r"(?:\{ファンファーレ\})?相手のフォロワー1体を選ぶ。それに[0-9]+ダメージ。"
    r"【スペルチェイン_[0-9]+】代わりに[0-9]+ダメージ。"
    r"(?:【SC_[0-9]+】それのリーダーに[0-9]+ダメージ。)?|"
    r"\{ファンファーレ\}自分の場にアミュレットがあるなら、"
    r"自分のリーダーは\{体力\}\+[0-9]+する。"
    r")"
)
_STATE_RULE = re.compile(
    r"(?:"
    r"これが受けるダメージを-[0-9]+する。|"
    r"相手のターン中、これが与えるダメージを\+[0-9]+する。|"
    r"これは相手を攻撃できない。|"
    r"\{ラストワード\}自分のPP最大値を\+[0-9]+する。|"
    r"\{進化\}リーダーの\{体力\}-[0-9]+：これは進化する。|"
    r"【攻撃時】フォロワーへの攻撃なら、これは\{攻撃力\}\+[0-9]+する。|"
    r"自分のエンドフェイズが来たとき、これは\{体力\}\+[0-9]+する。"
    r")"
)
_BOUNDED_GRANT = re.compile(
    r"(?:\{ファンファーレ\}自分の他のフォロワー1体を選ぶ。|"
    r"自分の墓場のコスト[0-9]+以下のフォロワー1枚を選ぶ。それを場に出す。)"
    r"それは(?:\{攻撃力\}\+[0-9]+/\{体力\}\+[0-9]+して、)?"
    r"【(?P<keyword>[^【】]+)】を持つ。"
)


_DECK_LOOK = re.compile(
    r"(?:"
    r"\{ファンファーレ\}自分のデッキの上1枚を見る。その中から、"
    r"スペル1枚を公開して手札に加えてよい。|"
    r"(?:\{ファンファーレ\})?自分のデッキの上[0-9]+枚を見る。その中から、"
    r"(?:アミュレット1枚を公開して手札に加えてよい。|"
    r"スペル1枚を公開して手札に加えてよい。スペル1枚を墓場に置いてよい。|"
    r"コスト[0-9]+以上の\{ドラゴン\}カード1枚を公開して手札に加えてよい。|"
    r"コスト[0-9]+のフォロワー1枚を場に出してよい。)"
    r"残りを好きな順にデッキの下に置く。|"
    r"各プレイヤーは、自身のメインフェイズが来たとき、デッキの上1枚を見る。"
    r"その中から、フォロワー1枚を場に出してよい。"
    r")"
)
_COSTED_EFFECT = re.compile(
    r"(?:"
    r"【進化時】場の他のカード1枚を手札に戻す：これは" + _STAT_GAIN + r"|"
    r"\{ファンファーレ\}自分の妖精・フォロワー1体を選ぶ。それは" + _STAT_GAIN + r"|"
    r"\{起動\}\{コスト[0-9]+\}これを\{アクト\}：これを手札に戻す。|"
    r"【進化時】手札のスペル1枚を捨てる：相手のフォロワー1体を選ぶ。"
    r"それに[0-9]+ダメージ。|"
    r"[0-9]+枚引く。【スペルチェイン_[0-9]+】自分のPPを[0-9]+回復する。|"
    r"相手のフォロワー1体を選ぶ。それを破壊する。"
    r"【スペルチェイン_[0-9]+】それのリーダーに[0-9]+ダメージ。|"
    r"\{ラストワード\}リーダーの\{体力\}-[0-9]+：これを場に出す。|"
    r"\{起動\}EXエリアの妖精・フォロワー1枚を消滅："
    r"自分のリーダーは\{体力\}\+[0-9]+する。[0-9]+枚引く。"
    r"この能力は1ターンに1回使える。|"
    r"自分のエンドフェイズが来たとき、自分のリーダーの\{体力\}[0-9]+以下なら、"
    r"これを\{アクト\}墓場に置く：相手のフォロワーすべてに[0-9]+ダメージ。|"
    r"\{ファンファーレ\}[0-9]+枚引く。自分のリーダーに[0-9]+ダメージ。"
    r")"
)
_COST_ADJUSTMENT = re.compile(
    r"(?:"
    r"【進化時】自分の墓場の元のコスト[0-9]+以下のスペル1枚を選ぶ。"
    r"それのコストを0にしてプレイする。|"
    r"これをプレイする際、【スペルチェイン_[0-9]+】コストを-[0-9]+する。"
    r"【SC_[0-9]+】代わりに-[0-9]+する。【SC_[0-9]+】代わりに-[0-9]+する。|"
    r"これをプレイする際、【真紅】状態なら、コストを-[0-9]+する。|"
    r"これをプレイする際、【ネクロチャージ_[0-9]+】コストを-[0-9]+する。|"
    r"\{起動\}これを\{アクト\}：相手のフォロワー1体を選ぶ。それを破壊する。"
    r")"
)


_REMINDER_EFFECT = re.compile(
    r"(?:"
    r"\{ファンファーレ\}場の他のカード1枚を手札に戻す：これは"
    + _STAT_GAIN
    + r"（手札に戻ったトークンはゲームから取り除く）|"
    r"自分の場のカード1枚を選ぶ。それを手札に戻す。[0-9]+枚引く。"
    r"（手札に戻ったトークンはゲームから取り除く）|"
    r"相手の場のフォロワー1体を選ぶ。それに[0-9]+ダメージ。"
    r"【覚醒】状態なら、代わりに[0-9]+ダメージ。"
    r"\(自分のPP最大値が7以上なら、【覚醒】状態になる\)|"
    r"\{ファンファーレ\}相手のフォロワー1体を選ぶ。それをEXエリアに置く。"
    r"（EXエリアに置いたエボルヴフォロワーはゲームから取り除く）|"
    r"相手のフォロワー1体を選ぶ。それのプレイヤーはそれをデッキの上か下に置く。"
    r"【コンボ_[0-9]+】それをデッキに置く代わりに、相手のフォロワーすべてを"
    r"それのプレイヤーはデッキの上か下に置く。"
    r"（1体ずつ上か下か決める）（デッキに置いたトークンはゲームから取り除く）"
    r")"
)
_STATE_REPLACEMENT = re.compile(
    r"(?:"
    r"相手のフォロワー1体を選ぶ。このターン、それはダメージを与えない。|"
    r"これが場にいる限り、自分がゴーレム・フォロワーをプレイする際、コストを-[0-9]+する。|"
    r"自分の場にゴーレム・フォロワーが出たとき、"
    r"相手のリーダー1人か相手の場のフォロワー1体を選ぶ。それに[0-9]+ダメージ。|"
    r"これがいる限り、自分のフォロワーは攻撃力ではなく体力と同じダメージを与える。|"
    r"【攻撃時】相手のフォロワーすべてに[0-9]+ダメージ。"
    r"自分の墓場に『ツインプリズナー・フラム』と『ツインプリズナー・グラス』があるなら、"
    r"代わりに[0-9]+ダメージ。"
    r")"
)
_ZONE_COST_REPLACEMENT = re.compile(
    r"(?:"
    r"\{ファンファーレ\}自分のデッキの上1枚をEXエリアに置く。"
    r"それをプレイする際、コストを-[0-9]+する。|"
    r"各プレイヤーは、手札すべてをデッキに戻し、シャッフルし、戻した枚数と同じだけ引く。"
    r"自分は、さらに[0-9]+枚引く。|"
    r"これをプレイする際、場のスタンド状態の\{ナイトメア\}カード4枚を墓場に置く、"
    r"または、EXエリアの\{ナイトメア\}カード4枚を消滅：コストを-[0-9]+する。|"
    r"これをプレイする際、墓場のスペル10枚を消滅：コストを[0-9]+にする。|"
    r"このターンのあと、自分の追加ターンを行う。|"
    r"自分のデッキの上1枚をEXエリアに置く。これをEXエリアが上限になるまでくり返す。|"
    r"次に自分のエンドフェイズが来たとき、自分のEXエリアのカードすべてを消滅させる。"
    r")"
)


_NAMED_TOKEN = re.compile(
    r"(?:"
    r"【進化時】『乙姫お守り隊』3枚を、場が上限になるまで出す。残りをEXエリアに置く。|"
    r"(?:\{ファンファーレ\}|\{起動\}これを\{アクト\}【土の秘術】：)"
    r"『防御型ゴーレム』1体か『攻撃型ゴーレム』1体を出す。|"
    r"\{起動\}これを\{アクト\}：『大地の魔片』1つを出す。|"
    r"\{ファンファーレ\}『冥焔の右腕・ミミ』1枚か『冥焔の左腕・ココ』1枚をEXエリアに置く。"
    r"【ネクロチャージ_[0-9]+】代わりに1枚ずつ置く。|"
    r"【進化時】『冥焔の右腕・ミミ』『冥焔の左腕・ココ』1枚ずつEXエリアに置く。|"
    r"\{起動\}\{コスト[0-9]+\}これと『ツインプリズナー・グラス』1体を墓場に置く："
    r"自分のデッキから『フラム＝グラス』1枚を探し、場に出す。"
    r")"
)
_STACK_COUNTER = re.compile(
    r"(?:"
    r"【スタック】|"
    r"\{ファンファーレ\}体力[0-9]+以下の相手のフォロワー1体を選ぶ。それを消滅させる。|"
    r"\{ラストワード\}『大地の魔片』1つを出す。自分の場の【スタック】を\+[0-9]+する。|"
    r"(?:\{ファンファーレ\}【土の秘術】|\{起動\}これを\{アクト\}【土の秘術】：)"
    r"相手のフォロワー1体を選ぶ。それに[0-9]+ダメージ。それのリーダーに[0-9]+ダメージ。|"
    r"自分がスペルをプレイしたとき、これにスペルカウンター[0-9]+つを置く。|"
    r"\{起動\}これのスペルカウンター[0-9]+つを取る：相手のフォロワー1体を選ぶ。"
    r"それに[0-9]+ダメージ。"
    r")"
)


_BOUNDED_BODIES = (
    _EVOLUTION,
    _FIXED_EFFECT,
    _FIXED_STATS,
    _CONDITIONAL_STATS,
    _QUICK,
    _REMOVAL,
    _DRAW_HAND_MOVE,
    _DECK_SEARCH,
    _TRIGGER_DAMAGE,
    _GRAVE_MOVE,
    _ACT_REMOVAL,
    _CONDITIONAL_EFFECT,
    _STATE_RULE,
    _DECK_LOOK,
    _COSTED_EFFECT,
    _COST_ADJUSTMENT,
    _STATE_REPLACEMENT,
    _ZONE_COST_REPLACEMENT,
    _NAMED_TOKEN,
    _STACK_COUNTER,
)


def _bounded_body(
    text: str, settings: Settings, keywords: Mapping[str, str]
) -> bool | None:
    """None: no bounded form applies; False: a grant whose keyword is not unique."""
    if any(pattern.fullmatch(text) for pattern in _BOUNDED_BODIES):
        return True
    grant = _BOUNDED_GRANT.fullmatch(text)
    if grant is None:
        return None
    return (
        sum(
            settings.classifier.terms[term].source_ja == grant["keyword"]
            for term in keywords
        )
        == 1
    )


def _declared_keywords(
    raw: str,
    source: SourceDescriptor,
    settings: Settings,
    keywords: Mapping[str, str],
    part: SourcePart,
    found: Recognized,
) -> bool:
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
    return True


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
    # Certify the attached reminder with its entire line, never in isolation.
    reminder_lines: list[tuple[int, int]] = []
    offset = 0
    for line in raw.split("\n"):
        if _REMINDER_EFFECT.fullmatch(line):
            reminder_lines.append((offset, offset + len(line)))
        offset += len(line) + 1
    bodies = 0
    for part, found in zip(derived.field.parts, derived.recognized, strict=True):
        if part.source_span.role == "layout":
            continue
        if (
            part.source_span.role in {"body", "reminder"}
            and part.source_span.segments
            and any(
                all(
                    start <= span.start and span.end <= end
                    for span in part.source_span.segments
                )
                for start, end in reminder_lines
            )
        ):
            bodies += part.source_span.role == "body"
            continue
        if part.source_span.role != "body":
            return False
        spans = part.source_span.segments
        bounded = (
            _bounded_body(raw[spans[0].start : spans[0].end], settings, keywords)
            if len(spans) == 1
            else None
        )
        if bounded is None:
            bounded = _declared_keywords(raw, source, settings, keywords, part, found)
        if not bounded:
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
