"""Current template members and positional schema checks, without historical replay."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import array, canonical
from sve_carddb.template_parameters.analysis import SAFE_INTEGER, prepared
from sve_carddb.template_parameters.verification import verify_values
from sve_carddb.template_sources.inventory import entry
from sve_carddb.template_sources.normalizer import VERSION, partition

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.template_parameters.models import Candidate, Hint, Schema
    from sve_carddb.template_sources.models import Entry
POSITIVE_ROLES = {
    "choice_ordinal",
    "card_ordinal",
    "repetition_ordinal",
    "turn_ordinal",
    "deck_top_ordinal",
    "counter_group_size",
    "damage_count_multiplier",
    "damage_attack_multiplier",
    "count_formula_multiplier",
    "attack_damage_multiplier",
}


@dataclass(frozen=True)
class Reconstructed:
    entry: Entry
    candidate: Candidate
    normalized: str
    field_text: str
    hints: tuple[Hint, ...]
    roles: tuple[str, ...]
    pending: tuple[str, ...]
    low_confidence: bool = False

    def verify_schema(self, schema: Schema) -> None:
        """Renaming or merging repeated slots cannot erase a source position or its role."""
        if self.pending:
            raise ValueError(
                "Template definition source has unresolved parameter roles"
            )
        hints = []
        used = set()
        for slot in schema.slots:
            roles = set()
            for occurrence in slot.occurrences:
                found = [
                    i
                    for i, hint in enumerate(self.hints)
                    if hint.occurrence == occurrence
                ]
                if len(found) != 1 or found[0] in used:
                    raise ValueError(
                        "Template schema must cover each source position exactly once"
                    )
                index = found[0]
                used.add(index)
                hint = self.hints[index]
                roles.add(self.roles[index])
                lower = 1 if self.roles[index] in POSITIVE_ROLES else 0
                if slot.type == "uint" and (
                    slot.min != lower or slot.max != SAFE_INTEGER
                ):
                    raise ValueError(
                        "Template numeric bounds differ from the recognized role"
                    )
                hints.append(hint.model_copy(update={"name": slot.name}))
            if len(roles) != 1:
                raise ValueError(
                    "Repeated template slot cannot combine distinct semantic roles"
                )
        if used != set(range(len(self.hints))):
            raise ValueError(
                "Template schema must cover each source position exactly once"
            )
        verify_values(self.field_text, self.normalized, schema, tuple(hints))

    def role_signature(self) -> bytes:
        """Keep semantic role differences separate from type/coordinate equality."""
        return canonical(
            [
                [hint.occurrence.model_dump(mode="json"), role]
                for hint, role in zip(self.hints, self.roles, strict=True)
            ]
        )


def _members(
    entries: tuple[Entry, ...],
    candidates: tuple[Candidate, ...],
    fields: dict[tuple[str, str], str],
    solved: dict[tuple[str, str], dict[str, JsonValue]],
    pending: dict[str, set[str]],
    *,
    store_id: str,
) -> tuple[Reconstructed, ...]:
    by_id = {e.id: e for e in entries}
    result = []
    for candidate in candidates:
        item = by_id[candidate.inventory_id]
        value = normalized(
            item,
            fields[item.source_ref.source_version_id, item.source_ref.locator],
            store_id=store_id,
        )
        hints, roles = [], []
        for hint in candidate.slots:
            solution = solved.get((item.id, hint.name))
            hints.append(
                hint
                if solution is None
                else hint.model_copy(
                    update={
                        "issues": tuple(
                            str(i) for i in array(solution["remaining_issues"])
                        )
                    }
                )
            )
            roles.append(
                hint.semantic_role
                if solution is None
                else str(solution["recognized_role"])
            )
        # Defining reminder boundaries remains a human decision, not recognition consent.
        causes = pending.get(item.id, set()) - {
            "legacy_parenthesis_classification_requires_review"
        }
        result.append(
            Reconstructed(
                item,
                candidate,
                value,
                fields[item.source_ref.source_version_id, item.source_ref.locator],
                tuple(hints),
                tuple(roles),
                tuple(sorted(causes)),
            )
        )
    return tuple(result)


def normalized(item: Entry, text: str, *, store_id: str) -> str:
    """Rebuild one exact source part including the fixed layout parameter recipe."""
    section = (
        int(item.source_ref.locator.rsplit("/", 1)[-1])
        if "/sections/" in item.source_ref.locator
        else None
    )
    for part in partition(text, section=section):
        if entry(item.source_ref, part, VERSION, store_id=store_id).id == item.id:
            return prepared(text, part)[0].normalized
    raise ValueError("Formal template inventory entry is absent from its frozen field")
