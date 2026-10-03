"""Flavor owners reuse immutable identity replay without borrowing current effects or names."""

from typing import TYPE_CHECKING

from sve_carddb.products.models import LocalizedText
from sve_carddb.registry.records import CardData, FaceData, PrintingData
from sve_carddb.snapshot.values import digest
from sve_carddb.sources.official_jp import card_url
from sve_carddb.template_translations.flavor_models import FlavorOwner
from sve_carddb.text_observations.archive import FrozenTexts
from sve_carddb.text_observations.intern import text_values
from sve_carddb.translations.name_replay import IdentityEvidence

if TYPE_CHECKING:
    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import Source
    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.translations.models import IdentityBasis
    from sve_carddb.translations.sources import Sources


class FlavorOwners:
    def __init__(
        self, sources: Sources, basis: IdentityBasis, authored_revision: str
    ) -> None:
        self.evidence = IdentityEvidence(sources, authored_revision)
        self.basis = basis
        registry = self.evidence.registry(basis)
        self.printings: dict[str, list[PrintingData]] = {}
        for record in registry.records.values():
            if isinstance(record.data, PrintingData) and record.data.region == "jp":
                self.printings.setdefault(card_url(record.data.card_no), []).append(
                    record.data
                )
        self.cards = {
            r.data.id: r.data
            for r in registry.records.values()
            if isinstance(r.data, CardData)
        }
        self.faces = {
            r.data.id: r.data
            for r in registry.records.values()
            if isinstance(r.data, FaceData)
        }
        self.providers: dict[tuple[str, str], FrozenTexts] = {}

    def resolve(self, ref: SourceRef, source: Source, text: str) -> FlavorOwner | None:
        """No guessed card number or matching paragraph establishes a printing identity."""
        matches = [
            (printing, mapping)
            for printing in self.printings.get(source.url, ())
            for mapping in printing.source_face_map
            if ref.locator == f"/faces/{mapping.source_index}/flavor"
        ]
        if len(matches) != 1:
            return None
        printing, mapping = matches[0]
        if (
            self.cards[printing.card_id].identity_state != "confirmed"
            or self.faces[mapping.face_id].card_id != printing.card_id
        ):
            return None
        key = ref.store_id, ref.batch_id
        if key not in self.providers:
            self.providers[key] = FrozenTexts(
                self.evidence.sources.stores[ref.store_id],
                *key,
                region="jp",
                parser_version=ref.parser,
            )
        frozen = self.providers[key].version(
            "jp", printing.card_no, ref.source_version_id
        )
        if frozen.observation != printing.observation:
            raise ValueError(
                "Flavor physical observation differs from its identity basis"
            )
        if (
            frozen.projected(mapping.source_index).flavor != text
            or digest(text.encode()) != ref.text_hash
        ):
            raise ValueError(
                "Flavor owner source differs from its exact physical field"
            )
        unit = text_values(LocalizedText(lang="ja", text=text))["id"]
        assert isinstance(unit, str)
        return FlavorOwner(printing.id, mapping.face_id, unit)


def verify_owner(
    db: Database, owner: FlavorOwner, *, source_hash: str, context_source_unit_id: str
) -> None:
    """A downstream context must use this printing's own flavor unit, even with pending effects."""
    printings = db.select(
        "printing", db.columns("printing"), where={"id": owner.printing_id}
    )
    faces = db.select("face", db.columns("face"), where={"id": owner.face_id})
    rows = db.select(
        "printing_face",
        db.columns("printing_face"),
        where={"printing_id": owner.printing_id, "face_id": owner.face_id},
    )
    if len(printings) != 1 or len(faces) != 1 or len(rows) != 1:
        raise ValueError("Flavor printing face owner is absent")
    printed, face, printing = rows[0].values, faces[0].values, printings[0].values
    if (
        face["card_id"] != printing["card_id"]
        or printed["card_id"] != printing["card_id"]
    ):
        raise ValueError("Flavor printing face belongs to another card")
    cards = db.select("card", db.columns("card"), where={"id": printing["card_id"]})
    if len(cards) != 1 or cards[0].values["identity_state"] != "confirmed":
        raise ValueError("Flavor owner requires confirmed card identity")
    if (
        printed["flavor_unit_id"] != owner.flavor_unit_id
        or context_source_unit_id != owner.flavor_unit_id
    ):
        raise ValueError("Flavor context must use its physical owner's flavor unit")
    units = db.select(
        "text_unit", db.columns("text_unit"), where={"id": owner.flavor_unit_id}
    )
    if len(units) != 1:
        raise ValueError("Flavor physical text unit is absent")
    unit = units[0].values
    if (
        printing["region"] != "jp"
        or unit["lang"] != "ja"
        or not isinstance(unit["text"], str)
        or digest(unit["text"].encode()) != source_hash
        or unit["content_hash"] != source_hash
    ):
        raise ValueError("Flavor physical text unit exact hash or language mismatch")
