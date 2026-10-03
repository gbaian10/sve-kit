"""Closed historical replay metadata; a valid envelope does not authorize replay."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves constrained wire annotations

from typing import Annotated, Literal, Self

from pydantic import Field, field_validator, model_validator

from sve_carddb.build_inputs import Revision
from sve_carddb.products.models import Code
from sve_carddb.registry.records import Hash, RecordData, Text, UInt
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_sources.models import Entry, Recipe
from sve_carddb.template_translations.flavor_models import FlavorEntry, FlavorInputs

SafePath = Annotated[
    str,
    Field(
        pattern=r"^(?!.*(?:^|/)\.{1,2}(?:/|$))[^/\\\x00-\x1f]+(?:/[^/\\\x00-\x1f]+)*\Z"
    ),
]


def _ordered(keys: tuple[str, ...], label: str) -> None:
    if keys != tuple(sorted(set(keys))):
        raise ValueError(f"{label} must be sorted and unique")


class Artifact(RecordData):
    path: SafePath
    hash: Hash


class SemanticBinding(RecordData):
    id: Code
    revision: Revision
    path: SafePath
    hash: Hash


class SemanticManifest(RecordData):
    semantic_version_format: Literal[1]
    id: Code
    entrypoint: Code
    files: Annotated[tuple[Artifact, ...], Field(min_length=1)]
    environment_packages: Annotated[tuple[Text, ...], Field(min_length=1)]

    @field_validator("semantic_version_format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Semantic manifest format must be integer one")
        return value

    @model_validator(mode="after")
    def _order(self) -> Self:
        _ordered(tuple(f.path for f in self.files), "Semantic manifest files")
        _ordered(self.environment_packages, "Semantic environment packages")
        return self


class PythonEnvironment(RecordData):
    version: Text
    implementation: Text
    unicode_version: Text


class PackageEnvironment(RecordData):
    name: Text
    version: Text
    artifacts: Annotated[tuple[Artifact, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def _order(self) -> Self:
        _ordered(tuple(a.path for a in self.artifacts), "Environment artifacts")
        return self


class ProducerEnvironment(RecordData):
    python: PythonEnvironment
    packages: Annotated[tuple[PackageEnvironment, ...], Field(min_length=1)]
    uv_lock_hash: Hash
    pyproject_hash: Hash

    @model_validator(mode="after")
    def _order(self) -> Self:
        _ordered(tuple(p.name for p in self.packages), "Environment packages")
        return self


class StreamDigest(RecordData):
    count: UInt
    hash: Hash


class OutputStreams(RecordData):
    entries: StreamDigest
    fields: StreamDigest
    members: StreamDigest
    coverage: StreamDigest
    checkpoint: StreamDigest
    source_uses: StreamDigest


class ExpectedOutputs(RecordData):
    format: Literal[1]
    recipe: Literal["semantic-output-v1"]
    streams: OutputStreams
    root: Hash

    @field_validator("format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Semantic output format must be integer one")
        return value

    @model_validator(mode="after")
    def _root(self) -> Self:
        if self.root != digest(
            canonical(self.model_dump(mode="json", exclude={"root"}))
        ):
            raise ValueError("Semantic output manifest root mismatch")
        return self


class EffectInputs(RecordData):
    kind: Literal["effect"]


class FlavorReplayInputs(FlavorInputs):
    kind: Literal["flavor"]

    @model_validator(mode="after")
    def _order(self) -> Self:
        keys = tuple(
            canonical(b.model_dump(mode="json")).decode() for b in self.identity_batches
        )
        _ordered(keys, "Flavor replay identity batches")
        return self


class ReplayContext(RecordData):
    format: Literal[1]
    semantic_bindings: Annotated[tuple[SemanticBinding, ...], Field(min_length=1)]
    environment: ProducerEnvironment
    inputs: Annotated[EffectInputs | FlavorReplayInputs, Field(discriminator="kind")]
    expected_outputs: ExpectedOutputs

    @field_validator("format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Template replay context format must be integer one")
        return value

    @model_validator(mode="after")
    def _order(self) -> Self:
        _ordered(tuple(b.id for b in self.semantic_bindings), "Semantic bindings")
        return self


class InventoryV2(RecordData):
    template_source_format: Literal[2]
    kind: Literal["template_source_inventory"]
    recipes: Annotated[tuple[Recipe, ...], Field(min_length=1)]
    replay_context: ReplayContext
    entries: tuple[Annotated[Entry | FlavorEntry, Field(discriminator="role")], ...]

    @field_validator("template_source_format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Template inventory format must be integer two")
        return value

    @model_validator(mode="after")
    def _closed(self) -> Self:
        _ordered(tuple(r.id for r in self.recipes), "Template recipes")
        _ordered(tuple(e.id for e in self.entries), "Template entries")
        flavor = isinstance(self.replay_context.inputs, FlavorReplayInputs)
        if any(isinstance(e, FlavorEntry) != flavor for e in self.entries):
            raise ValueError("Template entries differ from the replay input kind")
        exact = [r for r in self.recipes if r.id == "flavor-exact-v1"]
        if flavor != bool(exact) or any(r.config for r in exact):
            raise ValueError("Flavor replay requires its exact empty recipe config")
        return self

    def group_key(self) -> str:
        """Historical context is part of the cache identity, including expected output."""
        return digest(
            canonical(
                self.model_dump(mode="json", include={"recipes", "replay_context"})
            )
        )
