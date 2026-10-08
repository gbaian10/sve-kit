"""Verify current permanent identities against each frozen physical source face."""

from typing import TYPE_CHECKING

from sve_carddb.build_inputs import SourceUse
from sve_carddb.registry.records import CardData, FaceData, PrintingData
from sve_carddb.snapshot.values import canonical
from sve_carddb.sources import official_en, official_jp
from sve_carddb.text_observations.archive import FrozenTexts

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.registry.snapshot import RegistrySnapshot
    from sve_carddb.translations.sources import Sources


class IdentityEvidence:
    def __init__(self, sources: Sources, authored_revision: str) -> None:
        self.sources = sources
        self.authored_revision = authored_revision
        self.providers: dict[tuple[str, str], FrozenTexts] = {}
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
        registry = self.registry()
        lang, text, source = self.sources.text(ref)
        expected_region = "jp" if lang == "ja" else "en"
        if ref.parser != "translation-" + expected_region + "-v1" or lang not in {
            "ja",
            "en",
        }:
            raise ValueError("Name override must locate a physical name source")
        mappings: list[tuple[PrintingData, int, str]] = []
        for record in registry.records.values():
            if not isinstance(record.data, PrintingData):
                continue
            printing = record.data
            provider = official_jp if printing.region == "jp" else official_en
            if (
                printing.region != expected_region
                or provider.card_url(printing.card_no) != source.url
            ):
                continue
            mappings.extend(
                (printing, mapping.source_index, mapping.face_id)
                for mapping in printing.source_face_map
                if ref.locator == f"/faces/{mapping.source_index}/name"
            )
        if len(mappings) != 1:
            raise ValueError(
                "Name override frozen source has no unique physical face mapping"
            )
        printing, _, face = mappings[0]
        if (card_id is not None and printing.card_id != card_id) or (
            face_id is not None and face != face_id
        ):
            raise ValueError(
                "Name override frozen evidence belongs to another card or face"
            )
        cards = {
            r.data.id: r.data
            for r in registry.records.values()
            if isinstance(r.data, CardData)
        }
        faces = {
            r.data.id: r.data
            for r in registry.records.values()
            if isinstance(r.data, FaceData)
        }
        if (
            cards[printing.card_id].identity_state != "confirmed"
            or faces[face].card_id != printing.card_id
        ):
            raise ValueError("Name override requires confirmed physical identity")
        key = ref.batch_id, printing.region
        if key not in self.providers:
            batch = self.sources.batch(ref.batch_id)
            self.providers[key] = FrozenTexts(
                batch.root,
                batch.store_id,
                ref.batch_id,
                region=printing.region,
                parser_version=ref.parser,
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
