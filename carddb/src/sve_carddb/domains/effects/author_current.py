"""Admit author cards against current JP identities and visible rule lines."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.author_leaves import AuthorIndexRecord
from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_direct import inventory, materialize
from sve_carddb.domains.effects.author_inputs import (
    AuthorInputError,
    Diagnostic,
    SourceError,
)
from sve_carddb.domains.effects.author_lint import lint
from sve_carddb.domains.effects.author_resolution import handwriting
from sve_carddb.domains.effects.author_set import validate_author_set

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Row, Value
    from sve_carddb.contracts.author_leaves import IndexRecord
    from sve_carddb.domains.effects.author_expansion import AuthorExpander
    from sve_carddb.domains.effects.author_set import AuthorInputSet


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
    record: IndexRecord
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


class CurrentAuthorSources:
    """Resolve whole-card author coordinates from the selected build."""

    def __init__(self, db: Database) -> None:
        self.db = db
        self._jp_printings: tuple[int, dict[object, list[Row]]] | None = None

    def _printings(self, number: str) -> list[Row]:
        # One scan per database revision replaces a full table scan per lookup.
        revision = self.db.revision()
        if self._jp_printings is None or self._jp_printings[0] != revision:
            numbers: dict[object, list[Row]] = {}
            for row in self.db.select(
                "printing", self.db.columns("printing"), where={"region": "jp"}
            ):
                numbers.setdefault(row.values["card_no"], []).append(row)
            self._jp_printings = revision, numbers
        return self._jp_printings[1].get(number, [])

    def printing_card(self, number: str, *, prototype: bool = False) -> str:
        """Resolve an official JP printing only when all rows agree on identity."""
        rows = self._printings(number)
        identities = {_text(row.values["card_id"]) for row in rows}
        if len(identities) != 1:
            raise SourceError(
                "prototype_identity_unresolved"
                if prototype
                else "record_key_identity_mismatch"
            )
        if any(row.values["card_no_state"] != "official" for row in rows):
            raise SourceError(
                "prototype_identity_unresolved"
                if prototype
                else "provisional_record_key"
            )
        return identities.pop()

    def printing_numbers(self, card_id: str) -> list[str]:
        """Resolve official JP aliases through permanent identity, never number spelling."""
        rows = self.db.select(
            "printing",
            self.db.columns("printing"),
            where={"region": "jp", "card_id": card_id, "card_no_state": "official"},
        )
        numbers = sorted({_text(row.values["card_no"]) for row in rows})
        for number in numbers:
            if self.printing_card(number) != card_id:
                raise SourceError("record_key_identity_mismatch")
        return numbers

    def construction_names(self, card_id: str) -> list[JsonValue]:
        """JP primary construction name per face ordinal; None where none is derived.

        The engine compares these, so a collab printing's site name stays display text.
        """
        faces = {
            row.values["ordinal"]: _text(row.values["id"])
            for row in self.db.select(
                "face", ("id", "ordinal"), where={"card_id": card_id}
            )
        }
        names: list[JsonValue] = []
        for ordinal in range(len(faces)):
            primary = self.db.select(
                "face_rules_name",
                ("rules_name_id",),
                where={"face_id": faces[ordinal], "region": "jp", "role": "primary"},
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

    def _inventory(self, inputs: AuthorInputSet, expander: AuthorExpander) -> None:
        prototypes: set[str] = set()
        for file in inputs.prototype_files:
            tree = object_value(expander.schemas.prototype_tree(file.raw))
            prototypes.update(
                self.printing_card(number, prototype=True)
                for number in object_value(tree["cards"])
            )
        if prototypes & {
            record.card_id
            for record in inputs.index.records
            if isinstance(record, AuthorIndexRecord) and record.scope == "shared"
        }:
            raise SourceError("prototype_author_overlap")
        for record in inputs.index.records:
            if (
                not isinstance(record, AuthorIndexRecord)
                and self.printing_card(record.record_key, prototype=True)
                != record.card_id
            ):
                raise SourceError("prototype_index_identity_mismatch")

    @staticmethod
    def _supported(record: IndexRecord) -> AuthorIndexRecord:
        if not isinstance(record, AuthorIndexRecord):
            raise SourceError("unsupported_prototype")
        if record.scope == "en_override":
            raise SourceError("en_source_contract_unavailable")
        return record

    def validate_inputs(
        self, inputs: AuthorInputSet, expander: AuthorExpander
    ) -> tuple[CardResult, ...]:
        """Validate an in-memory complete inventory before writing any authored file."""
        validate_author_set(inputs, expander.schemas)
        try:
            self._inventory(inputs, expander)
        except SourceError as error:
            raise AuthorInputError(Diagnostic("source", error.reason)) from None

        authored = handwriting(inputs)
        results: list[CardResult] = []
        for record in inputs.index.records:
            number = record.record_key
            try:
                supported = self._supported(record)
                payload = inventory(self, number, card_id=supported.card_id)
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
            expander.schemas.expanded(canonical(tree), context=supported)
            violation = lint(tree, authored.cards[number].get("lint_exceptions", []))
            if violation is None:
                results.append(CardAccepted(supported, tree, payload))
            else:
                results.append(
                    CardRejected(
                        supported,
                        replace(
                            violation,
                            card_id=supported.card_id,
                            card_no=number,
                            scope=supported.scope,
                        ),
                    )
                )
        return tuple(results)

    def selection_from_results(
        self, inputs: AuthorInputSet, results: tuple[CardResult, ...]
    ) -> JsonValue:
        """Serialize one completed current-source check without repeating admission."""
        authors: list[JsonValue] = []
        for result in results:
            if not isinstance(result.record, AuthorIndexRecord):
                continue
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
            "documents": [
                [file.path, file.raw.decode("utf-8")] for file in inputs.prototype_files
            ],
            "authors": authors,
        }
