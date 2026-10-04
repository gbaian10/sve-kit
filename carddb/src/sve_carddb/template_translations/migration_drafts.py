"""Read private draft inputs without publishing source wording or review receipts."""

from dataclasses import dataclass

from pydantic import ValidationError

from sve_carddb.snapshot.values import object_value, parse
from sve_carddb.template_translations.preparation import Draft


@dataclass(frozen=True)
class EffectDraft:
    draft: Draft
    low_confidence: bool


def effect_drafts(drafts: bytes, reviews: bytes) -> tuple[EffectDraft, ...]:
    """Join by permanent legacy ID; old review verdicts describe confidence, not consent."""
    verdicts: dict[str, str] = {}
    for line in reviews.splitlines():
        row = object_value(parse(line))
        identifier, verdict = row.get("template"), row.get("verdict")
        if (
            not isinstance(identifier, str)
            or verdict not in {"ok", "suggest", "unsure"}
            or identifier in verdicts
        ):
            raise ValueError("Invalid or duplicate template draft review")
        verdicts[identifier] = str(verdict)
    result = {}
    for line in drafts.splitlines():
        row = object_value(parse(line))
        try:
            draft = Draft.model_validate(
                {
                    key: row.get(key)
                    for key in ("template", "normalized", "zh", "confidence", "note")
                },
                strict=True,
            )
        except ValidationError:
            raise ValueError("Invalid template translation draft") from None
        if draft.template in result:
            raise ValueError("Duplicate template translation draft")
        if draft.template not in verdicts:
            raise ValueError("Template draft review coverage differs from drafts")
        result[draft.template] = EffectDraft(
            draft,
            draft.confidence == "low" or verdicts[draft.template] == "unsure",
        )
    if set(result) != set(verdicts):
        raise ValueError("Template draft review coverage differs from drafts")
    return tuple(result[key] for key in sorted(result))


@dataclass(frozen=True)
class FlavorDraft:
    identifier: str
    source_hash: str
    source_text: str
    text: str
    low_confidence: bool


def flavor_drafts(raw: bytes) -> tuple[FlavorDraft, ...]:
    """Exact whole paragraphs remain private inputs; their hashes locate current fields."""
    from sve_carddb.snapshot.values import digest  # ruff: ignore[import-outside-top-level] -- flavor draft hashes are independent of N/X recipes

    result = {}
    for line in raw.splitlines():
        row = object_value(parse(line))
        identifier = row.get("flavor_id")
        source, text, confidence = (
            row.get("source_text"),
            row.get("zh_hant"),
            row.get("confidence"),
        )
        if (
            not isinstance(identifier, str)
            or not isinstance(source, str)
            or not isinstance(text, str)
            or confidence not in {"high", "medium", "low"}
        ):
            raise ValueError("Invalid flavor translation draft")
        try:
            checksum = digest(source.encode())
            text.encode()
        except UnicodeError:
            raise ValueError("Invalid flavor translation draft") from None
        if identifier != checksum[7:23] or identifier in result:
            raise ValueError("Duplicate or mismatched flavor draft fingerprint")
        result[identifier] = FlavorDraft(
            identifier, checksum, source, text, confidence == "low"
        )
    return tuple(result[key] for key in sorted(result))
