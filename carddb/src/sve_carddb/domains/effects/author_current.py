"""Admit author cards against current identities and their native rule lines."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_direct import inventory, materialize
from sve_carddb.domains.effects.author_inputs import (
    AuthorInputError,
    Diagnostic,
    SourceError,
)
from sve_carddb.domains.effects.author_lint import lint
from sve_carddb.domains.effects.author_resolution import handwriting
from sve_carddb.domains.effects.author_set import validate_author_set
from sve_carddb.domains.registry.records import (
    CardData,
    MappingReviewData,
    PrintingData,
)

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Row, Value
    from sve_carddb.contracts.author_leaves import AuthorIndexRecord
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.effects.author_expansion import AuthorExpander
    from sve_carddb.domains.effects.author_set import AuthorInputSet
    from sve_carddb.domains.registry.snapshot import RegistrySnapshot


@dataclass(frozen=True)
class CardAccepted:
    record: AuthorIndexRecord
    tree: dict[str, JsonValue]
    source_payload: dict[str, JsonValue]

    def lowering_input(self) -> JsonValue:
        """Serialize the direct author tree with its current identity and lines."""
        return {
            "admission": "direct",
            "tree": self.tree,
            "card_id": self.record.card_id,
            "card_no": self.record.record_key,
            "source_payload": self.source_payload,
        }


@dataclass(frozen=True)
class CardRejected:
    record: AuthorIndexRecord
    diagnostic: Diagnostic


type CardResult = CardAccepted | CardRejected


def _text(value: Value) -> str:
    if not isinstance(value, str) or not value:
        raise SourceError("source_database_shape")
    return value


def _one(db: Database, table: str, where: Mapping[str, Value]) -> Mapping[str, Value]:
    rows = db.select(table, db.columns(table), where=where)
    if len(rows) != 1:
        raise SourceError("source_current_missing")
    return rows[0].values


def english_only_cards(registry: RegistrySnapshot) -> frozenset[str]:
    """Require a reviewed absence and check against every Japanese printing."""
    japanese: set[str] = set()
    english: set[str] = set()
    confirmed: set[str] = set()
    reviewed: set[str] = set()
    for record in registry.records.values():
        data = record.data
        if isinstance(data, PrintingData):
            (japanese if data.region == "jp" else english).add(data.card_id)
        if isinstance(data, CardData) and data.identity_state == "confirmed":
            confirmed.add(data.id)
        if isinstance(data, MappingReviewData) and data.target_region == "jp":
            reviewed.add(data.card_id)
    return frozenset((reviewed & english & confirmed) - japanese)


class CurrentAuthorSources:
    """Resolve whole-card author coordinates from the selected build."""

    def __init__(
        self, db: Database, english_cards: frozenset[str] = frozenset()
    ) -> None:
        self.db = db
        self.english_cards = english_cards
        self._source_printings: tuple[int, dict[Value, list[Row]]] | None = None

    def source_region(self, card_id: str) -> Region:
        """Use EN only for identities with a confirmed absence of JP printings."""
        return "en" if card_id in self.english_cards else "jp"

    def _printings(self, number: str) -> list[Row]:
        # One scan per database revision replaces a full table scan per lookup.
        revision = self.db.revision()
        if self._source_printings is None or self._source_printings[0] != revision:
            numbers: dict[Value, list[Row]] = {}
            for row in self.db.select(
                "printing", ("id", "card_id", "card_no", "region", "card_no_state")
            ):
                if row.values["region"] == self.source_region(
                    _text(row.values["card_id"])
                ):
                    numbers.setdefault(row.values["card_no"], []).append(row)
            self._source_printings = revision, numbers
        return self._source_printings[1].get(number, [])

    def printing_card(self, number: str) -> str:
        """Resolve an official source printing only when all rows agree on identity."""
        rows = self._printings(number)
        identities = {_text(row.values["card_id"]) for row in rows}
        if len(identities) != 1:
            raise SourceError("record_key_identity_mismatch")
        if any(row.values["card_no_state"] != "official" for row in rows):
            raise SourceError("provisional_record_key")
        return identities.pop()

    def printing_id(self, number: str, *, region: Region | None = None) -> str:
        """Bind a program dependency or representative to one official printing."""
        rows = [
            row
            for row in self._printings(number)
            if row.values["card_no_state"] == "official"
            and (region is None or row.values["region"] == region)
        ]
        if len(rows) != 1:
            raise ValueError(f"Program printing is missing or ambiguous: {number}")
        return _text(rows[0].values["id"])

    def printing_numbers(self, card_id: str) -> list[str]:
        """Resolve official source-region aliases through permanent identity."""
        rows = self.db.select(
            "printing",
            self.db.columns("printing"),
            where={
                "region": self.source_region(card_id),
                "card_id": card_id,
                "card_no_state": "official",
            },
        )
        numbers = sorted({_text(row.values["card_no"]) for row in rows})
        for number in numbers:
            if self.printing_card(number) != card_id:
                raise SourceError("record_key_identity_mismatch")
        return numbers

    def construction_names(self, card_id: str) -> list[JsonValue]:
        """Native primary construction name per face ordinal; None where none is derived.

        The engine compares these, so a collab printing's site name stays display text.
        """
        faces = {
            row.values["ordinal"]: _text(row.values["id"])
            for row in self.db.select(
                "face", ("id", "ordinal"), where={"card_id": card_id}
            )
        }
        names: list[JsonValue] = []
        region = self.source_region(card_id)
        for ordinal in range(len(faces)):
            primary = self.db.select(
                "face_rules_name",
                ("rules_name_id",),
                where={"face_id": faces[ordinal], "region": region, "role": "primary"},
            )
            names.append(
                _text(
                    _one(
                        self.db,
                        "rules_name",
                        {"id": _text(primary[0].values["rules_name_id"])},
                    )["official_name"]
                )
                if len(primary) == 1
                else None
            )
        return names

    def validate_inputs(
        self, inputs: AuthorInputSet, expander: AuthorExpander
    ) -> tuple[CardResult, ...]:
        """Validate an in-memory complete inventory before writing any authored file."""
        validate_author_set(inputs, expander.schemas)
        authored = handwriting(inputs)
        results: list[CardResult] = []
        for record in inputs.index.records:
            number = record.record_key
            try:
                payload = inventory(self, number, card_id=record.card_id)
                card = materialize(self, number, authored)
            except (AuthorInputError, SourceError) as error:
                diagnostic = (
                    error.diagnostic
                    if isinstance(error, AuthorInputError)
                    else Diagnostic("source", error.reason)
                )
                results.append(
                    CardRejected(
                        record,
                        replace(
                            diagnostic,
                            card_id=record.card_id,
                            card_no=number,
                            scope=record.scope,
                        ),
                    )
                )
                continue
            tree: dict[str, JsonValue] = {
                "format": 1,
                "kind": "author_effect_set",
                "scope": "shared",
                "cards": {number: card},
            }
            expander.schemas.expanded(canonical(tree), context=record)
            violation = lint(tree, authored.cards[number].get("lint_exceptions", []))
            if violation is None:
                results.append(CardAccepted(record, tree, payload))
            else:
                results.append(
                    CardRejected(
                        record,
                        replace(
                            violation,
                            card_id=record.card_id,
                            card_no=number,
                            scope=record.scope,
                        ),
                    )
                )
        return tuple(results)

    def selection_from_results(self, results: tuple[CardResult, ...]) -> JsonValue:
        """Serialize one completed current-source check without repeating admission."""
        authors: list[JsonValue] = []
        for result in results:
            numbers = self.printing_numbers(result.record.card_id)
            if isinstance(result, CardAccepted):
                authors.append(
                    {
                        "status": "accepted",
                        "printings": list[JsonValue](numbers),
                        "input": result.lowering_input(),
                        "rules_names": self.construction_names(result.record.card_id),
                    }
                )
            else:
                authors.extend(
                    {
                        "status": "rejected",
                        "card_no": number,
                        "reason": result.diagnostic.reason,
                    }
                    for number in numbers or [result.record.record_key]
                )
        return {
            "documents": [],
            "authors": authors,
        }
