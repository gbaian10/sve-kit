"""Exact raw-field bindings derived from adoptions or pinned for synthetic staging."""

from sve_carddb.catalog.models import Term
from sve_carddb.core.models import RecordData
from sve_carddb.products.models import Code
from sve_carddb.registry.records import Region


class Binding(RecordData):
    region: Region
    kind: Code
    raw: str
    code: Code
    special_kinds: tuple[Code, ...] = ()


class Vocabulary(RecordData):
    bindings: tuple[Binding, ...]
    terms: tuple[Term, ...] = ()

    def lookup(self, region: Region, kind: str, raw: str) -> Binding:
        """Fail on unknown or ambiguous fields rather than deriving codes from text."""
        found = [
            item
            for item in self.bindings
            if (item.region, item.kind, item.raw) == (region, kind, raw)
        ]
        if len(found) != 1:
            candidates = sorted(
                {item.code for item in self.bindings if item.kind == kind}
            )
            raise ValueError(
                f"Missing or ambiguous explicit vocabulary binding: "
                f"kind={kind!r}, region={region!r}, raw={raw!r}, candidates={candidates!r}; "
                "new spellings require maintainer confirmation"
            )
        return found[0]

    def verify(self) -> None:
        """Require unique raw keys and declared special-kind references."""
        keys = [(item.region, item.kind, item.raw) for item in self.bindings]
        if len(set(keys)) != len(keys):
            raise ValueError("Duplicate vocabulary binding")
        term_keys = [(term.kind, term.code) for term in self.terms]
        if len(set(term_keys)) != len(term_keys):
            raise ValueError("Duplicate derived vocabulary term")
        codes = (
            {(term.kind, term.code) for term in self.terms if term.active}
            if self.terms
            else {(item.kind, item.code) for item in self.bindings}
        )
        for item in self.bindings:
            if self.terms and (item.kind, item.code) not in codes:
                raise ValueError("Derived binding lacks an active vocabulary term")
            if item.kind != "type" and item.special_kinds:
                raise ValueError("Special kinds belong to type bindings")
            if any(("special_kind", code) not in codes for code in item.special_kinds):
                raise ValueError("Special-kind vocabulary reference is missing")
