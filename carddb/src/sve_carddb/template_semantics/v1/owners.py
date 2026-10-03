"""Flavor owners reuse immutable identity replay without borrowing current effects or names."""

from typing import TYPE_CHECKING
from urllib.parse import quote

from sve_carddb.registry.records import CardData, FaceData, PrintingData
from sve_carddb.snapshot.values import digest
from sve_carddb.template_semantics.v1.page import CARD_DIR
from sve_carddb.template_semantics.v1.urls import canonicalize
from sve_carddb.template_translations.flavor_models import FlavorOwner
from sve_carddb.text_observations.archive import FrozenTexts
from sve_carddb.translations.name_replay import IdentityEvidence

if TYPE_CHECKING:
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
                self.printings.setdefault(
                    canonicalize(
                        CARD_DIR + "?cardno=" + quote(record.data.card_no, safe="")
                    ),
                    [],
                ).append(record.data)
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
                semantic_parser=self.evidence.sources.semantic_parser,
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
        unit = "t:ja:" + digest(text.encode())[7:23]
        return FlavorOwner(printing.id, mapping.face_id, unit)
