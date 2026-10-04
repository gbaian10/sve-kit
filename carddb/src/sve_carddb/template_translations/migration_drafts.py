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
