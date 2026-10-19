"""Classification admission verifies source scope, not apparent semantic quality."""

from pathlib import Path

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.effects.classifications import Classification, Member, Shard
from sve_carddb.domains.effects.reader import catalog, load


def member() -> Member:
    return Member.model_validate_json(
        canonical(
            {
                "kind": "member",
                "id": digest(b"member"),
                "template": "T-example",
                "position": {
                    "card_no": "TEST-001",
                    "region": "jp",
                    "face": 0,
                    "field": "effect",
                    "ordinal": None,
                    "line": 0,
                },
                "body_hash": digest(b"synthetic"),
                "raw_hash": digest(b"synthetic"),
                "body_length": 9,
                "raw_length": 9,
                "token_header_hash": None,
                "block": digest(b"member"),
                "evidence_hash": digest(b"source-record"),
                "clauses": [
                    {
                        "id": digest(b"clause"),
                        "template": "C-example",
                        "body": {"start": 0, "end": 9},
                        "raw": {"start": 0, "end": 9},
                    }
                ],
            }
        )
    )


def classification() -> Classification:
    return Classification.model_validate_json(
        canonical(
            {
                "kind": "classification",
                "id": "T-example",
                "status": "candidate",
                "family": "example",
                "normalized_hash": digest(b"synthetic"),
                "definition_hash": digest(b"definition"),
                "label_hash": digest(b"label"),
                "members": [member().id],
                "abilities": [
                    {
                        "id": "a1",
                        "head": "unknown",
                        "ir": "unknown",
                        "cost_timing": None,
                        "note_hash": None,
                    }
                ],
                "clauses": [
                    {
                        "id": digest(b"clause"),
                        "ability": "a1",
                        "decision": None,
                        "capture": None,
                        "stage": "unknown",
                        "ir": [],
                        "author": [],
                        "note_hash": digest(b""),
                    }
                ],
                "relations": [
                    {
                        "type": "sequence",
                        "sources": [{"clause": digest(b"clause"), "ability": None}],
                        "targets": [{"clause": digest(b"clause"), "ability": "a1"}],
                        "note_hash": digest(b"synthetic relation"),
                    }
                ],
            }
        )
    )


def encoded(records: list[JsonValue] | None = None) -> bytes:
    return canonical(
        {
            "namespace": "fixture",
            "basis": {"recipe": "fixture-v1", "inputs_hash": digest(b"inputs")},
            "records": records
            if records is not None
            else [
                classification().model_dump(mode="json"),
                member().model_dump(mode="json"),
            ],
        }
    )


def resolved_shard(raw: bytes) -> Shard:
    return Shard.model_validate_json(raw)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("status", "reviewed"),
        ("reviewed", True),
        ("verified", True),
        ("namespace", "another"),
    ],
)
def test_quality_flags_and_unknown_fields_are_rejected(
    field: str, value: JsonValue
) -> None:
    payload = classification().model_dump(mode="json")
    payload[field] = value
    with pytest.raises(ValidationError):
        resolved_shard(encoded([payload, member().model_dump(mode="json")]))


@pytest.mark.parametrize("change", ["ability", "clause", "duplicate", "ambiguous"])
def test_relation_endpoints_are_local_and_unambiguous(change: str) -> None:
    payload = classification().model_dump(mode="json")
    if change == "ability":
        payload["relations"][0]["targets"][0]["ability"] = "other"
    elif change == "clause":
        payload["relations"][0]["sources"][0]["clause"] = digest(b"missing")
    elif change == "duplicate":
        payload["clauses"].append(payload["clauses"][0])
    else:
        payload["abilities"].append({**payload["abilities"][0], "id": "a2"})
        payload["clauses"].append({**payload["clauses"][0], "ability": "a2"})
    with pytest.raises(ValidationError):
        resolved_shard(encoded([payload]))


def test_qualified_endpoints_keep_distinct_abilities_on_one_clause() -> None:
    payload = classification().model_dump(mode="json")
    payload["abilities"].append({**payload["abilities"][0], "id": "a2"})
    payload["clauses"].append({**payload["clauses"][0], "ability": "a2"})
    payload["relations"][0]["sources"][0]["ability"] = "a2"
    result = catalog(
        (resolved_shard(encoded([payload, member().model_dump(mode="json")])),)
    )
    assert result.classifications[0].relations[0].sources[0].ability == "a2"


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("missing", "Missing member"),
        ("wrong-owner", "incorrect template ownership"),
        ("unclaimed", "Unclaimed classification members"),
        ("scope", "outside template member scope"),
        ("duplicate", "Duplicate member identity"),
    ],
)
def test_complete_member_closure_rejects_equal_count_substitution(
    change: str, message: str
) -> None:
    label = classification().model_dump(mode="json")
    source = member().model_dump(mode="json")
    records: list[JsonValue] = [label, source]
    if change == "missing":
        label["members"] = [digest(b"missing")]
    elif change == "wrong-owner":
        source["template"] = "different"
    elif change == "unclaimed":
        records.append(other_member().model_dump(mode="json"))
    elif change == "scope":
        source["clauses"][0]["id"] = digest(b"outside")
    else:
        records.append(source)
    with pytest.raises(ValueError, match=message):
        catalog((resolved_shard(encoded(records)),))


@pytest.mark.parametrize("change", ["body-gap", "raw-overrun", "ordinal", "bool"])
def test_member_coordinates_and_spans_are_checked(change: str) -> None:
    source = member().model_dump(mode="json")
    if change == "body-gap":
        source["clauses"][0]["body"]["start"] = 1
    elif change == "raw-overrun":
        source["clauses"][0]["raw"]["end"] = 100
    elif change == "ordinal":
        source["position"]["ordinal"] = 0
    else:
        source["position"]["face"] = True
    with pytest.raises(ValidationError):
        resolved_shard(encoded([source]))


def test_duplicate_classifications_across_shards_are_not_overwritten() -> None:
    with pytest.raises(ValueError, match="Duplicate classification identity"):
        catalog(
            (
                resolved_shard(encoded()),
                resolved_shard(encoded([classification().model_dump(mode="json")])),
            )
        )


def test_empty_and_mixed_namespaces_fail_closed() -> None:
    with pytest.raises(ValueError, match="namespace"):
        catalog(())
    with pytest.raises(ValueError, match="namespace"):
        catalog(
            (
                resolved_shard(encoded()),
                resolved_shard(encoded().replace(b"fixture", b"other")),
            )
        )


def test_reader_rejects_symlink_shards(tmp_path: Path) -> None:
    path = tmp_path / "rules/classifications/TEST/001.yaml"
    path.parent.mkdir(parents=True)
    other = tmp_path / "other.yaml"
    other.write_bytes(encoded())
    path.symlink_to(other)
    with pytest.raises(ValueError, match="Symlink"):
        load(tmp_path)


@pytest.mark.parametrize(
    "name", ["bad.yaml", "TEST/index.yaml", "TEST/001.yml", "TEST/nested/001.yaml"]
)
def test_reader_does_not_silently_ignore_unknown_paths(
    tmp_path: Path, name: str
) -> None:
    path = tmp_path / "rules/classifications" / name
    path.parent.mkdir(parents=True)
    path.write_bytes(encoded())
    with pytest.raises(ValueError, match="Unsupported"):
        load(tmp_path)


def test_different_input_basis_cannot_share_a_namespace() -> None:
    first = resolved_shard(encoded())
    second = first.model_copy(
        update={
            "basis": first.basis.model_copy(update={"inputs_hash": digest(b"other")})
        }
    )
    with pytest.raises(ValueError, match="input basis"):
        catalog((first, second))


def test_member_block_cannot_cross_source_owner() -> None:
    source = member().model_dump(mode="json")
    source["block"] = digest(b"absent")
    with pytest.raises(ValueError, match="block"):
        catalog(
            (
                resolved_shard(
                    encoded([classification().model_dump(mode="json"), source])
                ),
            )
        )


def test_imported_candidate_catalog_preserves_the_complete_v0_scope() -> None:
    loaded = load(Path(__file__).resolve().parents[4] / "authored")
    assert loaded.namespace == "classification-v0"
    assert len(loaded.classifications) == 3669
    assert len({c.family for c in loaded.classifications}) == 2837
    assert sum(len(c.relations) for c in loaded.classifications) == 7491
    assert len(loaded.members) == 13913
    assert sum(len(m.clauses) for m in loaded.members) == 23414
    assert all(c.status == "candidate" for c in loaded.classifications)
    by_id = {c.id: c for c in loaded.classifications}
    for template, card_no in (("T48c4aac5e8", "BP20-P42"), ("T4290d0dab6", "BP11-T02")):
        assert any(
            m.id in by_id[template].members and m.position.card_no == card_no
            for m in loaded.members
        )


def other_member() -> Member:
    payload = member().model_dump(mode="json")
    payload["id"] = payload["block"] = digest(b"other-member")
    payload["position"]["card_no"] = "TEST-002"
    payload["clauses"][0]["id"] = digest(b"other-clause")
    return Member.model_validate_json(canonical(payload))


@pytest.mark.parametrize("separate", [True, False])
def test_duplicate_clause_identities_cannot_cross_members(separate: bool) -> None:
    first = classification().model_dump(mode="json")
    other = other_member().model_dump(mode="json")
    other["clauses"][0]["id"] = member().clauses[0].id
    records: list[JsonValue] = [first, member().model_dump(mode="json"), other]
    if separate:
        second = classification().model_dump(mode="json")
        second["id"] = other["template"] = "T-other"
        second["members"] = [other["id"]]
        records.append(second)
        message = "Classification scopes overlap"
    else:
        first["members"].append(other["id"])
        message = "Duplicate clause identity across members"
    with pytest.raises(ValueError, match=message):
        catalog((resolved_shard(encoded(records)),))


@pytest.mark.parametrize(
    ("change", "message"),
    [("tail", "do not cover member body"), ("duplicate", "Duplicate member clause")],
)
def test_member_requires_full_body_and_unique_clauses(
    change: str, message: str
) -> None:
    payload = member().model_dump(mode="json")
    if change == "tail":
        payload["clauses"][0]["body"]["end"] = 8
    else:
        payload["clauses"][0]["body"]["end"] = 4
        payload["clauses"][0]["raw"]["end"] = 4
        payload["clauses"].append(
            {
                **payload["clauses"][0],
                "body": {"start": 4, "end": 9},
                "raw": {"start": 4, "end": 9},
            }
        )
    with pytest.raises(ValueError, match=message):
        Member.model_validate_json(canonical(payload))


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("ability", "Duplicate local ability"),
        ("member", "Duplicate classification member"),
        ("reference", "Clause references absent local ability"),
    ],
)
def test_local_identities_and_ability_references(change: str, message: str) -> None:
    payload = classification().model_dump(mode="json")
    if change == "ability":
        payload["abilities"].append(payload["abilities"][0])
    elif change == "member":
        payload["members"].append(payload["members"][0])
    else:
        payload["clauses"][0]["ability"] = "absent"
    with pytest.raises(ValueError, match=message):
        Classification.model_validate_json(canonical(payload))


@pytest.mark.parametrize("change", ["mixed", "incomplete", "sequence", "none"])
def test_representative_coverage_and_member_template_alignment(change: str) -> None:
    label = classification().model_dump(mode="json")
    source = member().model_dump(mode="json")
    other = other_member().model_dump(mode="json")
    label["members"] = [other["id"], source["id"]]
    if change == "mixed":
        label["clauses"].append(
            {**label["clauses"][0], "id": other["clauses"][0]["id"]}
        )
    elif change == "incomplete":
        source["clauses"][0]["body"]["end"] = 4
        source["clauses"][0]["raw"]["end"] = 4
        source["clauses"].append(
            {
                **source["clauses"][0],
                "id": digest(b"tail"),
                "body": {"start": 4, "end": 9},
                "raw": {"start": 4, "end": 9},
            }
        )
    elif change == "sequence":
        other["clauses"][0]["template"] = "C-other"
    documents = (resolved_shard(encoded([label, source, other])),)
    if change == "none":
        assert len(catalog(documents).members) == 2
    else:
        message = (
            "template sequences differ"
            if change == "sequence"
            else "one complete representative"
        )
        with pytest.raises(ValueError, match=message):
            catalog(documents)


def test_distinct_member_ids_cannot_claim_the_same_position() -> None:
    other = other_member().model_dump(mode="json")
    other["position"] = member().position.model_dump(mode="json")
    with pytest.raises(ValueError, match="Duplicate source position"):
        catalog(
            (
                resolved_shard(
                    encoded(
                        [
                            classification().model_dump(mode="json"),
                            member().model_dump(mode="json"),
                            other,
                        ]
                    )
                ),
            )
        )


@pytest.mark.parametrize(
    "change", ["card", "face", "field", "ordinal", "later", "header", "none"]
)
def test_block_must_precede_member_within_the_same_owner(change: str) -> None:
    source = member().model_dump(mode="json")
    block = other_member().model_dump(mode="json")
    block["position"] = {**source["position"]}
    source["position"]["line"] = 1
    source["block"] = block["id"]
    label = classification().model_dump(mode="json")
    label["members"].append(block["id"])
    if change == "card":
        block["position"]["card_no"] = "TEST-002"
    elif change == "face":
        block["position"]["face"] = 1
    elif change == "field":
        block["position"].update(field="section", ordinal=0)
    elif change == "ordinal":
        source["position"].update(field="section", ordinal=0)
        block["position"].update(field="section", ordinal=1)
    elif change == "later":
        block["position"]["line"] = 2
    elif change == "header":
        source["token_header_hash"] = digest(b"different-header")
    documents = (resolved_shard(encoded([label, source, block])),)
    if change == "none":
        assert len(catalog(documents).members) == 2
    else:
        with pytest.raises(
            ValueError, match="Member block is outside its source owner scope"
        ):
            catalog(documents)


def test_raw_clause_spans_cannot_overlap_when_body_is_partitioned() -> None:
    source = member().model_dump(mode="json")
    source["clauses"][0]["body"]["end"] = 4
    source["clauses"][0]["raw"]["end"] = 5
    source["clauses"].append(
        {
            **source["clauses"][0],
            "id": digest(b"next-clause"),
            "body": {"start": 4, "end": 9},
            "raw": {"start": 4, "end": 9},
        }
    )
    with pytest.raises(ValueError, match="Clause spans do not partition member body"):
        Member.model_validate_json(canonical(source))
