"""Synthetic review pin and dispute checks do not create human adoption decisions."""

import re

import pytest
from pydantic import ValidationError

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_translations.review import (
    ModelReview,
    Resolution,
    require_resolved_dispute,
    verify,
)


@pytest.fixture(scope="module")
def review() -> ModelReview:
    return ModelReview(
        translated_by="Synthetic translator v1",
        reviewed_by="Synthetic reviewer v2",
        reviewed_at="2026-10-03T00:00:00Z",
        text_hash=digest(b"Synthetic {{amount}}"),
        result="agreed",
        resolution=None,
    )


def test_final_bytes_and_agreed_review(review: ModelReview) -> None:
    verify("Synthetic {{amount}}", review)
    require_resolved_dispute(review, sampled=False)


@pytest.mark.parametrize(
    "text", ["Synthetic N", "Synthetic {{amount}} ", "Synthetic \\{\\{amount\\}\\}"]
)
def test_draft_or_changed_final_bytes_require_new_review(
    review: ModelReview, text: str
) -> None:
    with pytest.raises(
        ValueError, match=r"^Template model review must pin the final exact text hash$"
    ):
        verify(text, review)


@pytest.mark.parametrize(
    ("change", "message"),
    [
        (
            {"reviewed_by": " Synthetic TRANSLATOR v1 "},
            "Template translation and review require different models",
        ),
        (
            {"translated_by": " "},
            "Template model review must identify both models and versions",
        ),
        (
            {"reviewed_by": " "},
            "Template model review must identify both models and versions",
        ),
        (
            {
                "resolution": {
                    "reviewed_by": "Synthetic human",
                    "reviewed_at": "2026-10-03T00:00:00Z",
                    "note": "Synthetic resolution",
                }
            },
            "Agreed template model review must not carry a dispute resolution",
        ),
    ],
)
def test_bad_model_review_boundaries(
    review: ModelReview, change: dict[str, object], message: str
) -> None:
    raw = {**review.model_dump(mode="json"), **change}
    with pytest.raises(ValidationError, match=re.escape(message)):
        ModelReview.model_validate_json(canonical(raw))


@pytest.mark.parametrize(
    ("resolved", "sampled"), [(False, False), (False, True), (True, False)]
)
def test_dispute_needs_both_human_resolution_and_sample(
    review: ModelReview, resolved: bool, sampled: bool
) -> None:
    dispute = review.model_copy(
        update={
            "result": "disputed",
            "resolution": Resolution(
                reviewed_by="Synthetic human",
                reviewed_at="2026-10-03T00:00:00Z",
                note="Synthetic resolved dispute",
            )
            if resolved
            else None,
        }
    )
    with pytest.raises(
        ValueError,
        match=r"^Disputed template translation requires human resolution and an actual sample$",
    ):
        require_resolved_dispute(dispute, sampled=sampled)


def test_resolved_dispute_still_pins_final_bytes(review: ModelReview) -> None:
    dispute = review.model_copy(
        update={
            "result": "disputed",
            "resolution": Resolution(
                reviewed_by="Synthetic human",
                reviewed_at="2026-10-03T00:00:00Z",
                note="Synthetic resolved dispute",
            ),
        }
    )
    verify("Synthetic {{amount}}", dispute)
    require_resolved_dispute(dispute, sampled=True)


def test_bare_legacy_verdict_cannot_become_formal_review() -> None:
    with pytest.raises(ValidationError, match="Field required"):
        ModelReview.model_validate_json(canonical({"verdict": "ok"}))
