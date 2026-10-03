"""Checked fixed parser access shared by sources, identity and field owner replay."""

from typing import TYPE_CHECKING

from sve_carddb.registry.records import Observation
from sve_carddb.snapshot.values import canonical
from sve_carddb.template_semantics.v1 import identity, official_en, official_jp
from sve_carddb.template_semantics.v1.presence import detect_presence
from sve_carddb.template_semantics.v1.projection import project
from sve_carddb.text_observations.models import FaceContent, TextCard
from sve_carddb.text_observations.presence import EffectPresence

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build_inputs import Source
    from sve_carddb.registry.records import Region


class Parser:
    @staticmethod
    def project(raw: bytes, url: str, provider: str) -> tuple[str, JsonValue]:
        """Only the finite installed implementation can supply a projection."""
        return project(raw, url, provider)

    @staticmethod
    def images(raw: bytes, number: str, region: str) -> tuple[str, ...]:
        """Keep historical image associations on the same complete physical parser."""
        card = (
            official_jp.extract_card(raw, number=number)
            if region == "jp"
            else official_en.extract_card(raw, number=number)
        )
        return tuple(f.image for f in card.faces)

    @staticmethod
    def card(source: Source, raw: bytes, region: Region, number: str) -> TextCard:
        """Rebuild the complete physical observation and presence with fixed parsers."""
        if region == "jp":
            card = official_jp.extract_card(raw, number=number)
            faces = tuple(
                FaceContent(
                    name=f.name,
                    effect=f.text,
                    sections=tuple(f.sections),
                    class_raw=f.card_class,
                    type_raw=f.card_type,
                    stats=(f.cost, f.power, f.hp),
                    traits=tuple(f.traits),
                    title=f.title,
                    flavor=f.flavor,
                )
                for f in card.faces
            )
            legacy = identity.legacy_jp(card)
            date, errata = card.release_date, card.errata_url
        else:
            english = official_en.extract_card(raw, number=number)
            faces = tuple(
                FaceContent(
                    name=f.name,
                    effect=f.text,
                    sections=tuple(f.sections),
                    class_raw=f.info["Class"],
                    type_raw=f.info["Card Type"],
                    stats=(f.stats["cost"], f.stats["power"], f.stats["hp"]),
                    traits=tuple(f.traits),
                    title=f.info.get("Universe"),
                    flavor=f.speech,
                )
                for f in english.faces
            )
            legacy = official_en.legacy_projection(english)
            date, errata = english.release_date, english.errata_url
        return TextCard(
            source=source,
            observation=Observation.model_validate_json(
                canonical(identity.observation(legacy, region))
            ),
            faces=faces,
            date_raw=date,
            has_errata_link=errata is not None,
            effect_presence=tuple(
                EffectPresence.model_validate(
                    detect_presence(
                        raw, source, region=region, number=number, source_index=i
                    ).model_dump(mode="json")
                )
                for i in range(len(faces))
            ),
            raw=raw,
        )
