"""Immutable configuration pins for report-only text staging."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import canonical, digest

if TYPE_CHECKING:
    from sve_carddb.products.models import LocalizedText
    from sve_carddb.text_observations.plan import TextPlan
    from sve_carddb.text_observations.vocabulary import Vocabulary


def text_configuration(
    plan: TextPlan, vocabulary: Vocabulary, published: tuple[LocalizedText, ...]
) -> dict[str, JsonValue]:
    """Explicitly bind vocabulary, exact candidate plan, and the historical text union."""
    return {
        "text_observations": plan.configuration(),
        "text_vocabulary": canonical(vocabulary.model_dump(mode="json")).decode(),
        "published_text_hash": digest(
            canonical(
                list[JsonValue](
                    sorted(
                        {
                            canonical(text.model_dump(mode="json")).decode()
                            for text in published
                        }
                    )
                )
            )
        ),
    }
