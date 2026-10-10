"""N0 numeric roles require source constructions, not generic suffix hints."""

import re
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING, Literal

from sve_carddb.contracts.four_layer import Constant, QuantitySpec
from sve_carddb.domains.translations.four_layer_choices import (
    choice_alternative,
    choice_index,
    choice_quantity,
)
from sve_carddb.domains.translations.four_layer_conditions import existence_context
from sve_carddb.domains.translations.four_layer_units import (
    compound_action,
    compound_selection,
    count_context,
    field_filter,
    source_unit,
)
from sve_carddb.domains.translations.parameters.explicit_rules import COUNTER_NAMES

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart
    from sve_carddb.domains.translations.parameters.models import Hint

_ROLES = {
    "damage_amount": "damage_amount",
    "recovery_amount": "recovery_amount",
    "received_damage_assigned_value": "damage_amount",
    "cost": "cost_value",
    "attack": "stat_value",
    "health": "stat_value",
    "cost_assigned_value": "cost_value",
    "attack_assigned_value": "stat_value",
    "health_assigned_value": "stat_value",
    "attack_health_assigned_value": "stat_value",
    "leader_health_assigned_value": "stat_value",
    "counter_remove_quantity": "counter_amount",
    "counter_group_size": "group_divisor",
    "damage_count_multiplier": "arithmetic_multiplier",
    "damage_attack_multiplier": "arithmetic_multiplier",
    "count_formula_multiplier": "arithmetic_multiplier",
    "attack_damage_multiplier": "arithmetic_multiplier",
    "cost_delta_magnitude": "cost_delta_magnitude",
    "attack_delta_magnitude": "stat_delta_magnitude",
    "health_delta_magnitude": "stat_delta_magnitude",
    "pp_capacity_delta_magnitude": "resource_delta_magnitude",
    "pp_delta_magnitude": "resource_delta_magnitude",
    "ep_delta_magnitude": "resource_delta_magnitude",
    "stack_delta_magnitude": "counter_delta_magnitude",
    **dict.fromkeys(
        (
            "counter_threshold",
            "original_cost_bound",
            "original_cost_sum_bound",
            "pp_capacity_bound",
            "pp_remaining_bound",
            "leader_health_bound",
            "attack_bound",
            "attack_sum_bound",
            "distinct_card_name_count",
            "distinct_original_cost_count",
            "received_damage_lower_bound",
            "combo_threshold",
            "lesson_threshold",
            "necrocharge_threshold",
            "spell_chain_threshold",
        ),
        "threshold",
    ),
    **{
        f"{direction}_{kind}_damage_delta_magnitude": "damage_delta_magnitude"
        for direction in ("dealt", "received")
        for kind in ("all", "ability", "combat")
    },
}
_END = r"(?=[。:、）」)\n]|$)"
_SELECT = re.compile(
    r"^(?P<unit>枚|体|つ|人)(?P<limit>まで)?(?:を)?選(?:ぶ|び|んで)(?=[。:、）)\n]|$)"
)
_EXIST = re.compile(
    r"^(?P<unit>枚|体|つ|人)(?:かN(?P=unit)(?=なら))?(?P<compare>以上|以下)?(?:あるなら|いるなら|ある限り|いる限り|なら|である限り|であれば|ある|いる)"
    r"(?:使える)?" + _END
)
_MOVE = re.compile(
    r"^枚(?:まで)?(?:を)?(?:引く|引き|引いて|引けない|引いたとき|捨てる|捨て|捨ててよい|捨てたとき|"
    r"戻す|戻し|加える|加え|置く|置き|出す|出し|消滅|消滅させる|消滅させてよい)" + _END
)
_TRANSFER = re.compile(
    r"^(?:枚|体|つ)(?:まで)?(?:を)?(?:、)?(?:裏向きで)?(?:自分の|相手の)?"
    r"(?:手札に(?:加える|加え|加えて)|墓場に(?:置く|置き|置いて)|"
    r"デッキの(?:上|下)に(?:置く|置き|置いて)|EXエリアに(?:置く|置き|置いて)|"
    r"デッキの上からN番目に置く|"
    r"場に(?:出す|出し|出して))(?:よい)?" + _END
)
_SEARCH = re.compile(
    r"^枚(?P<limit>まで)?(?:を)?探(?:し|して)、(?:それを)?"
    r"(?:手札に加える|場に出す|EXエリアに置く|墓場に置く|消滅させる|前者を場に出す。後者をEXエリアに置く)"
    + _END
)
_EQUIP = re.compile(r"^枚(?:を)?装備する" + _END)
_LOOK = re.compile(r"^枚(?:を)?見(?:る|て)" + _END)
_PUBLIC = re.compile(r"^枚(?:を)?公開(?:する)?" + _END)
_RETURN = re.compile(
    r"^(?P<unit>枚|体|つ)(?:を)?(?:手札|デッキ)に(?:戻す|戻し|戻してよい)" + _END
)
_CREATE = re.compile(r"^(?P<unit>体|つ)(?:を)?出す" + _END)
_REVEAL = re.compile(
    r"^枚(?P<limit>まで)?(?:を)?公開して(?:、)?"
    r"(?:それを|そのカードを)?(?:手札に加えるかEXエリアに置いてよい|手札に加える|手札に加えてよい|墓場に置く|場に出す|デッキの下に置く)"
    + _END
)
_CHOICE = re.compile(r"^つ(?P<limit>まで)?チョイス(?:する|して)?" + _END)
_ABILITY_COUNT = re.compile(r"^つ(?:を)?(?:持つ|持ち|持っている)" + _END)
_REPEAT = re.compile(
    r"^回(?P<limit>まで)?(?:を)?(?:行う|行い|行って|繰り返す|くり返す|使える|使用できる)"
    + _END
)
_FREQUENCY = re.compile(
    r"^回(?P<limit>まで)?(?:使える|使用できる|働く)(?=[。:、）」)\n]|$)"
)
_DURATION = re.compile(
    r"^ターン(?:に|につき)(?:N|[0-9０-９]+)回(?:まで)?(?:使える|使用できる|働く)(?=[。:、）」)\n]|$)"
)
_RESOURCE = re.compile(
    r"^(?P<unit>SEP|PP|EP)(?:を)?(?:支払う|払う|払える|回復する|回復)" + _END
)
_RESOURCE_PREFIX = re.compile(r"(?:自分|相手)の(?P<unit>SEP|PP|EP)(?:を)?$")
_RESOURCE_RECOVERY = re.compile(r"^回復(?:する|して)?" + _END)
_RESOURCE_ITEM = re.compile(r"(?<![A-Za-z0-9_])(?P<unit>SEP|EP)(?:を|は)$")
_RESOURCE_POSSESSION = re.compile(r"^つ(?:持つ|裏向きにしてよい)" + _END + r"|^つ[）)]")
_RESOURCE_PAYMENT = re.compile(
    r"^つ裏向きにすることで、(?:N|[0-9０-９]+)PPを払える" + _END
)
_COUNTER = "(?:" + "|".join((*COUNTER_NAMES, "魂", "スペル")) + ")カウンター"
_COUNTER_OBJECT = re.compile(_COUNTER + r"$")
_COUNTER_AMOUNT = re.compile(
    r"^(?P<unit>個|つ)(?P<limit>まで)?を(?:置く|置いてよい|取る)" + _END
)
_COUNTER_THRESHOLD = re.compile(
    r"^(?P<unit>個|つ)(?:以上|以下)(?:なら|である限り)(?:使える)?" + _END
)
_EVENT_THRESHOLD = re.compile(
    r"^回以上(?:攻撃した|攻撃していた|発動していた)なら" + _END
)
_EVOLUTION_FREQUENCY = re.compile(r"^ターンに何回でも使える" + _END)
_ENTRY_DIGIT = r"(?:N|[0-9０-９]+)"
_ENTRY_FREQUENCY = r"ターンに進化か(?:食事|憑依)はどちらか"
_ENTRY_DIVISOR = r"回につき使えるEPは" + _ENTRY_DIGIT + r"つ[）)]$"
_SACRIFICE = re.compile(
    r"^(?P<unit>枚|体|つ)(?:を)?(?:墓場に置く|アクトする|レストする|スタンドする|破壊する|消滅|"
    r"\{(?:アクト|レスト)\})" + _END
)
_LEADER = re.compile(r"^人(?:に(?:N|[0-9０-９]+)ダメージ|の(?:手札|墓場|場|デッキ))")
_ORDINAL = {
    "card_ordinal": ("card_index", "枚"),
    "repetition_ordinal": ("repeat_index", "回"),
    "turn_ordinal": ("turn_index", "ターン"),
    "deck_top_ordinal": ("deck_index", "番"),
}


@dataclass(frozen=True)
class Number:
    type: Literal["Nat", "Ordinal", "QuantitySpec"]
    role: str
    value: int | QuantitySpec
    source_unit: str | None
    unit_start: int | None = None


def _quantity(
    role: str,
    value: int,
    unit: str,
    mode: Literal["exact", "up_to", "at_least"] = "exact",
) -> Number:
    return Number(
        "QuantitySpec",
        role,
        QuantitySpec(mode=mode, expr=Constant(kind="constant", value=value)),
        unit,
    )


def recognize_number(raw: str, part: SourcePart, hint: Hint) -> Number | None:
    """The caller supplies the same canonical units used for the final source trace."""
    if hint.value is None or hint.issues:
        return None
    before = part.canonical_source[: hint.occurrence.start]
    after = part.canonical_source[hint.occurrence.end :]
    number = _recognize(raw, hint, before, after)
    if number is None or number.source_unit is None:
        return number
    unit_start = hint.occurrence.end if number.unit_start is None else number.unit_start
    if not part.canonical_source[unit_start:].startswith(number.source_unit):
        return None
    indices = range(unit_start, unit_start + len(number.source_unit))
    positions = sorted(
        {
            i
            for index in indices
            for span in part.units[index].origins
            for i in range(span.start, span.end)
        }
    )
    number = replace(number, source_unit="".join(raw[i] for i in positions))
    if number.type == "QuantitySpec" and number.role in {
        "selection_count",
        "existence_count",
    }:
        counted = count_context(before) or (
            existence_context(before) if number.role == "existence_count" else None
        )
        if (
            counted is None
            or not source_unit(counted, number.source_unit or "").merge_allowed
        ):
            return None
    return number


def number_issue(raw: str, part: SourcePart, hint: Hint) -> str:
    """A known counted set with the wrong unit differs from an unknown construction."""
    if hint.value is not None and not hint.issues:
        before = part.canonical_source[: hint.occurrence.start]
        after = part.canonical_source[hint.occurrence.end :]
        number = _recognize(raw, hint, before, after)
        if (
            number is not None
            and number.type == "QuantitySpec"
            and number.role in {"selection_count", "existence_count"}
            and number.source_unit is not None
            and (
                counted := count_context(before)
                or (
                    existence_context(before)
                    if number.role == "existence_count"
                    else None
                )
            )
            is not None
        ):
            decision = source_unit(counted, number.source_unit)
            if decision.reason == "source_unit_mismatch":
                return decision.reason
    return "n0_numeric_construction_unresolved"


def _recognize(raw: str, hint: Hint, before: str, after: str) -> Number | None:
    assert hint.value is not None
    role = hint.semantic_role
    if (match := _RESOURCE_PREFIX.search(before)) and _RESOURCE_RECOVERY.match(after):
        return Number(
            "Nat", "resource_amount", hint.value, match["unit"], match.start("unit")
        )
    if role in _ROLES:
        return Number("Nat", _ROLES[role], hint.value, _registered_unit(role, after))
    if role == "choice_ordinal":
        return (
            Number("Ordinal", "choice_index", hint.value, None)
            if choice_index(raw, hint)
            else None
        )
    if _CHOICE.match(after) and not choice_quantity(raw, hint):
        return None
    if before.endswith("代わりに") and choice_alternative(raw, hint):
        return _quantity("choice_mode_count", hint.value, "つ", "up_to")
    return _constructed_number(hint, before, after, None)


def _registered_unit(role: str, after: str) -> str | None:
    expected = (
        "ダメージ"
        if role == "damage_amount"
        else "個"
        if role.startswith("counter_")
        else "倍"
        if role.endswith("_multiplier")
        else "種類"
        if role in {"distinct_card_name_count", "distinct_original_cost_count"}
        else None
    )
    return expected if expected is not None and after.startswith(expected) else None


def _constructed_number(
    hint: Hint, before: str, after: str, unit: str | None
) -> Number | None:
    assert hint.value is not None
    role = hint.semantic_role
    if role == "counter_place_quantity":
        return (
            _quantity("counter_amount", hint.value, "個", "up_to")
            if after.startswith("個まで")
            else Number("Nat", "counter_amount", hint.value, "個")
        )
    if role in _ORDINAL:
        return _ordinal_number(hint.value, role, before, after)
    return (
        _generic_number(hint.value, before, after, unit)
        if role
        in {
            "numeric",
            "item_quantity",
            "leader_person_quantity",
            "player_person_quantity",
        }
        else None
    )


def _ordinal_number(value: int, role: str, before: str, after: str) -> Number | None:
    new_role, unit = _ORDINAL[role]
    if value < 1 or not after.startswith(unit + "目"):
        return None
    if role == "turn_ordinal":
        valid = (
            re.search(r"(?:先攻|後攻)のプレイヤーなら$", before) is not None
            and re.match(r"^ターン目以降(?:、|に)", after) is not None
        ) or (
            re.search(r"(?:自分|相手)のターンが$", before) is not None
            and re.match(r"^ターン目かそれ以降(?:でない)?なら" + _END, after)
            is not None
        )
    elif role == "repetition_ordinal":
        event = re.match(r"^回目の(?P<np>[^。:：]+)の(?:攻撃|進化)なら" + _END, after)
        valid = (
            re.search(r"このターン(?:、|中に)$", before) is not None
            and event is not None
            and count_context(event["np"]) is not None
        )
    else:
        valid = (
            re.search(r"(?:第|このターン中に|ターン中に|デッキの上から)$", before)
            is not None
        )
    return Number("Ordinal", new_role, value, unit) if valid else None


def _generic_number(
    value: int, before: str, after: str, unit: str | None
) -> Number | None:
    if counter := _counter_number(value, before, after) or _event_count_number(
        value, before, after
    ):
        return counter
    if compound := compound_selection(after, before):
        return _quantity(
            "selection_count", value, compound[0], "up_to" if compound[1] else "exact"
        )
    if match := _SELECT.match(after):
        return _quantity(
            "selection_count",
            value,
            unit or match["unit"],
            "up_to" if match["limit"] else "exact",
        )
    if match := _EXIST.match(after):
        mode: Literal["exact", "up_to", "at_least"] = (
            "at_least"
            if match["compare"] == "以上"
            else "up_to"
            if match["compare"] == "以下"
            else "exact"
        )
        return _quantity("existence_count", value, unit or match["unit"], mode)
    if match := _CHOICE.match(after):
        return _quantity(
            "choice_mode_count",
            value,
            "つ",
            "up_to" if match["limit"] else "exact",
        )
    return _action_number(value, before, after)


def _set_number(value: int, before: str, after: str) -> Number | None:
    if before.endswith("これはエボルヴデッキに") and re.match(
        r"^枚まで入れることができる" + _END, after
    ):
        return Number("Nat", "threshold", value, "枚")
    if re.search(r"(?:カード名|好きな数)$", before) and re.match(
        r"^つを指定する" + _END, after
    ):
        return _quantity("selection_count", value, "つ")
    if before.endswith("自分は") and re.match(
        r"^つ以上の選択肢をチョイスする際、", after
    ):
        return Number("Nat", "threshold", value, "つ")
    if (
        value >= 1
        and (counted := count_context(before)) is not None
        and re.match(r"^(?:枚|体|つ)につき(?:、|(?:N|X)ダメージ" + _END + r")", after)
        and source_unit(counted, after[0]).merge_allowed
    ):
        return Number("Nat", "group_divisor", value, after[0])
    if re.search(r"(?:自分|相手)プレイヤー$", before) and re.match(
        r"^人(?:は(?:手札を公開する|下記からNつチョイスする)|の場のカードがN枚以上なら)"
        + _END,
        after,
    ):
        return _quantity("selection_count", value, "人")
    return None


def _action_number(value: int, before: str, after: str) -> Number | None:
    if compound := compound_action(after, before):
        return (
            _quantity("selection_count", value, compound[0], "up_to")
            if compound[1]
            else Number("Nat", "count", value, compound[0])
        )
    if counted := _set_number(value, before, after):
        return counted
    if _ABILITY_COUNT.match(after) and re.search(r"(?:能力を|能力が|能力)$", before):
        return Number("Nat", "count", value, "つ")
    if (match := _CREATE.match(after)) and re.search(
        r"(?:『X』|フォロワー|アミュレット)(?:を)?$", before
    ):
        return Number("Nat", "count", value, match["unit"])
    if count_context(before) is not None and re.match(
        r"^枚を(?:表向き|裏向き)にする" + _END, after
    ):
        return Number("Nat", "count", value, "枚")
    return (
        _movement_number(value, after)
        or _payment_number(value, before, after)
        or _repetition_number(value, before, after)
        or _field_number(value, before, after)
    )


def _event_count_number(value: int, before: str, after: str) -> Number | None:
    if predicate := _predicate_number(value, before, after):
        return predicate
    counted = count_context(before)
    if (
        counted is not None
        and (
            match := re.match(
                r"^(?P<unit>枚|体|つ)以上に(?:能力)?ダメージを与えたとき" + _END, after
            )
        )
        and source_unit(counted, match["unit"]).merge_allowed
    ):
        return Number("Nat", "threshold", value, match["unit"])
    if (
        counted is not None
        and (match := re.match(r"^(?P<unit>体|つ)に(?:N|X)ダメージ" + _END, after))
        and source_unit(counted, match["unit"]).merge_allowed
    ):
        return _quantity("selection_count", value, match["unit"])
    if re.search(r"(?:自分|相手)が手札を$", before) and re.match(
        r"^枚以上捨てたとき" + _END, after
    ):
        return Number("Nat", "threshold", value, "枚")
    if re.search(r"(?:自分|相手)のデッキ$", before) and re.match(
        r"^枚が墓場に(?:置かれた|送られた)とき" + _END, after
    ):
        return Number("Nat", "count", value, "枚")
    return None


def _predicate_number(value: int, before: str, after: str) -> Number | None:
    if (unit := _event_predicate_unit(before)) and re.match(
        r"^" + unit + r"以上なら" + _END, after
    ):
        return Number("Nat", "threshold", value, unit)
    counted = count_context(before)
    if (
        counted is not None
        and counted.counted_zones == ("hand",)
        and re.match(
            r"^枚になるように(?:自分|相手|自身|それ|お互い)の手札を捨てる" + _END, after
        )
    ):
        return _quantity("existence_count", value, "枚")
    if (
        value >= 1
        and re.fullmatch(r"[（(]", before)
        and re.fullmatch(r"体ずつ上か下か決める[）)]", after)
    ):
        return Number("Nat", "group_divisor", value, "体")
    return None


def _event_predicate_unit(before: str) -> str | None:
    if re.search(r"「このターン中に(?:自分|相手)がプレイしたカードの枚数」が$", before):
        return "枚"
    if re.search(
        r"「このターン中に(?:自分|相手)のリーダーの体力が(?:増加|減少|回復)した回数」が$",
        before,
    ):
        return "回"
    return None


def _movement_number(value: int, after: str) -> Number | None:
    if _MOVE.match(after) or _TRANSFER.match(after) or _LOOK.match(after):
        unit = after[0]
        return (
            _quantity("selection_count", value, unit, "up_to")
            if after.startswith(unit + "まで")
            else Number("Nat", "count", value, unit)
        )
    if match := _SEARCH.match(after):
        return (
            _quantity("selection_count", value, "枚", "up_to")
            if match["limit"]
            else Number("Nat", "count", value, "枚")
        )
    if _EQUIP.match(after) or _PUBLIC.match(after):
        return Number("Nat", "count", value, "枚")
    if match := _RETURN.match(after):
        return Number("Nat", "count", value, match["unit"])
    if match := _REVEAL.match(after):
        return _quantity(
            "selection_count", value, "枚", "up_to" if match["limit"] else "exact"
        )
    return None


def _payment_number(value: int, before: str, after: str) -> Number | None:
    if match := _RESOURCE.match(after):
        return Number("Nat", "resource_amount", value, match["unit"])
    if match := _SACRIFICE.match(after):
        return Number("Nat", "count", value, match["unit"])
    if _LEADER.match(after) and re.search(r"(?:自分|相手)のリーダー$", before):
        return _quantity("selection_count", value, "人")
    if _RESOURCE_ITEM.search(before) and (
        _RESOURCE_POSSESSION.match(after) or _RESOURCE_PAYMENT.match(after)
    ):
        return Number("Nat", "resource_amount", value, "つ")
    return _counter_number(value, before, after)


def _counter_number(value: int, before: str, after: str) -> Number | None:
    if re.search(r"(?:^|[。、:：}】])(?:自分の)?場の『X』$", before) and (
        match := re.match(
            r"^(?P<unit>枚|体|つ)の" + _COUNTER + r"N(?:個|つ)を取る" + _END, after
        )
    ):
        return Number("Nat", "count", value, match["unit"])
    if _COUNTER_OBJECT.search(before) and (match := _COUNTER_AMOUNT.match(after)):
        return (
            _quantity("counter_amount", value, match["unit"], "up_to")
            if match["limit"]
            else Number("Nat", "counter_amount", value, match["unit"])
        )
    if (
        before.endswith("が")
        and _COUNTER_OBJECT.search(before[:-1])
        and (match := _COUNTER_THRESHOLD.match(after))
    ):
        return Number("Nat", "threshold", value, match["unit"])
    if (
        _COUNTER_OBJECT.search(before)
        and re.match(r"^(?:個|つ)につき、", after)
        and value >= 1
    ):
        return Number("Nat", "group_divisor", value, after[0])
    return None


def _repetition_number(value: int, before: str, after: str) -> Number | None:
    if (event := _repeated_event(value, before, after)) is not None:
        return event
    if (match := _REPEAT.match(after)) and re.search(
        r"(?:これを|下記を|この動作を|同じ操作を)$", before
    ):
        return (
            _quantity("repeat_count", value, "回", "up_to")
            if match["limit"]
            else Number("Nat", "repeat_count", value, "回")
        )
    if (
        (match := _FREQUENCY.match(after))
        and (duration := re.search(r"(?:N|[0-9０-９]+)ターン(?:に|につき)$", before))
        and _frequency_introduction(before[: duration.start()])
    ):
        return (
            _quantity("repeat_count", value, "回", "up_to")
            if match["limit"]
            else Number("Nat", "repeat_count", value, "回")
        )
    if (_DURATION.match(after) and _frequency_introduction(before)) or (
        _EVOLUTION_FREQUENCY.match(after) and before.endswith("自分は{進化}能力を")
    ):
        return Number("Nat", "duration_count", value, "ターン")
    if (
        _EVENT_THRESHOLD.match(after)
        and "このターン" in before
        and before.endswith("が")
    ):
        return Number("Nat", "threshold", value, "回")
    return (
        Number("Nat", "group_divisor", value, "回")
        if value >= 1
        and before.endswith("進化")
        and re.match(r"^回につき使えるEPは(?:N|[0-9０-９]+)つ[）)]", after)
        else None
    )


def _frequency_introduction(before: str) -> bool:
    if before.endswith("この能力は"):
        return True
    condition = re.search(
        r"この能力は(?P<np>[^。:：]+)N(?P<unit>枚|体|つ|回)(?:以上|以下)なら、$", before
    )
    if condition is None:
        return False
    counted = count_context(condition["np"])
    return (
        counted is not None and source_unit(counted, condition["unit"]).merge_allowed
    ) or _event_predicate_unit(condition["np"]) == condition["unit"]


def _repeated_event(value: int, before: str, after: str) -> Number | None:
    if entry := _entry_reminder_number(
        value, before, after
    ) or _additional_trigger_number(value, before, after):
        return entry
    if before.endswith("ドライブチェックを") and re.match(r"^回する" + _END, after):
        return Number("Nat", "repeat_count", value, "回")
    if before.endswith("サイコロを") and re.match(r"^回ふりなおしてよい" + _END, after):
        return Number("Nat", "repeat_count", value, "回")
    if re.search(r"(?:\{食事\})+\{コストN\}:これは$", before) and re.match(
        r"^回出走する" + _END, after
    ):
        return Number("Nat", "repeat_count", value, "回")
    if re.search(r"(?:自分の)?ターンごとに$", before) and re.match(r"^回、", after):
        return Number("Nat", "repeat_count", value, "回")
    return None


def _additional_trigger_number(value: int, before: str, after: str) -> Number | None:
    if before.endswith("自分の能力は追加で") and re.match(r"^回誘発する" + _END, after):
        return Number("Nat", "repeat_count", value, "回")
    return None


def _entry_reminder_number(value: int, before: str, after: str) -> Number | None:
    if re.fullmatch(r"[（(]", before) and re.fullmatch(
        _ENTRY_FREQUENCY + _ENTRY_DIGIT + r"回できる。" + _ENTRY_DIGIT + _ENTRY_DIVISOR,
        after,
    ):
        return Number("Nat", "duration_count", value, "ターン")
    if re.fullmatch(
        r"[（(]" + _ENTRY_DIGIT + _ENTRY_FREQUENCY, before
    ) and re.fullmatch(r"回できる。" + _ENTRY_DIGIT + _ENTRY_DIVISOR, after):
        return Number("Nat", "repeat_count", value, "回")
    if (
        value >= 1
        and re.fullmatch(
            r"[（(]" + _ENTRY_DIGIT + _ENTRY_FREQUENCY + _ENTRY_DIGIT + r"回できる。",
            before,
        )
        and re.fullmatch(_ENTRY_DIVISOR, after)
    ):
        return Number("Nat", "group_divisor", value, "回")
    return None


def _field_number(value: int, before: str, after: str) -> Number | None:
    if field := re.search(r"(コスト|攻撃力|体力)(?:[=:：])?$", before):
        if re.match(r"^(?:以上|以下)(?:の|なら|になるように|である限り)", after):
            return Number("Nat", "threshold", value, None)
        if field_filter(after) or (
            before.endswith("{コスト") and after.startswith("}")
        ):
            return Number(
                "Nat",
                "cost_value" if field[1] == "コスト" else "stat_value",
                value,
                None,
            )
    return None
