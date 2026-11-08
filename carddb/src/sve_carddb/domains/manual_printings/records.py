"""Strict immutable manual-printing data at the authored build-input boundary."""

import re
from typing import Annotated, Literal
from urllib.parse import urlsplit

from pydantic import Field, computed_field, field_validator, model_validator

from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.core.regions import Region
from sve_carddb.domains.registry.build import permanent_id
from sve_carddb.domains.registry.records import CardId, FaceId, PrintingId

PARSER_VERSION = "manual-printings-v1"
AREA = "manual-printings/printings"
Slug = Annotated[str, Field(pattern=r"^[a-z0-9][a-z0-9-]{0,63}\Z")]
Owner = Annotated[str, Field(pattern=r"^[A-Za-z0-9_-]+\Z")]
Positive = Annotated[int, Field(ge=1, le=65535)]
ReferenceClass = Literal["official_site", "official_x", "official_video"]

# Exact official hosts; subdomains and look-alike domains are third party.
OFFICIAL_HOSTS = frozenset(
    {
        "shadowverse-evolve.com",
        "en.shadowverse-evolve.com",
        "sve-gp.com",
        "bushiroad.com",
        "en.bushiroad.com",
    }
)
# Official X accounts, compared case-insensitively as X does.
OFFICIAL_X_ACCOUNTS = frozenset(
    {
        "shadowverse_ev",
        "shadowverse_jp",
        "svevolve_en",
        "shadowverserage",
        "bo_tcg_event",
    }
)
_X_STATUS = re.compile(r"^/([A-Za-z0-9_]{1,15})/status/[0-9]+/?\Z")
# Official videos are linked by their public watch URL; the channel is reviewed by hand.
_YOUTUBE_WATCH = re.compile(r"^v=[A-Za-z0-9_-]{11}\Z")
_YOUTUBE_SHORT = re.compile(r"^/[A-Za-z0-9_-]{11}\Z")


def reference_class(url: str) -> ReferenceClass:
    """Accept only an official site page, an official X post or an official video."""
    parts = urlsplit(url)
    plain = (
        parts.scheme == "https"
        and not parts.fragment
        and bool(parts.hostname)
        and parts.netloc.casefold() == (parts.hostname or "").casefold()
        and not any(character.isspace() or character < " " for character in url)
    )
    if not plain or any(
        item is not None for item in (parts.username, parts.password, parts.port)
    ):
        raise ValueError("Reference must be a plain https URL")
    if parts.hostname in OFFICIAL_HOSTS:
        return "official_site"
    if parts.hostname == "x.com":
        match = _X_STATUS.fullmatch(parts.path)
        if match is None or match.group(1).casefold() not in OFFICIAL_X_ACCOUNTS:
            raise ValueError("Reference is not a post by an official X account")
        return "official_x"
    if _official_video(parts.hostname or "", parts.path, parts.query):
        return "official_video"
    raise ValueError("Reference host is not an official site")


def _official_video(host: str, path: str, query: str) -> bool:
    if host == "www.youtube.com":
        return path == "/watch" and _YOUTUBE_WATCH.fullmatch(query) is not None
    return (
        host == "youtu.be" and not query and _YOUTUBE_SHORT.fullmatch(path) is not None
    )


def manual_anchor(region: str, slug: str) -> str:
    """Card numbers never contain this infix, so manual anchors cannot collide."""
    return region + ":manual:" + slug


class ManualName(RecordData):
    lang: Literal["ja", "en"]
    text: Text

    @field_validator("text")
    @classmethod
    def _trimmed(cls, value: str) -> str:
        if value != value.strip():
            raise ValueError("Manual name must not have surrounding whitespace")
        return value


class ManualImage(RecordData):
    sha256: Hash
    width: Positive
    height: Positive
    origin: Literal["maintainer_photo", "official_image", "official_crop"]
    source_url: str | None
    approved: Literal[True]

    @model_validator(mode="after")
    def _source(self) -> ManualImage:
        if self.origin == "maintainer_photo":
            if self.source_url is not None:
                raise ValueError("A maintainer photo has no source URL")
        elif self.source_url is not None:
            reference_class(self.source_url)
        elif self.origin == "official_image":
            raise ValueError("An official image requires its source URL")
        return self


class ManualPrintingData(RecordData):
    slug: Slug
    id: PrintingId
    card_id: CardId
    face_id: FaceId
    int_id: Annotated[int, Field(ge=1, le=4294967295)]
    region: Region
    home_set_id: Text
    card_type: Literal["ep", "sep"]
    card_no: Text | None
    card_no_state: Literal["none", "official", "provisional"]
    printed_code: Text | None
    name: ManualName
    release_kind: Literal["event", "basic"]
    references: tuple[str, ...]
    image: ManualImage | None

    @field_validator("home_set_id")
    @classmethod
    def _family(cls, value: str) -> str:
        if value != "EP":
            raise ValueError("Manual EP/SEP printings must belong to the EP family")
        return value

    @model_validator(mode="after")
    def _consistent(self) -> ManualPrintingData:
        anchor = manual_anchor(self.region, self.slug)
        if (
            self.id != permanent_id("p", anchor)
            or self.card_id != permanent_id("c", anchor)
            or self.face_id != permanent_id("f", self.card_id + ":0")
        ):
            raise ValueError("Manual printing IDs disagree with the slug recipe")
        if (self.card_no is None) != (self.card_no_state == "none"):
            raise ValueError("card_no is empty exactly when card_no_state is none")
        if self.card_no is not None and self.card_no != self.card_no.strip():
            raise ValueError("card_no must not have surrounding whitespace")
        if (
            self.printed_code is not None
            and self.printed_code != self.printed_code.strip()
        ):
            raise ValueError("printed_code must not have surrounding whitespace")
        if self.name.lang != ("ja" if self.region == "jp" else "en"):
            raise ValueError("Manual name language must match the printing region")
        if list(self.references) != sorted(set(self.references)):
            raise ValueError("References must be sorted and unique")
        for url in self.references:
            reference_class(url)
        if (self.release_kind == "basic") != (not self.references):
            raise ValueError(
                "Basic EP/SEP have no references; event printings need at least one"
            )
        if self.image is not None and (
            self.image.width == self.image.height
            or (self.card_type == "sep") != (self.image.width > self.image.height)
        ):
            raise ValueError("Image orientation disagrees with the card type")
        return self


class ManualEntry(RecordData):
    kind: Literal["manual_printing"]
    owner: Owner
    data: ManualPrintingData

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes this property; mypy cannot compose property decorators.
    @property
    def record_key(self) -> str:
        """Derive identity from the permanent printing ID, as the registry does."""
        return self.kind + ":" + self.data.id

    @model_validator(mode="after")
    def _owner(self) -> ManualEntry:
        if self.owner != self.data.home_set_id:
            raise ValueError("Manual printing owner must be its home_set_id")
        return self


class Shard(RecordData):
    format: Literal[1]
    kind: Literal["manual_printings_shard"]
    records: Annotated[tuple[ManualEntry, ...], Field(min_length=1)]

    @field_validator("format", mode="before")
    @classmethod
    def _version(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Manual printings format must be an integer")
        return value
