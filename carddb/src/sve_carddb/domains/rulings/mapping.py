"""Relate verified old domains to exact current source uses, never to an entire frame."""

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.four_layer import Code, OccurrenceKey, SemanticVariant
from sve_carddb.contracts.rulings import Mapping, PendingReason, Scope, Target
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import RecordData, Text

if TYPE_CHECKING:
    from collections.abc import Callable

    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding


class LegacyUse(RecordData):
    occurrence: OccurrenceKey
    scope: Scope
    parameters: dict[Code, JsonValue]


class Definition(RecordData):
    template_id: Text
    uses: tuple[LegacyUse, ...] | None
    semantic_variant: Code | SemanticVariant


class Namespace(RecordData):
    code: Code
    id_recipe: Code
    normalizer_version: Code


@dataclass(frozen=True)
class LegacyInputs:
    namespace: Namespace
    definitions: tuple[Definition, ...]
    verify: Callable[[Definition], None]

    def index(self) -> dict[str, Definition]:
        """The old source boundary must prove definitions and their complete explicit domains."""
        result = {}
        if self.namespace.code == "unknown":
            raise ValueError("Unknown legacy namespace must remain absent")
        for definition in self.definitions:
            if definition.template_id in result:
                raise ValueError("Duplicate legacy definition")
            self.verify(definition)
            if definition.uses is None:
                result[definition.template_id] = definition
                continue
            keys = [key(use.occurrence) for use in definition.uses]
            if not keys or len(keys) != len(set(keys)):
                raise ValueError(
                    "Legacy definition needs a nonempty unique complete domain"
                )
            if any(use.scope.role != use.occurrence.role for use in definition.uses):
                raise ValueError("Legacy scope differs from its source role")
            result[definition.template_id] = definition
        return result


@dataclass(frozen=True)
class FrameUse:
    frame: Frame
    binding: SourceBinding
    scope: Scope

    def target(self) -> Target:
        """Callers supply the classified build result, not a candidate draft or a text hash."""
        binding = self.binding
        occurrence = OccurrenceKey(
            owner=binding.source.owner,
            field=binding.source.field,
            ordinal=binding.source.ordinal,
            source_hash=binding.source.source_hash,
            line_ordinal=binding.line_ordinal,
            role=binding.source_span.role,
            segments=binding.source_span.segments,
        )
        if (
            binding.frame_id != self.frame.id
            or occurrence.role != self.frame.role
            or self.scope.role != occurrence.role
            or (
                self.frame.semantic_variant.state == "pending"
                and self.frame.semantic_variant.scope != occurrence
            )
        ):
            raise ValueError("Frame target differs from its verified source use")
        return Target(
            frame_id=self.frame.id,
            semantic_variant=self.frame.semantic_variant,
            scope=self.scope,
            occurrence=occurrence,
        )


def key(value: RecordData) -> bytes:
    """Use complete typed identities rather than ID prefixes or display wording."""
    return canonical(value.model_dump(mode="json"))


def location(value: OccurrenceKey) -> bytes:
    """A changed hash is a disposition only after both source versions have been verified."""
    return canonical(
        value.model_dump(mode="json", include={"owner", "field", "ordinal"})
    )


class Rebuild:
    def __init__(
        self,
        legacy: tuple[LegacyInputs, ...],
        current: tuple[FrameUse, ...],
        verify_current: Callable[[FrameUse], None],
    ) -> None:
        self.definitions: dict[str, list[tuple[Namespace, Definition]]] = defaultdict(
            list
        )
        self.namespaces = tuple(inputs.namespace for inputs in legacy)
        namespaces = set()
        for inputs in legacy:
            if inputs.namespace.code in namespaces:
                raise ValueError("Duplicate legacy namespace")
            namespaces.add(inputs.namespace.code)
            for identifier, definition in inputs.index().items():
                self.definitions[identifier].append((inputs.namespace, definition))
        self.targets: dict[bytes, list[Target]] = defaultdict(list)
        self.locations: dict[bytes, set[str]] = defaultdict(set)
        self.binding_ids: dict[bytes, str] = {}
        self.parameters: dict[bytes, dict[str, JsonValue]] = {}
        self.frames: dict[str, Frame] = {}
        seen = set()
        for use in current:
            verify_current(use)
            target = use.target()
            encoded = key(target)
            if encoded in seen:
                raise ValueError("Duplicate current frame source use")
            seen.add(encoded)
            previous = self.frames.setdefault(use.frame.id, use.frame)
            if previous != use.frame:
                raise ValueError("Conflicting current frame definition")
            self.targets[key(target.occurrence)].append(target)
            self.locations[location(target.occurrence)].add(
                target.occurrence.source_hash
            )
            self.binding_ids[encoded] = use.binding.id
            self.parameters[encoded] = dict(
                use.binding.model_dump(mode="json")["values"]
            )
        self.mappings = self._mappings()

    def _mappings(self) -> tuple[Mapping, ...]:
        result: list[Mapping] = []
        for definitions in self.definitions.values():
            for namespace, definition in definitions:
                for use in definition.uses or ():
                    result.extend(
                        Mapping(
                            legacy_namespace=namespace.code,
                            legacy_template_id=definition.template_id,
                            occurrence=use.occurrence,
                            frame_id=target.frame_id,
                            semantic_variant=target.semantic_variant,
                            scope=target.scope,
                        )
                        for target in self.candidates(use)
                    )
        return tuple(sorted(result, key=key))

    def candidates(self, use: LegacyUse) -> tuple[Target, ...]:
        """Exact source identity and equivalent scope are required even for pending candidates."""
        return tuple(
            sorted(
                (
                    t
                    for t in self.targets.get(key(use.occurrence), ())
                    if t.scope == use.scope
                ),
                key=key,
            )
        )

    def reason(self, use: LegacyUse) -> PendingReason:
        """Unchanged words cannot authorize a different role or domain."""
        if self.targets.get(key(use.occurrence)):
            return "unsupported_relation"
        hashes = self.locations.get(location(use.occurrence))
        if hashes is not None:
            return (
                "unsupported_relation"
                if use.occurrence.source_hash in hashes
                else "source_changed"
            )
        return "no_candidate"

    def families(self) -> list[dict[str, JsonValue]]:
        """Keep every old family and its parameter values, including unresolved variants."""
        groups: dict[str, list[dict[str, JsonValue]]] = defaultdict(list)
        for definitions in self.definitions.values():
            for namespace, definition in definitions:
                for use in definition.uses or ():
                    for target in self.candidates(use):
                        groups[target.frame_id].append(
                            {
                                "legacy_namespace": namespace.code,
                                "legacy_template_id": definition.template_id,
                                "occurrence": use.occurrence.model_dump(mode="json"),
                                "scope": use.scope.model_dump(mode="json"),
                                "parameter_changes": _parameter_changes(
                                    use.parameters, self.parameters[key(target)]
                                ),
                                "legacy_variant": definition.semantic_variant
                                if isinstance(definition.semantic_variant, str)
                                else definition.semantic_variant.model_dump(
                                    mode="json"
                                ),
                                "new_leaf_schema": self.frames[
                                    target.frame_id
                                ].leaf_schema.model_dump(mode="json"),
                                "semantic_variant": target.semantic_variant.model_dump(
                                    mode="json"
                                ),
                                "pending": target.semantic_variant.state == "pending",
                            }
                        )
        result: list[dict[str, JsonValue]] = []
        for identifier, members in sorted(groups.items()):
            variants = {
                canonical(m["legacy_variant"]): m["legacy_variant"] for m in members
            }
            result.append(
                {
                    "frame_id": identifier,
                    "legacy_variants": [variants[v] for v in sorted(variants)],
                    "families": sorted(members, key=canonical),
                }
            )
        return result


def _parameter_changes(
    old: dict[str, JsonValue], new: dict[str, JsonValue]
) -> list[JsonValue]:
    # Historical string operands can contain official card wording.
    return [
        {
            "slot": name,
            "old_hash": digest(canonical(old[name])) if name in old else None,
            "new_hash": digest(canonical(new[name])) if name in new else None,
        }
        for name in sorted(old.keys() | new.keys())
        if (name in old) != (name in new) or old.get(name) != new.get(name)
    ]
