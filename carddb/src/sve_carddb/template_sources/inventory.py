"""Read every sealed JP current page; retain hash-only full-field coverage proofs."""

from collections import Counter
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.core.json import array, canonical, digest, object_value
from sve_carddb.template_sources.models import Entry
from sve_carddb.template_sources.normalizer import (
    VERSION,
    Part,
    partition,
    verify_partition,
)
from sve_carddb.template_sources.pins import PARSER
from sve_carddb.text_observations.presence import detect_presence
from sve_carddb.translations.sources import pointer, project

if TYPE_CHECKING:
    from sve_carddb.core.provenance import Source
    from sve_carddb.ingest.archive.frozen_sources import FrozenSources
    from sve_carddb.text_observations.presence import PresenceState


@dataclass
class Scan:
    expected_versions: tuple[str, ...]
    entries: list[Entry] = field(default_factory=list)
    pages: list[dict[str, JsonValue]] = field(default_factory=list)
    fields: list[dict[str, JsonValue]] = field(default_factory=list)
    failures: list[dict[str, JsonValue]] = field(default_factory=list)
    history_gaps: int = 0
    documents: dict[str, JsonValue] = field(default_factory=dict)


def entry(ref: SourceRef, part: Part, normalizer_id: str) -> Entry:
    """Store and archive batch names stay out of the key, so a reseal keeps entry IDs."""
    key: JsonValue = [
        ref.model_dump(mode="json", exclude={"batch_id"}),
        part.line_ordinal,
        part.role,
        [[span.start, span.end] for span in part.segments],
    ]
    return Entry(
        id="inv:" + digest(canonical(key))[7:],
        source_ref=ref,
        line_ordinal=part.line_ordinal,
        role=part.role,
        normalizer_id=normalizer_id,
        normalized_hash=part.normalized_hash,
    )


def fields(document: JsonValue) -> tuple[tuple[str, str | None, int | None], ...]:
    """Enumerate the whole JP ability projection, including empty/unknown main text."""
    result: list[tuple[str, str | None, int | None]] = []
    faces = array(object_value(document).get("faces"))
    if not faces:
        raise ValueError("Template projection must contain card faces")
    for index, value in enumerate(faces):
        face = object_value(value)
        text = face.get("text")
        if text is not None and not isinstance(text, str):
            raise ValueError("Template field must be exact text or unknown")
        result.append((f"/faces/{index}/text", text, None))
        for section, text in enumerate(array(face.get("sections"))):
            if not isinstance(text, str):
                raise TypeError("Template section must contain exact text")
            result.append((f"/faces/{index}/sections/{section}", text, section))
    return tuple(result)


def _field(
    scan: Scan, source: Source, locator: str, text: str | None, section: int | None
) -> None:
    proof: dict[str, JsonValue] = {
        "source_version_id": source.id,
        "locator": locator,
        "state": "unknown" if text is None else "empty" if not text else "text",
        "text_hash": None if text is None else digest(text.encode()),
        "covered": False,
    }
    if text is None:
        scan.fields.append(proof)
        return
    try:
        parts = partition(text, section=section)
    except ValueError as error:
        reason = (
            "unrecognized_token_header"
            if str(error) == "Unrecognized legacy token header"
            else "legacy_partition_failed"
        )
        scan.failures.append({**proof, "reason": reason})
        scan.fields.append(proof)
        return
    verify_partition(text, parts)
    ref = SourceRef(
        batch_id=source.archive.batch_id,
        source_version_id=source.id,
        parser=PARSER,
        locator=locator,
        text_hash=digest(text.encode()),
    )
    scan.entries.extend(entry(ref, part, VERSION) for part in parts)
    proof["covered"] = True
    proof["code_points"] = len(text)
    proof["segments"] = [
        {
            "entry_id": entry(ref, part, VERSION).id,
            "role": part.role,
            "ranges": [[span.start, span.end] for span in part.segments],
        }
        for part in parts
    ]
    scan.fields.append(proof)


def _presence(
    scan: Scan, source: Source, locator: str, text: str | None, state: PresenceState
) -> None:
    reason = None
    if state == "unknown":
        reason = "unknown_effect_presence"
    elif text is None and state != "absent":
        reason = "present_effect_not_transcribed"
    elif text is None:
        scan.fields[-1]["state"] = "absent"
        scan.fields[-1]["covered"] = True
    if reason:
        scan.failures.append(
            {"source_version_id": source.id, "locator": locator, "reason": reason}
        )


def _scan(sources: FrozenSources) -> Scan:
    if [(scope.provider, scope.kind) for scope in sources.inventory.scope] != [
        ("jp", "card")
    ]:
        raise ValueError("Template checkpoint requires an exclusively JP card batch")
    scan = Scan(tuple(item.source_version_id for item in sources.inventory.current))
    scan.history_gaps = len(sources.inventory.history_gaps)
    for current in sources.inventory.current:
        source, raw, descriptor = sources.read(
            current.source_version_id, parser_version=PARSER
        )
        if (descriptor.provider, descriptor.kind, descriptor.url, source.kind) != (
            "jp",
            "card",
            current.url,
            "official_page",
        ):
            raise ValueError("Template frozen source identity or media mismatch")
        try:
            lang, document = project(raw, source.url, "jp")
            projected = fields(document)
            scan.documents[source.id] = document
        except ValueError, LookupError, UnicodeError:
            scan.failures.append(
                {"source_version_id": source.id, "reason": "jp_projection_failed"}
            )
            continue
        assert lang == "ja"
        number = object_value(document)["number"]
        if not isinstance(number, str):
            raise TypeError("Template projection lacks a card number")
        presence = [
            detect_presence(raw, source, region="jp", number=number, source_index=index)
            for index in range(len(array(object_value(document)["faces"])))
        ]
        scan.pages.append(
            {
                "source_version_id": source.id,
                "fields": [locator for locator, _, _ in projected],
                "section_counts": [
                    len(array(object_value(face)["sections"]))
                    for face in array(object_value(document)["faces"])
                ],
                "presence": [item.value() for item in presence],
            }
        )
        for locator, text, section in projected:
            _field(scan, source, locator, text, section)
            if section is None:
                _presence(
                    scan,
                    source,
                    locator,
                    text,
                    presence[int(locator.split("/")[2])].result.state,
                )
    if len({item.id for item in scan.entries}) != len(scan.entries):
        raise ValueError("Template inventory entry IDs must be unique")
    return scan


def scan_current(sources: FrozenSources) -> Scan:
    """The installed parser enumerates the complete sealed batch, without old producers."""
    return _scan(sources)


def _proof_entries(proof: dict[str, JsonValue]) -> tuple[set[str], bool]:
    segments = [object_value(value) for value in array(proof.get("segments", []))]
    identifiers = {str(value["entry_id"]) for value in segments}
    spans: list[tuple[int, int]] = []
    for value in segments:
        for span in array(value["ranges"]):
            pair = array(span)
            if len(pair) != 2 or type(pair[0]) is not int or type(pair[1]) is not int:  # ruff: ignore[magic-value-comparison] -- half-open interval pairs have exactly two integer endpoints
                return identifiers, False
            spans.append((pair[0], pair[1]))
    position = 0
    for start, end in sorted(spans):
        if start != position or end <= position:
            return identifiers, False
        position = end
    expected = 0 if proof["state"] == "absent" else proof.get("code_points")
    return identifiers, position == expected and len(identifiers) == len(segments)


def coverage(scan: Scan) -> dict[str, JsonValue]:
    """Compare actual page/field proofs with the independently expected sealed set."""
    actual = [str(page["source_version_id"]) for page in scan.pages]
    expected_fields: set[tuple[str, str]] = set()
    for page in scan.pages:
        for face, count in enumerate(array(page["section_counts"])):
            if type(count) is not int:
                raise TypeError("Template coverage section count must be an integer")
            expected_fields.add((str(page["source_version_id"]), f"/faces/{face}/text"))
            expected_fields.update(
                (str(page["source_version_id"]), f"/faces/{face}/sections/{section}")
                for section in range(count)
            )
    actual_fields = [
        (proof["source_version_id"], proof["locator"]) for proof in scan.fields
    ]
    proof_entries = [_proof_entries(proof) for proof in scan.fields]
    expected_entries = set().union(*(identifiers for identifiers, _ in proof_entries))
    actual_entries = [item.id for item in scan.entries]
    field_hashes = {
        (str(proof["source_version_id"]), str(proof["locator"])): proof["text_hash"]
        for proof in scan.fields
    }
    trace_complete = (
        len(actual) == len(set(actual))
        and set(actual) == set(scan.expected_versions)
        and len(actual_fields) == len(set(actual_fields))
        and set(actual_fields) == expected_fields
        and len(actual_entries) == len(set(actual_entries))
        and set(actual_entries) == expected_entries
        and all(valid for _, valid in proof_entries)
        and all(proof["covered"] is True for proof in scan.fields)
        and all(
            field_hashes.get(
                (item.source_ref.source_version_id, item.source_ref.locator)
            )
            == item.source_ref.text_hash
            for item in scan.entries
        )
    )
    return {
        "complete": trace_complete and not scan.failures,
        "trace_complete": trace_complete,
        "expected_pages": len(scan.expected_versions),
        "parsed_pages": len(actual),
        "expected_fields": len(expected_fields),
        "covered_fields": sum(proof["covered"] is True for proof in scan.fields),
        "field_states": dict(Counter(str(proof["state"]) for proof in scan.fields)),
        "role_entries": dict(Counter(item.role for item in scan.entries)),
        "history_gaps": scan.history_gaps,
        "history_counted_in_frequency": False,
        "missing_versions": list[JsonValue](
            sorted(set(scan.expected_versions) - set(actual))
        ),
        "failures": list[JsonValue](scan.failures),
    }


def replay(item: Entry, document: JsonValue) -> str:
    """Reconstruct a candidate from its complete field and verify every declared hash."""
    text = pointer(document, item.source_ref.locator)
    if not isinstance(text, str) or digest(text.encode()) != item.source_ref.text_hash:
        raise ValueError("Template entry must locate exact hash-verified text")
    if item.normalizer_id != VERSION or item.source_ref.parser != PARSER:
        raise ValueError("Template entry uses an unsupported recipe")
    located = [
        (text, section)
        for locator, text, section in fields(document)
        if locator == item.source_ref.locator
    ]
    if not located:
        raise ValueError(
            "Template entry cannot be replayed from the pinned field recipe"
        )
    _, section = located[0]
    for part in partition(text, section=section):
        if entry(item.source_ref, part, item.normalizer_id) == item:
            return part.normalized
    raise ValueError("Template entry cannot be replayed from the pinned field recipe")
