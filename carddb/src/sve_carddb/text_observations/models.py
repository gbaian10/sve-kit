"""Immutable exact face content and provenance at the observation boundary."""

from typing import Protocol

from pydantic import JsonValue

from sve_carddb.build_inputs import Source  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves fields at runtime
from sve_carddb.registry.records import Observation, RecordData, Region
from sve_carddb.snapshot.values import canonical, digest


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
        return digest(canonical(self.fields()))

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

    def locator(self) -> str:
        """Bind the use to the original physical source index, never inferred ordinals."""
        return canonical(
            {
                "region": self.region,
                "card_no": self.card_no,
                "face_index": self.source_index,
            }
        ).decode()
