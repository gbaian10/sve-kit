"""Fixed member, coordinate and raw-role reconstruction for effect inventories."""

from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import array, digest
from sve_carddb.template_semantics.v1.inventory import entry
from sve_carddb.template_semantics.v1.parameters.analysis import prepared
from sve_carddb.template_semantics.v1.projection import pointer, project
from sve_carddb.template_sources.normalizer import VERSION, partition
from sve_carddb.template_translations.sources import Reconstructed

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.frozen_sources import FrozenSources
    from sve_carddb.template_parameters.models import Candidate
    from sve_carddb.template_sources.models import Entry
PARSER = "translation-jp-v1"


def normalized_field(item: Entry, text: str) -> str:
    """Rebuild one exact source part including the fixed layout parameter recipe."""
    section = (
        int(item.source_ref.locator.rsplit("/", 1)[-1])
        if "/sections/" in item.source_ref.locator
        else None
    )
    for part in partition(text, section=section):
        if entry(item.source_ref, part, VERSION).id == item.id:
            return prepared(text, part)[0].normalized
    raise ValueError("Formal template inventory entry is absent from its frozen field")


def _fields(
    sources: FrozenSources, entries: tuple[Entry, ...]
) -> dict[tuple[str, str], str]:
    fields: dict[tuple[str, str], str] = {}
    for item in entries:
        ref = item.source_ref
        key = ref.source_version_id, ref.locator
        if key not in fields:
            source, raw, _ = sources.read(ref.source_version_id, parser_version=PARSER)
            _, document = project(raw, source.url, "jp")
            value = pointer(document, ref.locator)
            if not isinstance(value, str) or digest(value.encode()) != ref.text_hash:
                raise ValueError(
                    "Formal template field must match its exact source hash"
                )
            fields[key] = value
    return fields


def _members(
    entries: tuple[Entry, ...],
    candidates: tuple[Candidate, ...],
    fields: dict[tuple[str, str], str],
    solved: dict[tuple[str, str], dict[str, JsonValue]],
    pending: dict[str, set[str]],
) -> tuple[Reconstructed, ...]:
    by_id = {e.id: e for e in entries}
    result = []
    for candidate in candidates:
        item = by_id[candidate.inventory_id]
        normalized = normalized_field(
            item, fields[item.source_ref.source_version_id, item.source_ref.locator]
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
                normalized,
                fields[item.source_ref.source_version_id, item.source_ref.locator],
                tuple(hints),
                tuple(roles),
                tuple(sorted(causes)),
            )
        )
    return tuple(result)
