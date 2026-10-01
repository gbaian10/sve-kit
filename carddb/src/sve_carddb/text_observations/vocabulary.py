"""Explicit raw-field bindings supplied and pinned by the staging build caller."""

from sve_carddb.products.models import Code  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves constrained aliases at runtime
from sve_carddb.registry.records import RecordData, Region


class Binding(RecordData):
    region: Region
    kind: Code
    raw: str
    code: Code
    special_kinds: tuple[Code, ...] = ()


class Vocabulary(RecordData):
    bindings: tuple[Binding, ...]

    def lookup(self, region: Region, kind: str, raw: str) -> Binding:
        """Fail on unknown or ambiguous fields rather than deriving codes from text."""
        found = [
            item
            for item in self.bindings
            if (item.region, item.kind, item.raw) == (region, kind, raw)
        ]
        if len(found) != 1:
            raise ValueError("Missing or ambiguous explicit vocabulary binding")
        return found[0]

    def verify(self) -> None:
        """Require unique raw keys and declared special-kind references."""
        keys = [(item.region, item.kind, item.raw) for item in self.bindings]
        if len(set(keys)) != len(keys):
            raise ValueError("Duplicate vocabulary binding")
        codes = {(item.kind, item.code) for item in self.bindings}
        for item in self.bindings:
            if item.kind != "type" and item.special_kinds:
                raise ValueError("Special kinds belong to type bindings")
            if any(("special_kind", code) not in codes for code in item.special_kinds):
                raise ValueError("Special-kind vocabulary reference is missing")
