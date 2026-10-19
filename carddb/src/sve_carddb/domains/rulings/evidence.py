"""Require every historical member before materializing a ruling's old domain."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from sve_carddb.core.models import Hash, RecordData
from sve_carddb.domains.rulings.mapping import Definition, LegacyInputs, LegacyUse, key

if TYPE_CHECKING:
    from collections.abc import Callable

    from sve_carddb.domains.effects.classifications import Member
    from sve_carddb.domains.effects.reader import Catalog
    from sve_carddb.domains.rulings.mapping import Namespace


class Missing(RecordData):
    member_id: Hash
    reason: Literal["missing_source", "ambiguous_source", "unproven_role"]


@dataclass(frozen=True)
class Capture:
    inputs: LegacyInputs
    missing: dict[str, tuple[Missing, ...]]


def capture(
    catalog: Catalog,
    identifiers: frozenset[str],
    *,
    verify_catalog: Callable[[Catalog], Namespace],
    resolve_member: Callable[[Member], LegacyUse | Missing],
) -> Capture:
    """Boundary callbacks prove old inputs, never infer them from current matches."""
    namespace = verify_catalog(catalog)
    if namespace.code != catalog.namespace:
        raise ValueError("Historical proof has a different classification namespace")
    members = {member.id: member for member in catalog.members}
    definitions: dict[str, Definition] = {}
    missing: dict[str, tuple[Missing, ...]] = {}
    for classification in catalog.classifications:
        if classification.id not in identifiers:
            continue
        uses, gaps = _members(
            tuple(members[i] for i in classification.members), resolve_member
        )
        missing[classification.id] = tuple(gaps)
        definitions[classification.id] = Definition(
            template_id=classification.id,
            uses=None if gaps else uses,
            semantic_variant=classification.family,
        )

    def verify(definition: Definition) -> None:
        if definitions.get(definition.template_id) != definition:
            raise ValueError("Historical definition or complete member domain differs")

    inputs = LegacyInputs(namespace, tuple(definitions.values()), verify)
    inputs.index()
    return Capture(inputs, missing)


def _members(
    members: tuple[Member, ...],
    resolve: Callable[[Member], LegacyUse | Missing],
) -> tuple[tuple[LegacyUse, ...], tuple[Missing, ...]]:
    uses: dict[bytes, LegacyUse] = {}
    gaps: list[Missing] = []
    for member in members:
        result = resolve(member)
        if isinstance(result, Missing):
            if result.member_id != member.id:
                raise ValueError("Missing evidence refers to another member")
            gaps.append(result)
            continue
        if result.scope.role != result.occurrence.role:
            raise ValueError("Historical role differs from its explicit domain")
        encoded = key(result.occurrence)
        previous = uses.setdefault(encoded, result)
        if previous != result:
            raise ValueError("Printings disagree on a shared historical source use")
    return tuple(uses[k] for k in sorted(uses)), tuple(gaps)
