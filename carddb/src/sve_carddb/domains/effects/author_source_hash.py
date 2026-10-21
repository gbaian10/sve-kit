"""Closed effect-source-v1 payloads and exact source fingerprints."""

from typing import Annotated, Literal, Self

from pydantic import AfterValidator, Field, model_validator

from sve_carddb.contracts.four_layer import Code, Hash, Id, Owner
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import RecordData, UInt


def _ordered_codes(values: tuple[str, ...]) -> tuple[str, ...]:
    if values != tuple(sorted(set(values))):
        raise ValueError("Source codes must be sorted and unique")
    return values


Codes = Annotated[tuple[Code, ...], AfterValidator(_ordered_codes)]


class Known[T](RecordData):
    state: Literal["known"]
    value: T


class Unknown(RecordData):
    state: Literal["unknown"]
    value: None


type Attribute[T] = Annotated[Known[T] | Unknown, Field(discriminator="state")]


class SourceAttributes(RecordData):
    class_code: Attribute[Code | None]
    type_code: Attribute[Code]
    cost: Attribute[UInt | None]
    attack: Attribute[UInt | None]
    defense: Attribute[UInt | None]
    traits: Attribute[Codes]
    titles: Attribute[Codes]
    special_kinds: Attribute[Codes]


class SourceText(RecordData):
    field: Literal["name", "effect", "section"]
    ordinal: UInt | None
    state: Literal["present", "absent", "unknown"]
    source_hash: Hash | None

    @model_validator(mode="after")
    def _shape(self) -> Self:
        if (self.field == "section") != (self.ordinal is not None):
            raise ValueError("Source text field and ordinal disagree")
        if self.state == "unknown":
            if self.source_hash is not None:
                raise ValueError("Unknown text cannot have a source hash")
        elif self.source_hash is None:
            raise ValueError("Known text requires a source hash")
        if self.state == "absent" and (
            self.field != "effect"
            or self.source_hash != digest(b"").removeprefix("sha256:")
        ):
            raise ValueError("Absent text must be the empty effect")
        return self


class SourceRulesName(RecordData):
    rules_name_id: Id
    role: Literal["primary", "collab", "treated_as"]


class SourceFacePayload(RecordData):
    face_id: Id
    revision_id: Id
    texts: tuple[SourceText, ...]
    attributes: SourceAttributes
    rules_names: tuple[SourceRulesName, ...]

    @model_validator(mode="after")
    def _order(self) -> Self:
        actual = tuple((text.field, text.ordinal) for text in self.texts)
        expected = (
            ("name", None),
            ("effect", None),
            *(("section", ordinal) for ordinal in range(len(self.texts) - 2)),
        )
        if actual != expected:
            raise ValueError(
                "Source texts require name, effect and contiguous sections"
            )
        names = tuple((name.rules_name_id, name.role) for name in self.rules_names)
        if names != tuple(sorted(set(names))):
            raise ValueError("Source rules names must be sorted and unique")
        return self


class SourceTokenLink(RecordData):
    source_owner: Owner
    section: UInt | None
    token_face_id: Id


class AuthorSourcePayload(RecordData):
    recipe: Literal["effect-source-v1"]
    normalizer: Code
    region: Literal["jp"]
    faces: Annotated[tuple[SourceFacePayload, ...], Field(min_length=1)]
    token_links: tuple[SourceTokenLink, ...]

    @model_validator(mode="after")
    def _closure(self) -> Self:
        faces = tuple(face.face_id for face in self.faces)
        if faces != tuple(sorted(set(faces))):
            raise ValueError("Source faces must be sorted and unique")
        links = tuple(
            canonical(link.model_dump(mode="json")) for link in self.token_links
        )
        if links != tuple(sorted(set(links))):
            raise ValueError("Token links must be sorted and unique")
        if any(link.token_face_id not in faces for link in self.token_links):
            raise ValueError("Token target is outside the source face closure")
        return self


def source_payload(value: AuthorSourcePayload) -> bytes:
    """Revalidate closed fields before producing canonical-json-v1 bytes."""
    # Frozen models still permit unchecked model_construct/model_copy and nested edits.
    raw = canonical(value.model_dump(mode="json"))
    checked = AuthorSourcePayload.model_validate_json(raw)
    return canonical(checked.model_dump(mode="json"))


def source_version(value: AuthorSourcePayload) -> str:
    """Fingerprint all source facts without claiming current-source admission."""
    return "effect-source-v1:" + digest(source_payload(value))
