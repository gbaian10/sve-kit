"""An alias has a closed raw spelling and still needs the registered full ability."""

from dataclasses import dataclass


@dataclass(frozen=True)
class KeywordAlias:
    spelling: str
    full_name: str
    target: str
    role: str


KEYWORD_ALIASES = {
    "keyword_alias_nc": KeywordAlias(
        "NC", "ネクロチャージ", "term:ability.necrocharge", "necrocharge_threshold"
    ),
    "keyword_alias_sc": KeywordAlias(
        "SC", "スペルチェイン", "term:ability.spell_chain", "spell_chain_threshold"
    ),
}
