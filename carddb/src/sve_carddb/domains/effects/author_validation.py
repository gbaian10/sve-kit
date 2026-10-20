"""Local author invariants over decoded types; no registry or source admission."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import BaseModel, RootModel

from sve_carddb.contracts.author_evidence import (
    Actor,
    BodyDependencies,
    BodyRekey,
    Domain,
    InlineDependencies,
    MacroDependencies,
    MacroMechanical,
    Meta,
    Registration,
    UseRekey,
)
from sve_carddb.contracts.author_leaves import ParamRef
from sve_carddb.core.json import canonical

if TYPE_CHECKING:
    from collections.abc import Iterator

type PathParts = tuple[str | int, ...]


@dataclass(frozen=True)
class Violation:
    reason: str
    path: PathParts


def typed_nodes(
    value: object, path: PathParts = ()
) -> Iterator[tuple[PathParts, object]]:
    """Walk decoded nodes using wire aliases and transparent root wrappers."""
    if isinstance(value, RootModel):
        yield from typed_nodes(value.root, path)
        return
    yield path, value
    if isinstance(value, BaseModel):
        for name, field in type(value).model_fields.items():
            child: object = getattr(value, name)
            if field.exclude or (field.exclude_if and field.exclude_if(child)):
                continue
            key = field.serialization_alias or field.alias or name
            yield from typed_nodes(child, (*path, key))
    elif isinstance(value, dict):
        for key, child in value.items():
            if isinstance(key, str):
                yield from typed_nodes(child, (*path, key))
    elif isinstance(value, tuple | list):
        for index, child in enumerate(value):
            yield from typed_nodes(child, (*path, index))


def inline_parameters(value: object, path: PathParts) -> Iterator[Violation]:
    """Only a decoded ParamRef is a parameter; metadata keys are not syntax."""
    for location, node in typed_nodes(value, path):
        if isinstance(node, ParamRef):
            yield Violation("inline_param_reference", location)


def _ordered(values: tuple[BaseModel, ...]) -> bool:
    encoded = [
        canonical(value.model_dump(mode="json", by_alias=True)) for value in values
    ]
    return encoded == sorted(set(encoded))


def _dependency_order(
    node: BodyDependencies | InlineDependencies | MacroDependencies, path: PathParts
) -> Iterator[Violation]:
    for field in ("interfaces", "rulings"):
        values = node.interfaces if field == "interfaces" else node.rulings
        if not _ordered(values):
            yield Violation("unordered_or_duplicate_dependencies", (*path, field))
    if isinstance(node, BodyDependencies) and not _ordered(node.sources):
        yield Violation("unordered_or_duplicate_dependencies", (*path, "sources"))


def _review_identity(actor: Actor) -> bytes:
    return canonical(
        actor.model if actor.model is not None else actor.model_dump(mode="json")
    )


def _use_identity(node: InlineDependencies | MacroDependencies) -> bytes:
    return canonical(
        {
            "sources": [source.model_dump(mode="json") for source in node.sources],
            "frame_id": node.frame_id,
            "ability_scope": node.ability_scope,
        }
    )


def _reviews(value: Meta | Registration, path: PathParts) -> Iterator[Violation]:
    reviews = value.reviewed_by if isinstance(value, Meta) else value.reviews
    field = "reviewed_by" if isinstance(value, Meta) else "reviews"
    seen: set[tuple[bytes, bytes]] = set()
    uses: dict[bytes, str] = {}
    for index, review in enumerate(reviews):
        location = (*path, field, index)
        dependencies: list[BodyDependencies | InlineDependencies | MacroDependencies]
        if isinstance(review, MacroMechanical):
            kind = "macro_mechanical"
            dependencies = [review.use_dependencies]
            actor = None
        else:
            kind = review["kind"]
            actor = review["reviewer"]
            dependencies = [review["dependencies"]]
            rekey = review.get("rekey")
            if isinstance(rekey, BodyRekey | UseRekey):
                dependencies.append(rekey.after)
            if review["verdict"] == "disputed" and review["reason"] is None:
                yield Violation("disputed_reason_required", (*location, "reason"))
        local: set[tuple[bytes, bytes]] = set()
        for dependency in dependencies:
            if actor is not None:
                key = (
                    _review_identity(actor),
                    canonical(dependency.model_dump(mode="json")),
                )
                if key in seen:
                    yield Violation("duplicate_review", location)
                local.add(key)
            if isinstance(dependency, InlineDependencies | MacroDependencies):
                identity = _use_identity(dependency)
                if identity in uses and uses[identity] != kind:
                    yield Violation("review_kind_conflict", location)
                uses[identity] = kind
        seen.update(local)


def evidence_violations(value: object) -> Iterator[Violation]:
    """Validate only facts available within the decoded document."""
    for path, node in typed_nodes(value):
        if isinstance(node, Domain):
            if list(node.regions) != sorted(set(node.regions)):
                yield Violation("unordered_or_duplicate_regions", (*path, "regions"))
        elif isinstance(
            node, BodyDependencies | InlineDependencies | MacroDependencies
        ):
            yield from _dependency_order(node, path)
        elif isinstance(node, Meta | Registration):
            yield from _reviews(node, path)
