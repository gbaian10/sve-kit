"""N0 numeric roles require source constructions, not generic suffix hints."""

import re
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING, Literal

from sve_carddb.contracts.four_layer import Constant, QuantitySpec
from sve_carddb.domains.translations.four_layer_units import count_context, source_unit
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
_END = r"(?=[。:、）)\n]|$)"
_SELECT = re.compile(
    r"^(?P<unit>枚|体|つ|人)(?P<limit>まで)?(?:を)?選(?:ぶ|び|んで)(?=[。:、）)\n]|$)"
)
_EXIST = re.compile(
    r"^(?P<unit>枚|体|つ|人)(?P<compare>以上|以下)?(?:あるなら|いるなら|ある限り|いる限り|なら|である限り|であれば|ある|いる)"
    + _END
)
_MOVE = re.compile(
    r"^枚(?:を)?(?:引く|引き|引いて|捨てる|捨て|戻す|戻し|加える|加え|置く|置き|出す|出し|消滅させる)(?=[。:、）)\n]|$)"
)
_TRANSFER = re.compile(
    r"^(?:枚|体|つ)(?:まで)?(?:を)?(?:、)?(?:裏向きで)?(?:自分の|相手の)?"
    r"(?:手札に(?:加える|加え|加えて)|墓場に(?:置く|置き|置いて)|"
    r"デッキの(?:上|下)に(?:置く|置き|置いて)|EXエリアに(?:置く|置き|置いて)|"
    r"場に(?:出す|出し|出して))(?:よい)?" + _END
)
_LOOK = re.compile(r"^枚(?:を)?見(?:る|て)" + _END)
_REVEAL = re.compile(
    r"^枚(?P<limit>まで)?(?:を)?公開して(?:、)?"
    r"(?:それを|そのカードを)?(?:手札に加える|手札に加えてよい|墓場に置く|場に出す)"
    + _END
)
_CHOICE = re.compile(r"^つ(?P<limit>まで)?チョイス(?:する|して)?" + _END)
_ABILITY_COUNT = re.compile(r"^つ(?:を)?(?:持つ|持ち|持っている)" + _END)
_REPEAT = re.compile(
    r"^回(?P<limit>まで)?(?:を)?(?:行う|行い|行って|繰り返す|くり返す|使える|使用できる)"
    + _END
)
_FREQUENCY = re.compile(r"^回(?P<limit>まで)?(?:使える|使用できる|働く)" + _END)
_DURATION = re.compile(
    r"^ターン(?:に|につき)(?:N|[0-9０-９]+)回(?:まで)?(?:使える|使用できる|働く)" + _END
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
    r"^(?P<unit>個|つ)(?:以上|以下)(?:なら|である限り)" + _END
)
_EVENT_THRESHOLD = re.compile(
    r"^回以上(?:攻撃した|攻撃していた|発動していた)なら" + _END
)
_EVOLUTION_FREQUENCY = re.compile(r"^ターンに何回でも使える" + _END)
_SACRIFICE = re.compile(
    r"^(?P<unit>枚|体|つ)(?:を)?(?:墓場に置く|アクトする|レストする|破壊する)" + _END
)
_LEADER = re.compile(r"^人(?:に(?:N|[0-9０-９]+)ダメージ|の(?:手札|墓場|場|デッキ))")
_ORDINAL = {
    "card_ordinal": ("card_index", "枚"),
    "repetition_ordinal": ("repeat_index", "回"),
    "turn_ordinal": ("turn_index", "ターン"),
    "deck_top_ordinal": ("deck_index", "番"),
}
_MIN_OPTIONS = 2


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
        counted = count_context(before)
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
            and (counted := count_context(before)) is not None
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
            if _choice_index(raw, hint)
            else None
        )
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
        new_role, expected_unit = _ORDINAL[role]
        introduction = re.search(
            r"(?:第|このターン中に|ターン中に|デッキの上から)$", before
        )
        if (
            hint.value >= 1
            and introduction is not None
            and after.startswith(expected_unit + "目")
        ):
            return Number("Ordinal", new_role, hint.value, expected_unit)
        return None
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


def _generic_number(
    value: int, before: str, after: str, unit: str | None
) -> Number | None:
    if counter := _counter_number(value, before, after):
        return counter
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


def _action_number(value: int, before: str, after: str) -> Number | None:
    if _MOVE.match(after) or _TRANSFER.match(after) or _LOOK.match(after):
        unit = after[0]
        if after.startswith(unit + "まで"):
            return _quantity("selection_count", value, unit, "up_to")
        return Number("Nat", "count", value, unit)
    if match := _REVEAL.match(after):
        return _quantity(
            "selection_count", value, "枚", "up_to" if match["limit"] else "exact"
        )
    if _ABILITY_COUNT.match(after) and re.search(r"(?:能力を|能力が|能力)$", before):
        return Number("Nat", "count", value, "つ")
    return (
        _payment_number(value, before, after)
        or _repetition_number(value, before, after)
        or _field_number(value, before, after)
    )


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
    if (match := _REPEAT.match(after)) and re.search(
        r"(?:これを|下記を|この動作を|同じ操作を)$", before
    ):
        return (
            _quantity("repeat_count", value, "回", "up_to")
            if match["limit"]
            else Number("Nat", "repeat_count", value, "回")
        )
    if (match := _FREQUENCY.match(after)) and re.search(
        r"この能力は(?:N|[0-9０-９]+)ターン(?:に|につき)$", before
    ):
        return (
            _quantity("repeat_count", value, "回", "up_to")
            if match["limit"]
            else Number("Nat", "repeat_count", value, "回")
        )
    if (_DURATION.match(after) and before.endswith("この能力は")) or (
        _EVOLUTION_FREQUENCY.match(after) and before.endswith("自分は{進化}能力を")
    ):
        return Number("Nat", "duration_count", value, "ターン")
    if (
        _EVENT_THRESHOLD.match(after)
        and "このターン" in before
        and before.endswith("が")
    ):
        return Number("Nat", "threshold", value, "回")
    if (
        value >= 1
        and before.endswith("進化")
        and re.match(r"^回につき使えるEPは(?:N|[0-9０-９]+)つ[）)]", after)
    ):
        return Number("Nat", "group_divisor", value, "回")
    return None


def _choice_index(raw: str, hint: Hint) -> bool:
    """One complete field proves ordered explicit indices without a cross-frame port."""
    protected = re.sub(r"『[^』]*』", lambda m: " " * len(m[0]), raw)
    protected = re.sub(r"[（(][^（）()]*[）)]", lambda m: " " * len(m[0]), protected)
    introduction = tuple(
        re.finditer(
            r"(?<![A-Za-z0-9０-９])(?P<count>[0-9０-９]+)つ(?:まで)?チョイス(?:する|して)?[。:：]",
            protected,
        )
    )
    if len(introduction) != 1:
        return False
    intro = introduction[0]
    tail = protected[intro.end() :]
    # A fresh ability head or blank paragraph cannot inherit another ability's intro.
    boundary = re.search(
        r"\n\s*\n|(?:^|[。\n])\s*(?:\{(?:ファンファーレ|起動|ラストワード|進化|食事|憑依)\}|"
        r"【(?:進化時|攻撃時|超進化時)】)",
        tail,
    )
    stop = intro.end() + boundary.start() if boundary is not None else len(raw)
    labels = tuple(re.finditer(r"【([0-9０-９]+)】", protected[intro.end() : stop]))
    if (
        len(labels) < _MIN_OPTIONS
        or not 1 <= int(intro["count"]) <= len(labels)
        or [int(m[1]) for m in labels] != list(range(1, len(labels) + 1))
    ):
        return False
    if re.search(r"【[0-9０-９]+】", protected[: intro.start()]):
        return False
    return any(
        len(hint.source_segments) == 1
        and hint.source_segments[0].start == intro.end() + label.start(1)
        and hint.source_segments[0].end == intro.end() + label.end(1)
        for label in labels
    )


def _field_number(value: int, before: str, after: str) -> Number | None:
    if field := re.search(r"(コスト|攻撃力|体力)(?:[=:：])?$", before):
        if re.match(r"^(?:以上|以下)(?:の|なら|になるように|である限り)", after):
            return Number("Nat", "threshold", value, None)
        if re.match(r"^(?:の)?(?:カード|フォロワー|アミュレット|スペル)", after) or (
            before.endswith("{コスト") and after.startswith("}")
        ):
            return Number(
                "Nat",
                "cost_value" if field[1] == "コスト" else "stat_value",
                value,
                None,
            )
    return None
