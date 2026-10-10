"""Exact adopted card-name concepts and explicitly proposed vocabulary, never name-to-card guesses."""

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Protocol

from pydantic import JsonValue

from sve_carddb.core.json import digest

if TYPE_CHECKING:
    from sve_carddb.core.provenance import Source
    from sve_carddb.domains.catalog.adoption_models import SourceRef
    from sve_carddb.domains.text_observations.vocabulary import Vocabulary


class Evidence(Protocol):
    def text(self, ref: SourceRef) -> tuple[str, str, Source]:
        """Return a hash-verified complete field and its source language."""
        ...


@dataclass(frozen=True)
class Resolution:
    target: dict[str, JsonValue] | None
    issues: tuple[str, ...]


@dataclass
class References:
    card_names: dict[str, list[str]] = field(default_factory=dict)
    terms: dict[str, list[tuple[str, str]]] = field(default_factory=dict)
    vocabulary: Vocabulary | None = None
    pins: dict[str, JsonValue] = field(default_factory=dict)

    def quoted(self, raw: str) -> Resolution:
        """Only a unique exact adopted Japanese card-name concept supplies a term ID."""
        found = self.card_names.get(raw, [])
        if len(found) != 1:
            reason = (
                "ambiguous_card_name_concept" if found else "missing_card_name_concept"
            )
            return Resolution(None, (reason,))
        identifier = found[0]
        return Resolution({"kind": "term", "id": identifier}, ())

    def proposed_vocabulary(self, kind: str, raw: str) -> Resolution:
        """Maintainer confirmation permits proposals, not an adopted FK or renderer input."""
        if self.vocabulary is None:
            return Resolution(None, ("missing_proposed_vocabulary",))
        try:
            binding = self.vocabulary.lookup("jp", kind, raw)
        except ValueError:
            return Resolution(None, ("unknown_or_ambiguous_vocabulary",))
        target: dict[str, JsonValue] = {
            "kind": "vocabulary",
            "vocabulary_kind": kind,
            "vocabulary_code": binding.code,
            "special_kinds": list(binding.special_kinds),
            "adoption_status": "proposal_only",
        }
        issues = ["vocabulary_not_adopted"]
        if binding.special_kinds:
            issues.append("composite_vocabulary_requires_separate_slots")
        return Resolution(target, tuple(issues))

    def braced_term(self, raw: str) -> Resolution:
        """A registered spelling is a candidate concept; braces alone do not settle semantic role."""
        found = self.terms.get(raw, [])
        if len(found) != 1:
            return Resolution(None, ("ambiguous_or_missing_term_concept",))
        identifier, _ = found[0]
        return Resolution(
            {"kind": "term", "id": identifier},
            ("term_role_requires_review",),
        )

    def header_trait(self, raw: str) -> Resolution:
        """The header grammar plus a unique adopted trait provides both position and category."""
        found = self.terms.get(raw, [])
        if len(found) != 1 or found[0][1] != "trait":
            return Resolution(None, ("unknown_or_ambiguous_header_trait",))
        identifier, _ = found[0]
        return Resolution({"kind": "term", "id": identifier}, ())

    @staticmethod
    def unclassified_header() -> Resolution:
        """Do not infer a trait from a malformed or unknown header prefix."""
        return Resolution(None, ("unrecognized_header_trait_layout",))

    def term_mentions(self, raw: str) -> tuple[dict[str, JsonValue], ...]:
        """Substring hits remain diagnostics; they do not establish a semantic slot role."""
        return tuple(
            {
                "source_name_hash": digest(name.encode()),
                "id": identifier,
                "category": category,
                "reason": "term_mention_requires_semantic_role_review",
            }
            for name in sorted(self.terms)
            if name in raw
            for identifier, category in self.terms[name]
        )
