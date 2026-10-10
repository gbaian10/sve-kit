"""Finite lexical proposals share the four-layer source partition and trace."""

from typing import TYPE_CHECKING

from sve_carddb.contracts.template_parameters import Range
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.recognition.analysis import analyze
from sve_carddb.domains.translations.recognition.candidate_matching import recognize
from sve_carddb.domains.translations.recognition.lexical import Part
from sve_carddb.domains.translations.recognition.references import References

from .test_four_layer_classification import source

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.domains.translations.recognition.models import Candidate


def partition(text: str, *, section: int | None = None) -> tuple[Part, ...]:
    field = normalize_source(
        text, source(text, "effect" if section is None else "section")
    )
    result = []
    for part in field.parts:
        role = part.source_span.role
        assert role in {"body", "reminder", "token_header", "layout"}
        result.append(
            Part(
                role,
                tuple(
                    Range(start=s.start, end=s.end) for s in part.source_span.segments
                ),
                part.canonical_source,
                part.units,
            )
        )
    return tuple(result)


def candidate(
    text: str, refs: References | None = None, *, section: int | None = None
) -> Candidate:
    return analyze(text, partition(text, section=section)[0], refs or References())


def matches(
    text: str, rule: str, refs: References | None = None
) -> tuple[dict[str, JsonValue], ...]:
    evidence = refs or References()
    return recognize(
        text, partition(text)[0], candidate(text, evidence), evidence, (rule,)
    )
