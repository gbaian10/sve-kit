"""Finite proposal diagnostics never authorize equivalence or change source text."""

import re
from dataclasses import dataclass
from difflib import SequenceMatcher
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue

from sve_carddb.snapshot.values import canonical, digest

if TYPE_CHECKING:
    from sve_carddb.registry.records import Region
    from sve_carddb.text_observations.models import FaceContent

Category = Literal[
    "whitespace",
    "punctuation",
    "reminder",
    "section_layout",
    "terminology",
    "sentence_pattern",
    "uncovered",
]
_PUNCTUATION = frozenset(".。．,，、;；:：()（）[]【】「」『』!！?？")
_DRAW = re.compile(r"Draw ([1-9]) cards?\.")


@dataclass(frozen=True)
class RuleProposal:
    rule_id: str
    category: Category
    regions: tuple[str, ...]
    fields: tuple[str, ...]
    action: Literal["classify_only", "equivalent"]


PROPOSALS = (
    RuleProposal(
        "wp:eol-v1", "whitespace", ("jp", "en"), ("text", "sections"), "equivalent"
    ),
    RuleProposal(
        "wp:layout-space-v1",
        "whitespace",
        ("jp", "en"),
        ("text", "sections"),
        "classify_only",
    ),
    RuleProposal(
        "wp:punctuation-v1",
        "punctuation",
        ("jp", "en"),
        ("text", "sections"),
        "classify_only",
    ),
    RuleProposal(
        "wp:identified-reminder-v1",
        "reminder",
        ("jp", "en"),
        ("sections",),
        "classify_only",
    ),
    RuleProposal(
        "wp:repartition-v1",
        "section_layout",
        ("jp", "en"),
        ("text", "sections"),
        "classify_only",
    ),
    RuleProposal(
        "wp:term-token-v1", "terminology", ("jp", "en"), ("text",), "classify_only"
    ),
    RuleProposal(
        "wp:draw-plural-v1", "sentence_pattern", ("en",), ("text",), "classify_only"
    ),
)


@dataclass(frozen=True)
class Classification:
    categories: tuple[Category, ...]
    rule_ids: tuple[str, ...]
    differences: tuple[dict[str, JsonValue], ...]

    def report(self) -> dict[str, JsonValue]:
        """Keep exact offsets and hashes while redacting every source character."""
        return {
            "categories": list[JsonValue](self.categories),
            "proposed_rule_ids": list[JsonValue](self.rule_ids),
            "differences": list[JsonValue](self.differences),
            "approval_status": "unapproved",
            "automatic_equivalence": False,
        }


def _fields(content: FaceContent) -> dict[str, str | None]:
    return {"text": content.effect} | {
        f"sections/{i}": text for i, text in enumerate(content.sections)
    }


def _difference(
    before: FaceContent, after: FaceContent
) -> tuple[dict[str, JsonValue], ...]:
    old = before.wording_fields() | _fields(before)
    new = after.wording_fields() | _fields(after)
    del old["sections"], new["sections"]
    result: list[dict[str, JsonValue]] = []
    for field in sorted(old.keys() | new.keys()):
        original, revised = old.get(field), new.get(field)
        if original == revised:
            continue
        item: dict[str, JsonValue] = {
            "field": field,
            "before_hash": digest(canonical(original)),
            "after_hash": digest(canonical(revised)),
        }
        if isinstance(original, str) and isinstance(revised, str):
            item["ranges"] = [
                [a, b, c, d]
                for opcode, a, b, c, d in SequenceMatcher(
                    None, original, revised, autojunk=False
                ).get_opcodes()
                if opcode != "equal"
            ]
        result.append(item)
    return tuple(result)


def _layout_matches(before: FaceContent, after: FaceContent) -> list[str]:
    old, new = _fields(before), _fields(after)
    if old.keys() != new.keys() or any(
        v is None for v in (*old.values(), *new.values())
    ):
        return []
    changes = [(old[k], new[k]) for k in old if old[k] != new[k]]
    if not changes:
        return []
    rules = []
    if all(
        a is not None
        and b is not None
        and a.replace("\r\n", "\n") == b.replace("\r\n", "\n")
        for a, b in changes
    ):
        rules.append("wp:eol-v1")
    if all(
        a is not None
        and b is not None
        and "".join(c for c in a if not c.isspace())
        == "".join(c for c in b if not c.isspace())
        for a, b in changes
    ):
        rules.append("wp:layout-space-v1")
    if all(
        a is not None
        and b is not None
        and "".join(c for c in a if c not in _PUNCTUATION)
        == "".join(c for c in b if c not in _PUNCTUATION)
        for a, b in changes
    ):
        rules.append("wp:punctuation-v1")
    return rules


def _section_matches(
    before: FaceContent, after: FaceContent, reminders: frozenset[int]
) -> list[str]:
    result = []
    if before.effect == after.effect and len(before.sections) == len(after.sections):
        changed = {
            i
            for i, (a, b) in enumerate(
                zip(before.sections, after.sections, strict=True)
            )
            if a != b
        }
        if changed and changed <= reminders:
            result.append("wp:identified-reminder-v1")
    if (
        before.effect is not None
        and after.effect is not None
        and before.sections != after.sections
        and before.effect + "".join(before.sections)
        == after.effect + "".join(after.sections)
    ):
        result.append("wp:repartition-v1")
    return result


def classify(
    before: FaceContent,
    after: FaceContent,
    *,
    region: Region,
    known_reminders: frozenset[int] = frozenset(),
    term_pairs: tuple[tuple[str, str], ...] = (),
) -> Classification:
    """Classify only fully matched finite differences; approvals are a separate input."""
    differences = _difference(before, after)
    if not differences:
        return Classification((), (), ())
    protected_before = before.wording_fields() | {"text": None, "sections": []}
    protected_after = after.wording_fields() | {"text": None, "sections": []}
    if (
        protected_before != protected_after
        or before.effect is None
        or after.effect is None
    ):
        return Classification(("uncovered",), (), differences)
    rules = _layout_matches(before, after) + _section_matches(
        before, after, known_reminders
    )
    if before.sections == after.sections:
        a, b = _DRAW.fullmatch(before.effect), _DRAW.fullmatch(after.effect)
        if region == "en" and a is not None and b is not None and a[1] == b[1]:
            rules.append("wp:draw-plural-v1")
        if any(
            old
            and new
            and old != new
            and before.effect.count(old) == 1
            and before.effect.replace(old, new) == after.effect
            for old, new in term_pairs
        ):
            rules.append("wp:term-token-v1")
    categories = {p.category for p in PROPOSALS if p.rule_id in rules}
    if not rules:
        categories.add("uncovered")
    return Classification(
        tuple(sorted(categories)), tuple(sorted(set(rules))), differences
    )
