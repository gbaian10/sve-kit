"""Current physical flavor owners remain bound to their own frozen printing observation."""

from typing import TYPE_CHECKING

from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.products.models import LocalizedText
from sve_carddb.registry.records import CardData, FaceData, PrintingData
from sve_carddb.snapshot.values import digest
from sve_carddb.sources.official_jp import card_url
from sve_carddb.template_translations.flavor_models import FlavorOwner
from sve_carddb.text_observations.archive import FrozenTexts
from sve_carddb.text_observations.intern import text_values

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_inputs import Source
    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.registry.snapshot import RegistrySnapshot


class Owners:
    def __init__(self, registry: RegistrySnapshot, stores: dict[str, Path]) -> None:
        self.stores = dict(stores)
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
        self.providers: dict[str, FrozenTexts] = {}

    def resolve(self, ref: SourceRef, source: Source, text: str) -> FlavorOwner | None:
        """A matching paragraph cannot borrow another printing's confirmed identity."""
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
        key = ref.batch_id
        if key not in self.providers:
            archive = FrozenSources.configured(self.stores, key)
            self.providers[key] = FrozenTexts(
                archive.root,
                archive.store_id,
                key,
                region="jp",
                parser_version=ref.parser,
            )
        frozen = self.providers[key].version(
            "jp", printing.card_no, ref.source_version_id
        )
        if frozen.observation != printing.observation:
            raise ValueError(
                "Flavor physical observation differs from its current identity"
            )
        if (
            frozen.projected(mapping.source_index).flavor != text
            or digest(text.encode()) != ref.text_hash
        ):
            raise ValueError(
                "Flavor owner source differs from its exact physical field"
            )
        unit = text_values(LocalizedText(lang="ja", text=text))["id"]
        if not isinstance(unit, str):
            raise TypeError("Flavor text unit requires a stable ID")
        return FlavorOwner(printing.id, mapping.face_id, unit)
