"""Signs stay literal; magnitudes retain distinct resource, direction and damage roles."""

from dataclasses import dataclass


@dataclass(frozen=True)
class SignedContext:
    role: str
    before: str
    after: str
    target: str | None = None


RESOURCE_END = r"^(?:する(?=[。:、）)])|できない(?=[。:、）)])|:)"
DAMAGE_END = r"^する(?=[。:、）)」])"
SIGNED_CONTEXTS = {
    "pp_capacity_delta": SignedContext(
        "pp_capacity_delta_magnitude",
        r"(?:自分の|(?:^|[、。:}】]|フェイズに))PP最大値を[+-]$",
        RESOURCE_END,
    ),
    "pp_delta": SignedContext("pp_delta_magnitude", r"自分のPPを[+-]$", RESOURCE_END),
    "ep_delta": SignedContext("ep_delta_magnitude", r"自分のEPを[+-]$", RESOURCE_END),
    "stack_delta": SignedContext(
        "stack_delta_magnitude",
        r"自分の場の【(?P<term>スタック)】を[+-]$",
        r"^する(?=[。:、）)])",
        "term:ability.stack",
    ),
}
SIGNED_CONTEXTS.update(
    {
        f"{direction}_{kind}_damage_delta": SignedContext(
            f"{direction}_{kind}_damage_delta_magnitude",
            r"が(?:次に)?" + verb + damage_type + r"ダメージを[+-]$",
            DAMAGE_END,
        )
        for direction, verb in (("dealt", "与える"), ("received", "受ける"))
        for kind, damage_type in (("all", ""), ("ability", "能力"), ("combat", "交戦"))
    }
)
