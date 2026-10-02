"""Independent fingerprint/member coverage and collision counterexamples."""

import copy
import re
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.template_sources import checkpoint
from sve_carddb.template_sources.checkpoint import LegacyTemplate, compare, read_legacy
from sve_carddb.template_sources.inventory import Occurrence, Scan, coverage

from .template_source_fixtures import template_case as template_case  # ruff: ignore[useless-import-alias] -- register reusable synthetic inputs

if TYPE_CHECKING:
    from pathlib import Path

    from .template_source_fixtures import Case


def test_all_fingerprints_and_all_source_uses_are_independently_counted(
    template_case: Case,
) -> None:
    result = compare(template_case.scan, template_case.legacy)
    assert object_value(result["fingerprints"])["reproduced"] == 3
    assert object_value(result["fingerprints"])["generated"] == 4
    assert object_value(result["fingerprints"])["complete"] is True
    assert object_value(result["legacy_member_coverage"])["matched"] == 5
    assert object_value(result["legacy_member_coverage"])["additional_members"] == 1
    assert coverage(template_case.scan)["complete"] is True
    assert coverage(template_case.scan)["expected_pages"] == 3
    assert coverage(template_case.scan)["expected_fields"] == 6
    assert coverage(template_case.scan)["field_states"] == {"text": 5, "absent": 1}


def test_reproducing_every_template_does_not_prove_complete_member_coverage(
    template_case: Case,
) -> None:
    scan = copy.deepcopy(template_case.scan)
    scan.occurrences = [
        item
        for item in scan.occurrences
        if item.member_hash != digest(b"SYN-02#1/text/0")
    ]
    result = compare(scan, template_case.legacy)
    assert object_value(result["fingerprints"])["complete"] is True
    members = object_value(result["legacy_member_coverage"])
    assert members["complete"] is False
    assert members["failures"] == [
        {
            "template": template_case.legacy[0].identifier,
            "member_hash": digest(b"SYN-02#1/text/0"),
            "reason": "missing_archived_ability_member",
            "actual_normalized_hash": None,
        }
    ]


def test_changed_member_hash_is_reported_even_when_another_member_reproduces_the_id(
    template_case: Case,
) -> None:
    scan = copy.deepcopy(template_case.scan)
    index = next(
        i
        for i, item in enumerate(scan.occurrences)
        if item.member_hash == digest(b"SYN-02#1/text/0")
    )
    scan.occurrences[index] = replace(
        scan.occurrences[index],
        normalized="ChangedN",
        template="T" + digest(b"ChangedN")[7:17],
    )
    result = compare(scan, template_case.legacy)
    assert object_value(result["fingerprints"])["complete"] is True
    assert object_value(result["legacy_member_coverage"])["complete"] is False
    assert object_value(result["legacy_member_coverage"])["failures"] == [
        {
            "template": template_case.legacy[0].identifier,
            "member_hash": digest(b"SYN-02#1/text/0"),
            "reason": "archived_member_normalized_changed",
            "actual_normalized_hash": digest(b"ChangedN"),
        }
    ]


def test_every_unreproduced_template_has_its_own_reason(template_case: Case) -> None:
    scan = copy.deepcopy(template_case.scan)
    scan.occurrences = []
    result = compare(scan, template_case.legacy)
    assert object_value(result["fingerprints"])["complete"] is False
    assert object_value(result["fingerprints"])["templates"] == [
        {
            "template": item.identifier,
            "normalized_hash": digest(item.normalized.encode()),
            "reproduced": False,
            "expected_members": len(item.members),
            "matched_members": 0,
            "reasons": ["missing_archived_ability_member"],
        }
        for item in template_case.legacy
    ]


@pytest.mark.parametrize(
    "change",
    [
        "entry_field",
        "entry_hash",
        "proof_gap",
        "proof_overlap",
        "proof_bad_pair",
        "proof_noninteger",
        "proof_duplicate",
        "proof_tail",
    ],
)
def test_field_hash_and_original_partition_are_verified_independently(
    template_case: Case, change: str
) -> None:
    scan = copy.deepcopy(template_case.scan)
    if change.startswith("entry"):
        first = scan.entries[0]
        ref = first.source_ref.model_copy(
            update={"locator": "/faces/0/name"}
            if change == "entry_field"
            else {"text_hash": digest(b"wrong")}
        )
        scan.entries[0] = first.model_copy(update={"source_ref": ref})
    else:
        proof = next(proof for proof in scan.fields if proof["state"] == "text")
        segments = array(proof["segments"])
        if change == "proof_duplicate":
            segments.append(segments[0])
        elif change == "proof_tail":
            proof["code_points"] = 999
        else:
            span = array(array(object_value(segments[0])["ranges"])[0])
            if change == "proof_gap":
                span[0] = 1
            elif change == "proof_overlap":
                span[0] = -1
            elif change == "proof_bad_pair":
                span.pop()
            else:
                span[0] = True
    assert coverage(scan)["complete"] is False
    assert coverage(scan)["trace_complete"] is False


def test_unknown_page_presence_does_not_hide_successful_trace_accounting(
    template_case: Case,
) -> None:
    scan = copy.deepcopy(template_case.scan)
    scan.failures.append({"reason": "unknown_effect_presence"})
    assert coverage(scan)["complete"] is False
    assert coverage(scan)["trace_complete"] is True


@pytest.mark.parametrize(
    "change",
    [
        "page",
        "duplicate_page",
        "field",
        "duplicate_field",
        "failed_field",
        "unparsed_page",
        "unknown",
        "section",
        "entries",
    ],
)
def test_source_coverage_checks_the_complete_sealed_set_and_all_fields(
    template_case: Case, change: str
) -> None:
    scan = copy.deepcopy(template_case.scan)
    if change == "page":
        scan.pages.pop()
    elif change == "duplicate_page":
        scan.pages.append(scan.pages[0])
    elif change == "field":
        scan.fields.pop()
    elif change == "duplicate_field":
        scan.fields.append(scan.fields[0])
    elif change == "failed_field":
        scan.fields[0]["covered"] = False
    elif change == "unparsed_page":
        scan.expected_versions = (*scan.expected_versions, "missing")
    elif change == "unknown":
        scan.failures.append({"reason": "unknown_effect_presence"})
    elif change == "entries":
        scan.entries.pop()
    else:
        scan.fields = [
            proof for proof in scan.fields if proof["locator"] != "/faces/0/sections/1"
        ]
        for page in scan.pages:
            page["fields"] = [
                locator
                for locator in array(page["fields"])
                if locator != "/faces/0/sections/1"
            ]
    assert coverage(scan)["complete"] is False
    assert (
        object_value(compare(scan, template_case.legacy)["fingerprints"])["complete"]
        is True
    )


@pytest.mark.parametrize("kind", ["short", "full", "legacy", "member"])
def test_hash_collisions_and_duplicate_use_ids_are_rejected(
    kind: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    scan = Scan((), ())
    scan.occurrences = [
        Occurrence("T0000000000", "First", digest(b"first"), "one"),
        Occurrence(
            "T0000000000" if kind == "short" else "T1111111111",
            "Second",
            digest(b"first" if kind == "member" else b"second"),
            "two",
        ),
    ]
    legacy: tuple[LegacyTemplate, ...] = ()
    if kind == "full":
        monkeypatch.setattr(checkpoint, "digest", lambda _: "sha256:" + "a" * 64)
        message = "Full normalized hash collides with different bytes"
    elif kind == "member":
        message = "Current JP template occurrence must have a unique legacy member"
    else:
        message = "Legacy short template ID collides with different normalized bytes"
        if kind == "legacy":
            legacy = (LegacyTemplate("T0000000000", "Wrong", ("one",)),)
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        compare(scan, legacy)


@pytest.mark.parametrize(
    "change",
    [
        "bad_json",
        "empty",
        "id",
        "normalized",
        "lines",
        "bool_lines",
        "members",
        "empty_members",
        "duplicate_id",
        "duplicate_member",
        "global_member",
    ],
)
def test_legacy_catalog_is_validated_without_echoing_draft_text(
    tmp_path: Path, change: str
) -> None:
    value: dict[str, JsonValue] = {
        "template": "T" + digest(b"Synthetic")[7:17],
        "normalized": "Synthetic",
        "members": ["one"],
        "lines": 1,
    }
    if change in {
        "id",
        "normalized",
        "lines",
        "bool_lines",
        "members",
        "empty_members",
    }:
        key = {"id": "template", "bool_lines": "lines", "empty_members": "members"}.get(
            change, change
        )
        changes: dict[str, JsonValue] = {
            "id": "T0000000000",
            "normalized": "Private synthetic text",
            "lines": 2,
            "bool_lines": True,
            "members": [3],
            "empty_members": [],
        }
        value[key] = changes[change]
    if change == "duplicate_member":
        value["members"], value["lines"] = ["one", "one"], 2
    raw = canonical(value) + b"\n"
    message = "Invalid legacy template fingerprint or member set"
    if change in {"empty", "duplicate_id"}:
        raw = b"" if change == "empty" else raw * 2
        message = "Legacy template IDs must be nonempty and unique"
    elif change == "bad_json":
        raw = b"Private synthetic text"
    elif change == "global_member":
        raw += (
            canonical(
                {
                    **value,
                    "template": "T" + digest(b"Other")[7:17],
                    "normalized": "Other",
                }
            )
            + b"\n"
        )
        message = "Legacy template members must be globally unique"
    elif change == "duplicate_member":
        message = "Legacy template members must be globally unique"
    path = tmp_path / "legacy.jsonl"
    path.write_bytes(raw)
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        read_legacy(path)
