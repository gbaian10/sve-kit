"""Counted-kind evidence comes from adopted definitions, independently of source units."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.domains.translations.four_layer_units import CountContext
from sve_carddb.domains.translations.glossary.records import TermRecord

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart
    from sve_carddb.domains.translations.inputs import Snapshot
    from sve_carddb.domains.translations.parameters.references import References
    from sve_carddb.domains.translations.sources import Sources

_NAMED = re.compile(r"(?:^|[。:：}】])(?:自分|相手)の場の『(?P<name>X)』(?:を|が)?$")
_TOKEN_TRAIT = re.compile(
    r"(?:^|[。:：}】])(?:自分|相手)の場の(?P<trait>[^。:：・]+)・トークン(?:を|が)?$"
)


@dataclass(frozen=True)
class KindFacts:
    named: tuple[tuple[str, str], ...] = ()
    token_traits: tuple[tuple[str, tuple[str, ...]], ...] = ()

    def context(
        self, raw: str, part: SourcePart, end: int, refs: References
    ) -> CountContext | None:
        """A missing or heterogeneous definition stays unresolved; no classifier supplies kind."""
        before = part.canonical_source[:end]
        if match := _NAMED.search(before):
            spelling = "".join(
                raw[s.start : s.end] for s in part.units[match.start("name")].origins
            )
            resolution = refs.quoted(spelling)
            if resolution.target is None:
                return None
            term_id = resolution.target.get("id")
            kinds = {kind for identifier, kind in self.named if identifier == term_id}
            if len(kinds) == 1:
                return CountContext(
                    "select.unrestricted.v1",
                    kinds.pop(),
                    ("battlefield",),
                    frozenset({False, True}),
                    "cardinality",
                )
        if (match := _TOKEN_TRAIT.search(before)) and refs.vocabulary is not None:
            try:
                trait = refs.vocabulary.lookup("jp", "trait", match["trait"])
            except ValueError:
                return None
            kinds = {
                kind
                for code, members in self.token_traits
                if code == trait.code
                for kind in members
            }
            if len(kinds) == 1:
                return CountContext(
                    "select.card.v1", kinds.pop(), ("battlefield",), True, "cardinality"
                )
        return None


@dataclass(frozen=True)
class _Catalog:
    revisions: tuple[Mapping[str, Value], ...]
    faces: Mapping[Value, Mapping[str, Value]]
    units: Mapping[Value, Mapping[str, Value]]
    active: frozenset[Value]

    def kind(self, value: Value) -> str:
        if not isinstance(value, str) or value not in self.active:
            raise ValueError("Counted-kind definition lacks an active type reference")
        return value


def _catalog(db: Database) -> _Catalog:
    revisions = tuple(
        r.values for r in db.rows("face_revision") if r.values["region"] == "jp"
    )
    faces = {r.values["id"]: r.values for r in db.rows("face")}
    cards = {r.values["id"]: r.values for r in db.rows("card")}
    units = {r.values["id"]: r.values for r in db.rows("text_unit")}
    active = frozenset(
        r.values["code"]
        for r in db.rows("vocabulary")
        if r.values["kind"] == "type" and r.values["active"] is True
    )
    confirmed = tuple(
        r
        for r in revisions
        if cards[faces[r["face_id"]]["card_id"]]["identity_state"] == "confirmed"
    )
    return _Catalog(confirmed, faces, units, active)


def _named_kinds(
    catalog: _Catalog, snapshot: Snapshot, sources: Sources
) -> tuple[tuple[str, str], ...]:
    named: set[tuple[str, str]] = set()
    for record in snapshot.current_records():
        if not isinstance(record, TermRecord) or record.data.category != "card_name":
            continue
        data = record.data
        ref = data.source_ref
        if (
            ref is None
            or data.source_span is not None
            or ref.parser != "translation-jp-v1"
        ):
            continue
        location = re.fullmatch(r"/faces/(?P<ordinal>0|[1-9][0-9]*)/name", ref.locator)
        if location is None:
            continue
        candidates = tuple(
            revision
            for revision in catalog.revisions
            if revision["source_id"] == ref.source_version_id
            and catalog.faces[revision["face_id"]]["ordinal"]
            == int(location["ordinal"])
        )
        if not candidates:
            continue
        lang, spelling, source = sources.text(ref)
        if lang != "ja" or source.id != ref.source_version_id:
            raise ValueError("Named counted-kind source must be Japanese")
        for revision in candidates:
            unit = catalog.units[revision["name_unit_id"]]
            if unit["text"] == spelling:
                named.add((data.id, catalog.kind(revision["type_code"])))
    return tuple(sorted(named))


def adopted_kinds(db: Database, snapshot: Snapshot, sources: Sources) -> KindFacts:
    """Use the current build's explicit type/token/trait fields without narrowing reference domains."""
    catalog = _catalog(db)
    tokens = {
        r.values["revision_id"]
        for r in db.rows("face_special_kind")
        if r.values["special_kind_code"] == "token"
    }
    traits: dict[str, set[str]] = {}
    token_kinds = {
        r["id"]: catalog.kind(r["type_code"])
        for r in catalog.revisions
        if r["id"] in tokens
    }
    for row in db.rows("face_trait"):
        kind = token_kinds.get(row.values["revision_id"])
        trait = row.values["trait_code"]
        if kind is not None and isinstance(trait, str):
            traits.setdefault(trait, set()).add(kind)
    return KindFacts(
        _named_kinds(catalog, snapshot, sources),
        tuple((trait, tuple(sorted(kinds))) for trait, kinds in sorted(traits.items())),
    )
