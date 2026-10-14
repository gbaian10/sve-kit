"""Verify current permanent identities against each frozen physical source face."""

import re
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical
from sve_carddb.core.provenance import SourceUse
from sve_carddb.domains.registry.records import CardData, FaceData, PrintingData
from sve_carddb.domains.text_observations.archive import FrozenTexts
from sve_carddb.parse.pages import official_en, official_jp

if TYPE_CHECKING:
    from sve_carddb.domains.catalog.adoption_models import SourceRef
    from sve_carddb.domains.registry.snapshot import RegistrySnapshot
    from sve_carddb.domains.translations.sources import Sources


class IdentityEvidence:
    def __init__(self, sources: Sources, authored_revision: str) -> None:
        self.sources = sources
        self.authored_revision = authored_revision
        self.providers: dict[tuple[str, str, str], FrozenTexts] = {}
        registry = self.registry()
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
        self.mappings: dict[tuple[str, str, int], list[tuple[PrintingData, str]]] = {}
        for record in registry.records.values():
            if isinstance(record.data, PrintingData):
                printing = record.data
                provider = official_jp if printing.region == "jp" else official_en
                url = provider.card_url(printing.card_no)
                for mapping in printing.source_face_map:
                    self.mappings.setdefault(
                        (printing.region, url, mapping.source_index), []
                    ).append((printing, mapping.face_id))
        self.uses: list[SourceUse] = []

    def registry(self) -> RegistrySnapshot:
        """Owner associations use the same current registry as this command's source resolver."""
        return self.sources.identities.registry()

    def association(
        self,
        ref: SourceRef,
        *,
        card_id: str | None = None,
        face_id: str | None = None,
    ) -> tuple[str, str, str, str]:
        """Bind raw URL, complete observation and physical source index to permanent keys."""
        lang, text, source = self.sources.text(ref)
        expected_region = "jp" if lang == "ja" else "en"
        if ref.parser != "translation-" + expected_region + "-v1" or lang not in {
            "ja",
            "en",
        }:
            raise ValueError("Name override must locate a physical name source")
        match = re.fullmatch(r"/faces/(0|[1-9][0-9]*)/name", ref.locator)
        mappings = (
            self.mappings.get((expected_region, source.url, int(match[1])), [])
            if match
            else []
        )
        if len(mappings) != 1:
            raise ValueError(
                "Name override frozen source has no unique physical face mapping"
            )
        printing, face = mappings[0]
        if (card_id is not None and printing.card_id != card_id) or (
            face_id is not None and face != face_id
        ):
            raise ValueError(
                "Name override frozen evidence belongs to another card or face"
            )
        if (
            self.cards[printing.card_id].identity_state != "confirmed"
            or self.faces[face].card_id != printing.card_id
        ):
            raise ValueError("Name override requires confirmed physical identity")
        key = ref.batch_id, printing.region, ref.parser
        if key not in self.providers:
            batch = self.sources.batch(ref.batch_id)
            self.providers[key] = FrozenTexts(
                batch.root,
                batch.store_id,
                ref.batch_id,
                region=printing.region,
                parser_version=ref.parser,
                sources=batch,
            )
        frozen = self.providers[key].version(
            printing.region, printing.card_no, ref.source_version_id
        )
        if frozen.observation != printing.observation:
            raise ValueError(
                "Name override physical observation differs from identity basis"
            )
        self.uses.append(
            SourceUse(
                source=source,
                usage="name_identity",
                locator=canonical([printing.id, face, ref.locator]).decode(),
            )
        )
        return lang, text, printing.card_id, face
