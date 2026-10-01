"""Immutable exact face content and provenance at the observation boundary."""

from typing import Protocol

from pydantic import Field, JsonValue

from sve_carddb.build_inputs import Source  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves fields at runtime
from sve_carddb.html import parse, select_all, select_one
from sve_carddb.registry.records import Observation, RecordData, Region
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.text_observations.presence import EffectPresence  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves evidence fields


class FaceContent(RecordData):
    name: str
    effect: str | None
    sections: tuple[str, ...] = ()
    class_raw: str
    type_raw: str
    stats: tuple[str, str, str]
    traits: tuple[str, ...] = ()
    title: str | None = None
    flavor: str | None = None

    def fields(self) -> dict[str, JsonValue]:
        """Keep current-bearing fields exact; flavor belongs to the physical printing."""
        return self.model_dump(mode="json", exclude={"flavor"})

    def fingerprint(self) -> str:
        """Hash complete current-bearing content without normalizing wording."""
        return digest(canonical(self.wording_fields()))

    def wording_fields(self) -> dict[str, JsonValue]:
        """Use the wording-face-v1 field names while preserving extractor values."""
        return {
            "name": self.name,
            "class": self.class_raw,
            "type": self.type_raw,
            "cost": self.stats[0],
            "attack": self.stats[1],
            "defense": self.stats[2],
            "traits": list[JsonValue](self.traits),
            "title": self.title,
            "text": self.effect,
            "sections": list[JsonValue](self.sections),
        }

    def possible_no_effect(self) -> bool:
        """Provide a conservative diagnostic hint, never an empty-text inference."""
        return (
            self.effect is None
            and not self.sections
            and self.type_raw in {"フォロワー", "Follower"}
            and all(value.isascii() and value.isdecimal() for value in self.stats)
        )


class TextCard(RecordData):
    source: Source
    observation: Observation
    faces: tuple[FaceContent, ...]
    date_raw: str | None = None
    has_errata_link: bool = False
    effect_presence: tuple[EffectPresence, ...] = ()
    raw: bytes | None = Field(default=None, exclude=True)

    def projected(self, index: int) -> FaceContent:
        """Change only proven absence; unknown empty values remain deferred."""
        content = self.faces[index]
        if not self.effect_presence:
            return content
        proof = self.effect_presence[index]
        effect = content.effect
        if proof.result.state == "absent":
            if effect not in {None, ""}:
                raise ValueError("Absent effect presence contradicts extracted effect")
            effect = ""
        elif proof.result.state == "unknown" and effect in {None, ""}:
            effect = None
        elif (
            proof.result.state == "present"
            and effect is not None
            and not effect
            and not content.sections
            and self.raw is not None
        ):
            face = select_all(parse(self.raw.decode()), ".cardlist-Detail_Box_Inner")[
                index
            ]
            container = select_one(face, ".detail")
            if (
                container is not None
                and container.text()
                and not container.text().strip()
            ):
                effect = container.text()
        return content.model_copy(update={"effect": effect})


class TextProvider(Protocol):
    def card(self, region: Region, card_no: str) -> TextCard | None:
        """Resolve a pinned source and independently transcribe every physical face."""
        ...


class FaceObservation(RecordData):
    card_id: str
    printing_id: str
    face_id: str
    region: Region
    card_no: str
    source_index: int
    card: TextCard
    content: FaceContent
    correction_keys: tuple[str, ...] = ()

    def locator(self) -> str:
        """Bind the use to the original physical source index, never inferred ordinals."""
        return canonical(
            {
                "region": self.region,
                "card_no": self.card_no,
                "face_index": self.source_index,
            }
        ).decode()


def candidate_revision_id(item: FaceObservation) -> str:
    """Bind a candidate identity to its exact face/region/content, never crawl order."""
    identity: list[JsonValue] = [item.face_id, item.region, item.content.fingerprint()]
    if item.correction_keys:
        identity.extend(item.correction_keys)
    return "rev:v1:" + digest(canonical(identity)).removeprefix("sha256:")
