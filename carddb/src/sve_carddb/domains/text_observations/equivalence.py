"""Conservative wording comparisons and exact-content authored decisions."""

import re
from typing import TYPE_CHECKING, Literal
from unicodedata import normalize

from pydantic import JsonValue, model_validator

from sve_carddb.core.authored import shards
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.core.regions import Region

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.text_observations.models import FaceContent

Classification = Literal["same_effect", "pending_review", "errata", "unresolved"]
Method = Literal["mechanical", "ai", "manual", "none"]

# Only keyword-attached explanations can disappear; arbitrary conditions cannot.
_JP_REMINDER = re.compile(
    r"(【(?:守護|疾走|突進|必殺|ドレイン|指定攻撃|オーラ|威圧)】)[。\s]*"
    r"\((?:【[^】]+】を持つ|これが場に出たときか|相手は【威圧】)[^()]*\)"
)
_EN_REMINDER = re.compile(
    r"\b(Ward|Storm|Rush|Bane|Drain|Quick|Assail|Intimidate|Aura)\.?\s*"
    r"\((?:Followers with|Cards with|While you have engaged followers with Ward|"
    r"This follower can be engaged|After this follower fights)[^()]*\)"
)
_PUNCTUATION = str.maketrans("", "", ".,;:。、，；：")


def comparison_text(text: str) -> str:
    """Ignore spacing, sentence punctuation and keyword-attached reminder prose."""
    text = normalize("NFKC", text)
    text = _JP_REMINDER.sub(r"\1", text)
    text = _EN_REMINDER.sub(r"\1", text)
    return "".join(normalize("NFKC", text).translate(_PUNCTUATION).split())


def equivalent(contents: tuple[FaceContent, ...]) -> bool:
    """Require all non-effect fields exact and all rule bodies mechanically equal."""
    if not contents or any(content.effect is None for content in contents):
        return False
    values = []
    for content in contents:
        value = content.wording_fields()
        value["text"] = comparison_text(content.effect or "")
        value["sections"] = [comparison_text(section) for section in content.sections]
        values.append(canonical(value))
    return len(set(values)) == 1


def comparison_hash(contents: tuple[FaceContent, ...]) -> str:
    """Pin the complete set of variants so decisions expire when wording changes."""
    return digest(
        canonical(
            list[JsonValue](sorted({content.fingerprint() for content in contents}))
        )
    )


class WordingDecision(RecordData):
    face_id: Text
    region: Region
    comparison_hash: Hash
    classification: Classification
    method: Literal["ai", "manual"]
    reason: Text
    basis_printing_id: Text | None = None

    @model_validator(mode="after")
    def basis(self) -> WordingDecision:
        """Reserve chronology overrides for explicit manual decisions."""
        if self.basis_printing_id is not None and self.method != "manual":
            raise ValueError("Only manual decisions may override the release basis")
        if self.basis_printing_id is not None and self.classification != "same_effect":
            raise ValueError("Only equivalent wording may select a shared basis")
        return self


class _Shard(RecordData):
    format: Literal[1]
    kind: Literal["wording_decisions"]
    records: tuple[WordingDecision, ...]


def load_decisions(root: Path) -> tuple[WordingDecision, ...]:
    """Read only bounded wording shards, rejecting duplicate face/region decisions."""
    result: dict[tuple[str, Region], WordingDecision] = {}
    for _, _, content in shards(root, ("wording",), optional=("wording",)):
        for decision in _Shard.model_validate_json(content).records:
            key = decision.face_id, decision.region
            if key in result:
                raise ValueError("Duplicate wording decision")
            result[key] = decision
    return tuple(value for _, value in sorted(result.items()))
