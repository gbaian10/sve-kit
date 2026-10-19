"""Readable evidence admits exact source versions without changing resolved identities."""

import json
import re
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.effects.classifications import Shard
from sve_carddb.domains.effects.reader import catalog, document, load, source_document
from sve_carddb.domains.effects.storage import StoredClassification

if TYPE_CHECKING:
    from pathlib import Path


def text_record() -> dict[str, JsonValue]:
    return {
        "id": digest(canonical({"raw": "synthetic", "body": "synthetic"})),
        "raw": "synthetic",
    }


def source_payload() -> dict[str, JsonValue]:
    return {
        "format": 1,
        "kind": "classification_source_shard",
        "namespace": "fixture",
        "basis": {"recipe": "fixture-v1", "inputs_hash": digest(b"inputs")},
        "owner": "TEST",
        "records": [text_record()],
    }


def member_record() -> dict[str, JsonValue]:
    return {
        "kind": "member",
        "id": "TEST-001:effect:0",
        "template": "T-example",
        "position": {
            "card_no": "TEST-001",
            "region": "jp",
            "face": 0,
            "field": "effect",
            "line": 0,
        },
        "source": text_record()["id"],
        "body_length": 9,
        "evidence_hash": digest(b"source-record"),
        "clauses": [{"template": "C-example", "body": {"start": 0, "end": 9}}],
    }


def label_record() -> dict[str, JsonValue]:
    return {
        "kind": "classification",
        "id": "T-example",
        "status": "candidate",
        "family": "example",
        "normalized_hash": digest(b"synthetic"),
        "definition_hash": digest(b"definition"),
        "label_hash": digest(b"label"),
        "members": ["TEST-001:effect:0"],
        "abilities": [{"id": "a1", "head": "unknown", "ir": "unknown"}],
        "clauses": [
            {
                "id": "TEST-001:effect:0~0",
                "ability": "a1",
                "stage": "unknown",
                "ir": [],
                "author": [],
                "note_hash": digest(b""),
            }
        ],
        "relations": [
            {
                "type": "sequence",
                "sources": [{"clause": "TEST-001:effect:0~0"}],
                "targets": [{"clause": "TEST-001:effect:0~0", "ability": "a1"}],
                "note_hash": digest(b"relation"),
            }
        ],
    }


def encoded(records: list[JsonValue] | None = None) -> bytes:
    return canonical(
        {
            "format": 2,
            "kind": "effect_classification_shard",
            "namespace": "fixture",
            "basis": {"recipe": "fixture-v1", "inputs_hash": digest(b"inputs")},
            "records": [label_record(), member_record()]
            if records is None
            else records,
        }
    )


def write_catalog(root: Path) -> tuple[Path, Path]:
    records = root / "rules/classifications/TEST/001.yaml"
    sources = root / "rules/classification-sources/TEST/001.yaml"
    records.parent.mkdir(parents=True)
    sources.parent.mkdir(parents=True)
    records.write_bytes(encoded())
    sources.write_bytes(canonical(source_payload()))
    return records, sources


def test_readable_evidence_restores_original_ids_and_unknown_annotations(
    tmp_path: Path,
) -> None:
    write_catalog(tmp_path)
    loaded = load(tmp_path)
    assert loaded.namespace == "fixture"
    member = loaded.members[0]
    assert member.id == member.block == digest(b"TEST-001:effect:0")
    assert member.body_hash == member.raw_hash == digest(b"synthetic")
    assert member.raw_length == member.body_length == 9
    assert member.position.ordinal is None
    assert member.token_header_hash is None
    clause = member.clauses[0]
    assert clause.id == digest(b"TEST-001:effect:0~0")
    assert clause.raw == clause.body
    label = loaded.classifications[0]
    assert label.members == (member.id,)
    assert label.clauses[0].id == clause.id
    assert label.status == "candidate"
    assert label.abilities[0].cost_timing is None
    assert label.abilities[0].note_hash is None
    assert label.clauses[0].decision is label.clauses[0].capture is None
    assert label.relations[0].sources[0].clause == clause.id
    assert label.relations[0].sources[0].ability is None
    assert label.relations[0].targets[0].ability == "a1"


def test_unicode_body_can_differ_noncontiguously_from_raw() -> None:
    text: dict[str, JsonValue] = {"raw": "甲[旁註]乙", "body": "甲乙"}
    version = digest(canonical(text))
    sources = source_payload()
    sources["records"] = [{"id": version, **text}]
    member = member_record()
    member.update(
        source=version,
        body_length=2,
        raw_length=6,
        block="TEST-001:effect:0",
        token_header_hash=digest(b"header"),
        clauses=[
            {
                "template": "C-example",
                "body": {"start": 0, "end": 2},
                "raw": {"start": 0, "end": 6},
            }
        ],
    )
    result = catalog(
        (
            document(
                encoded([label_record(), member]),
                (source_document(canonical(sources)),),
            ),
        )
    )
    assert result.members[0].body_hash == digest("甲乙".encode())
    assert result.members[0].raw_hash == digest("甲[旁註]乙".encode())
    assert result.members[0].raw_length == 6
    assert result.members[0].clauses[0].raw.end == 6
    assert result.members[0].token_header_hash == digest(b"header")


@pytest.mark.parametrize("field", ["raw", "body", "id"])
def test_changed_text_without_matching_version_hash_is_rejected(field: str) -> None:
    text = text_record()
    text[field] = digest(b"wrong") if field == "id" else "must-not-echo-this-text"
    sources = source_payload()
    sources["records"] = [text]
    with pytest.raises(
        ValueError, match=r"^Classification text version hash mismatch$"
    ):
        source_document(canonical(sources))


@pytest.mark.parametrize("change", ["missing", "wrong-version", "wrong-owner"])
def test_text_reference_must_exist_in_the_member_owner(change: str) -> None:
    sources = source_payload()
    member = member_record()
    if change == "wrong-owner":
        sources["owner"] = "OTHER"
    elif change == "wrong-version":
        member["source"] = digest(b"missing")
    parsed = () if change == "missing" else (source_document(canonical(sources)),)
    with pytest.raises(ValueError, match="Missing classification text version"):
        document(encoded([member]), parsed)


@pytest.mark.parametrize(
    "change", ["body-length", "raw-length", "body-gap", "body-overrun", "raw-overrun"]
)
def test_stored_lengths_and_spans_are_checked_against_exact_text(change: str) -> None:
    member = member_record()
    if change == "body-length":
        member.update(
            body_length=8,
            raw_length=9,
            clauses=[
                {
                    "template": "C-example",
                    "body": {"start": 0, "end": 8},
                    "raw": {"start": 0, "end": 9},
                }
            ],
        )
    elif change == "raw-length":
        member.update(
            raw_length=8,
            clauses=[
                {
                    "template": "C-example",
                    "body": {"start": 0, "end": 9},
                    "raw": {"start": 0, "end": 8},
                }
            ],
        )
    else:
        member["clauses"] = [
            {
                "template": "C-example",
                "body": {
                    "start": 1 if change == "body-gap" else 0,
                    "end": 10 if change == "body-overrun" else 9,
                },
                "raw": {"start": 0, "end": 10 if change == "raw-overrun" else 9},
            }
        ]
    message = (
        "Classification text length mismatch"
        if change in {"body-length", "raw-length"}
        else "Invalid resolved classification record"
    )
    with pytest.raises(ValueError, match="^" + message + "$"):
        document(encoded([member]), (source_document(canonical(source_payload())),))


@pytest.mark.parametrize("field", ["namespace", "basis"])
def test_text_versions_cannot_cross_input_bases(field: str) -> None:
    sources = source_payload()
    sources[field] = (
        "other"
        if field == "namespace"
        else {"recipe": "other", "inputs_hash": digest(b"other")}
    )
    with pytest.raises(ValueError, match="namespace or input basis differs"):
        document(encoded(), (source_document(canonical(sources)),))


def test_duplicate_text_version_in_one_owner_is_rejected() -> None:
    source = source_document(canonical(source_payload()))
    with pytest.raises(ValueError, match="Duplicate classification text version"):
        document(encoded(), (source, source))


@pytest.mark.parametrize("source", [True, False], ids=["text", "classification"])
@pytest.mark.parametrize("replacement", [True, 1.0, "1", 3])
def test_formats_require_their_exact_integer(
    source: bool, replacement: JsonValue
) -> None:
    raw = canonical(source_payload()) if source else encoded()
    version = b'"format":1' if source else b'"format":2'
    raw = raw.replace(version, b'"format":' + json.dumps(replacement).encode())
    if source:
        with pytest.raises(ValueError, match=r"^Invalid classification source shard$"):
            source_document(raw)
    else:
        sources = (source_document(canonical(source_payload())),)
        with pytest.raises(ValueError, match=r"^Invalid classification shard$"):
            document(raw, sources)


@pytest.mark.parametrize(
    "raw",
    [b"format: 2\nformat: 2\n", b"1: invalid\n", b"records: [", b"x" * 1048576],
    ids=["duplicate-key", "non-string-key", "invalid", "oversize"],
)
def test_invalid_yaml_diagnostics_are_bounded(tmp_path: Path, raw: bytes) -> None:
    records, _ = write_catalog(tmp_path)
    records.write_bytes(raw)
    message = (
        "Authored input must be smaller than one MiB: authored/rules/classifications/TEST/001.yaml"
        if len(raw) == 1048576
        else "Invalid authored YAML input: authored/rules/classifications/TEST/001.yaml"
    )
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        load(tmp_path)


@pytest.mark.parametrize("kind", ["member", "text"])
def test_file_owner_matches_member_and_text_owner(tmp_path: Path, kind: str) -> None:
    records, sources = write_catalog(tmp_path)
    path = records if kind == "member" else sources
    other = path.parent.parent / "OTHER" / path.name
    other.parent.mkdir()
    path.rename(other)
    with pytest.raises(ValueError, match="owner differs from shard path"):
        load(tmp_path)


@pytest.mark.parametrize(
    ("change", "message"),
    [
        (
            "missing",
            "Missing authored data area: authored/rules/classification-sources",
        ),
        ("symlink", "Symlink authored data area"),
        ("unknown-path", "Unsupported classification shard path"),
        (
            "bad-yaml",
            "Invalid authored YAML input: authored/rules/classification-sources/TEST/001.yaml",
        ),
        ("bad-record", "Invalid classification source shard"),
    ],
)
def test_source_area_is_required_and_validated(
    tmp_path: Path, change: str, message: str
) -> None:
    _, sources = write_catalog(tmp_path)
    if change == "missing":
        sources.unlink()
        sources.parent.rmdir()
        sources.parent.parent.rmdir()
    elif change == "symlink":
        other = tmp_path / "text.yaml"
        sources.rename(other)
        sources.symlink_to(other)
    elif change == "unknown-path":
        sources.rename(sources.with_name("index.yaml"))
    elif change == "bad-yaml":
        sources.write_bytes(b"raw: [")
    else:
        sources.write_bytes(canonical({**source_payload(), "extra": "do-not-echo"}))
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$") as caught:
        load(tmp_path)
    assert "do-not-echo" not in str(caught.value)


def test_invalid_classification_does_not_echo_payload(tmp_path: Path) -> None:
    records, _ = write_catalog(tmp_path)
    records.write_bytes(encoded().replace(b'"candidate"', b'"must-not-echo"'))
    with pytest.raises(ValueError, match=r"^Invalid classification shard$"):
        load(tmp_path)


def test_legacy_wire_format_is_not_accepted() -> None:
    with pytest.raises(ValueError, match="Invalid classification shard"):
        document(encoded().replace(b'"format":2', b'"format":1'))


def test_unreferenced_text_version_is_rejected(tmp_path: Path) -> None:
    _, sources = write_catalog(tmp_path)
    payload = source_payload()
    raw = "unused synthetic text"
    payload["records"] = [
        text_record(),
        {"id": digest(canonical({"raw": raw, "body": raw})), "raw": raw},
    ]
    sources.write_bytes(canonical(payload))
    with pytest.raises(ValueError, match=r"^Unreferenced classification text version$"):
        load(tmp_path)


def test_stored_records_cannot_enter_the_resolved_catalog() -> None:
    stored = StoredClassification.model_validate_json(canonical(label_record()))
    with pytest.raises(ValidationError):
        Shard.model_validate(
            {
                "namespace": "fixture",
                "basis": {"recipe": "fixture-v1", "inputs_hash": digest(b"inputs")},
                "records": (stored,),
            }
        )
