"""Mechanical preferences for current wording, without asserting effect equivalence."""

import re
from dataclasses import dataclass
from typing import Literal
from unicodedata import normalize

Difference = Literal[
    "identical", "reminder_only", "supplemental_sections", "other_wording"
]

_PARENTHESES = re.compile(r"\([^()]*\)")
_JP_KEYWORD = re.compile(
    r"^【(守護|疾走|突進|必殺|ドレイン|指定攻撃|オーラ|威圧|スタック|"
    r"スタートアミュレット|シングルドライブ|ツインドライブ)】を持つ"
)
_EN_KEYWORD = re.compile(
    r"^(?:Followers|Cards) with (Ward|Storm|Rush|Bane|Drain|Quick|Assail|Intimidate|Aura|Drive)\b"
)
_MECHANISMS = (
    ("これが場に出たときか、自分のエンドフェイズ", "【守護】"),
    ("相手は【威圧】", "【威圧】"),
    ("自分のPP最大値が7以上なら、【覚醒】", "【覚醒】"),
    ("自分の墓場のスペルが5枚以上なら、【スペルチェイン", "【スペルチェイン"),
    ("自分の墓場のスペルが7枚以上なら、【スペルチェイン", "【スペルチェイン"),
    ("While you have engaged followers with Ward", "Ward"),
    ("This follower can be engaged", "Ward"),
    ("After this follower fights", "Bane"),
    ("Followers attacking", "Bane"),
    ("When a follower with Drain", "Drain"),
    ("Whenever a follower", "Twin Drive"),
    ("Racing followers have", "race"),
)
_PUNCTUATION = str.maketrans("", "", ".,;:。、，；：")


def _reminder(body: str, context: str) -> bool:
    if match := _JP_KEYWORD.match(body):
        return f"【{match[1]}】" in context
    if match := _EN_KEYWORD.match(body):
        return match[1] in context
    return any(
        body.startswith(prefix) and keyword in context
        for prefix, keyword in _MECHANISMS
    )


def _text(value: str) -> tuple[str, int]:
    value = normalize("NFKC", value)
    count = 0

    def replace(match: re.Match[str]) -> str:
        nonlocal count
        # A keyword must occur outside the parenthesis; a condition alone is not proof.
        if _reminder(match[0][1:-1], value[: match.start()]):
            count += 1
            return ""
        return match[0]

    stripped = _PARENTHESES.sub(replace, value)
    return "".join(stripped.translate(_PUNCTUATION).split()), count


@dataclass(frozen=True)
class Wording:
    """Keep non-wording attributes exact while comparing main and supplemental text."""

    identity: bytes
    text: str
    sections: tuple[str, ...] = ()


def _subsequence(left: tuple[str, ...], right: tuple[str, ...]) -> bool:
    remaining = iter(right)
    return all(any(value == item for value in remaining) for item in left)


def preferred_wordings(
    values: tuple[Wording, ...],
) -> tuple[tuple[int, ...], Difference]:
    """Prefer full supplements and absent reminders only for mechanically equal bodies."""
    indices = tuple(range(len(values)))
    if len(set(values)) <= 1:
        return indices, "identical"
    main = tuple(_text(value.text) for value in values)
    sections = tuple(
        tuple(_text(section) for section in value.sections) for value in values
    )
    if (
        len(
            {
                (value.identity, text[0])
                for value, text in zip(values, main, strict=True)
            }
        )
        != 1
    ):
        return indices, "other_wording"
    normalized = tuple(tuple(text for text, _ in items) for items in sections)
    richest = tuple(
        i
        for i, candidate in enumerate(normalized)
        if all(_subsequence(other, candidate) for other in normalized)
    )
    if not richest:
        return indices, "other_wording"
    counts = tuple(main[i][1] + sum(count for _, count in sections[i]) for i in indices)
    fewest = min(counts[i] for i in richest)
    preferred = tuple(i for i in richest if counts[i] == fewest)
    difference: Difference = (
        "supplemental_sections"
        if len(set(normalized)) > 1
        else "reminder_only"
        if len(set(counts)) > 1
        else "other_wording"
    )
    return preferred, difference


def quote_after(quoted: bool, line: str) -> bool:
    """Carry quoted ability text across lines without borrowing its headers for the owner."""
    quoted ^= bool(line.count('"') % 2)
    opened = max(line.rfind("「"), line.rfind("“"))
    closed = max(line.rfind("」"), line.rfind("”"))
    return opened > closed if opened != closed else quoted
