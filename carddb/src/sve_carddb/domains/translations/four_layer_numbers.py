"""N0 numeric roles require source constructions, not generic suffix hints."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from sve_carddb.contracts.four_layer import Constant, QuantitySpec

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart
    from sve_carddb.domains.translations.parameters.models import Hint

_ROLES = {
    "damage_amount": "damage_amount",
    "recovery_amount": "recovery_amount",
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
_UNIT = re.compile(
    r"^(ターン|種類|ダメージ|ＳＥＰ|SEP|ＰＰ|PP|ＥＰ|EP|枚|体|つ|点|回(?!復)|人|個|倍|番)"
)
_END = r"(?=[。:、）)\n]|$)"
_SELECT = re.compile(
    r"^(?P<unit>枚|体|つ|人)(?P<limit>まで)?(?:を)?選(?:ぶ|び|んで)(?=[。:、）)\n]|$)"
)
_EXIST = re.compile(
    r"^(?P<unit>枚|体|つ|人)(?P<compare>以上|以下)?(?:あるなら|いるなら|ある|いる|なら|である限り|であれば)"
)
_MOVE = re.compile(
    r"^枚(?:を)?(?:引く|引き|引いて|捨てる|捨て|戻す|戻し|加える|加え|置く|置き|出す|出し|消滅させる)(?=[。:、）)\n]|$)"
)
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
    raw_after = raw[hint.source_segments[-1].end :]
    matched = _UNIT.match(raw_after)
    unit = matched[1] if matched is not None else None
    role = hint.semantic_role
    if role in _ROLES:
        return Number("Nat", _ROLES[role], hint.value, unit)
    return _constructed_number(hint, before, after, unit)


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
    if re.match(r"^つ(?:まで)?チョイス" + _END, after):
        return _quantity(
            "choice_mode_count",
            value,
            "つ",
            "up_to" if after.startswith("つまで") else "exact",
        )
    if _MOVE.match(after):
        return Number("Nat", "count", value, "枚")
    return _field_number(value, before, after)


def _field_number(value: int, before: str, after: str) -> Number | None:
    if field := re.search(r"(コスト|攻撃力|体力)(?:[=:：])?$", before):
        if re.match(r"^(?:以上|以下)(?:の|なら|になるように|である限り)", after):
            return Number("Nat", "threshold", value, None)
        if re.match(r"^の(?:カード|フォロワー|アミュレット|スペル)", after) or (
            before.endswith("{コスト") and after.startswith("}")
        ):
            return Number(
                "Nat",
                "cost_value" if field[1] == "コスト" else "stat_value",
                value,
                None,
            )
    return None
