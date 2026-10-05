"""Current catalog references reuse its validated permanent codes, not old proposals."""

from dataclasses import dataclass
from typing import override

from sve_carddb.template_parameters.references import References as LegacyReferences
from sve_carddb.template_parameters.references import Resolution


@dataclass
class References(LegacyReferences):
    def __post_init__(self) -> None:
        """Proposal-only bindings cannot become current reference targets."""
        if self.vocabulary is not None:
            self.vocabulary.verify()
            if self.vocabulary.bindings and not self.vocabulary.terms:
                raise ValueError("Current references require a derived active catalog")

    @override
    def quoted(self, raw: str) -> Resolution:
        """A recognized name without a concept keeps its spelling; ambiguity stays unresolved."""
        result = super().quoted(raw)
        if result.issues == ("missing_card_name_concept",):
            return Resolution({"kind": "term", "card_name": raw}, ())
        return result

    @override
    def proposed_vocabulary(self, kind: str, raw: str) -> Resolution:
        """The existing analyzer calls this hook; only verified current codes resolve it."""
        if self.vocabulary is None:
            return Resolution(None, ("missing_current_vocabulary",))
        try:
            binding = self.vocabulary.lookup("jp", kind, raw)
        except ValueError:
            return Resolution(None, ("unknown_or_ambiguous_vocabulary",))
        return Resolution(
            {
                "kind": "vocabulary",
                "vocabulary_kind": kind,
                "vocabulary_code": binding.code,
                "special_kinds": list(binding.special_kinds),
            },
            ("composite_vocabulary_requires_separate_slots",)
            if binding.special_kinds
            else (),
        )
