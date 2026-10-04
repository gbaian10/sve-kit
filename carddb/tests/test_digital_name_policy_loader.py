"""Whole-entry refusal tests use exact single messages, never live/private inputs."""

import re
import shutil
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.digital_name_policies.loader import (
    ADOPTED_APPROVALS,
    ADOPTED_PROJECTIONS,
    INDEX,
    decoded,
    load,
    model,
)
from sve_carddb.digital_name_policies.models import Policy
from sve_carddb.registry.storage import MAX_BYTES, read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value

from .adoption_fixtures import commit
from .digital_name_policy_fixtures import (
    LINKS,
    exclusion,
    loader_repository,
    policy,
    receipt,
    rewrite,
)

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> tuple[Path, str]:
    return loader_repository(tmp_path_factory.mktemp("name-policy-loader"))


@pytest.fixture(autouse=True)  # ruff: ignore[pytest-fixture-autouse] -- versioned link guards share synthetic document bindings
def synthetic_bindings(
    baseline: tuple[Path, str], monkeypatch: pytest.MonkeyPatch
) -> None:
    # Versioned link immutability uses synthetic bindings rather than private evidence.
    for purpose in ("links",):
        document = policy(baseline[0], purpose)
        approved = str(document["approved_document_hash"])
        monkeypatch.setitem(ADOPTED_PROJECTIONS, approved, digest(canonical(document)))
        monkeypatch.setitem(
            ADOPTED_APPROVALS,
            approved,
            digest(canonical(receipt(baseline[0], purpose))),
        )


def copied(baseline: tuple[Path, str], root: Path) -> Path:
    shutil.copytree(baseline[0], root)
    return root


def reject(root: Path, message: str) -> None:
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        load(root / "authored", root, commit(root))


def test_complete_entry_is_detached_and_retains_both_hashes(
    baseline: tuple[Path, str],
) -> None:
    root, revision = baseline
    snapshot = load(root / "authored", root, revision)
    assert len(snapshot.files) == 5
    assert len(snapshot.policies) == 1
    for purpose in ("links",):
        loaded = snapshot.effective(purpose)
        document = loaded.document()
        assert document.approved_document_hash != digest(loaded.policy)
        document.content.clear()
        assert loaded.document().content
        assert loaded.excluded().entries == ()
    assert snapshot.pins()["authored_revision"] == revision


@pytest.mark.parametrize(
    "raw",
    [
        b"x: &a [*a]",
        b"x: *a",
        b"x: !custom hi",
        b"x: 1\nx: 2",
        b"x: .nan",
        b"\xff",
        b"x: 1\n---\nx: 2",
        b"? [a, b]\n: value",
    ],
    ids=[
        "cycle",
        "alias",
        "custom-tag",
        "duplicate",
        "nan",
        "utf8",
        "documents",
        "complex-key",
    ],
)
def test_yaml_boundary(raw: bytes) -> None:
    with pytest.raises(ValueError, match=r"^Invalid digital-name policy YAML$"):
        decoded(raw)


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        (b"x: &a hi\ny: *a", {"x": "hi", "y": "hi"}),
        (b"x: !!str hi", {"x": "hi"}),
        (b"x: {<<: {k: hi}}", {"x": {"k": "hi"}}),
    ],
    ids=["alias", "standard-tag", "merge"],
)
def test_yaml_expansion_is_allowed(raw: bytes, expected: JsonValue) -> None:
    assert decoded(raw) == expected


def test_size_boundary() -> None:
    with pytest.raises(
        ValueError, match=r"^Digital-name policy file exceeds size limit$"
    ):
        decoded(b" " * MAX_BYTES)


@pytest.mark.parametrize("field", ["digital_name_policy_format", "version"])
def test_bool_is_not_a_version(baseline: tuple[Path, str], field: str) -> None:
    raw = policy(baseline[0], "links")
    raw[field] = True
    message = (
        "Policy format must be an integer"
        if field.endswith("_format")
        else "Invalid digital-name policy fields"
    )
    with pytest.raises(ValueError, match="^" + message + "$"):
        model(Policy, raw)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("extra", 1),
        ("kind", "unknown"),
        ("version", 0),
        ("purpose", "coverage"),
        ("projection_recipe", "new-rules-v2"),
        ("approved_document_hash", "sha256:abc"),
    ],
)
def test_unknown_envelope_is_not_ignored(
    baseline: tuple[Path, str], tmp_path: Path, field: str, value: JsonValue
) -> None:
    root = copied(baseline, tmp_path / "repo")
    raw = policy(root, "links")
    raw[field] = value
    rewrite(root, "links", raw)
    reject(root, "Invalid digital-name policy fields")


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("sequence", "Digital-name policy version sequence is incomplete"),
        ("empty", "Digital-name policy version sequence is incomplete"),
        ("path", "Digital-name policy indexed path mismatch"),
        ("predecessor", "Digital-name policy predecessor mismatch"),
        ("hash", "Digital-name policy indexed hash mismatch"),
        ("missing", "Digital-name policy indexed member is missing"),
        ("orphan", "Unindexed digital-name policy input"),
        ("identity", "Digital-name policy envelope identity mismatch"),
        ("content_keys", "Digital-name policy content projection mismatch"),
        ("content_id", "Digital-name policy content projection mismatch"),
        ("projection", "Previously adopted digital-name document projection changed"),
        ("semantics", "Unsupported digital-name policy rule semantics"),
    ],
)
def test_entry_and_rule_closure(
    baseline: tuple[Path, str], tmp_path: Path, fault: str, message: str
) -> None:
    root = copied(baseline, tmp_path / "repo")
    index_path = root / "authored" / INDEX
    index = object_value(read_yaml(index_path))
    entries = array(object_value(index["policies"])[LINKS])
    entry = object_value(entries[0])
    if fault in {"sequence", "path", "predecessor", "hash"}:
        bad_values: dict[str, JsonValue] = {
            "sequence": 2,
            "path": "../outside.yaml",
            "predecessor": digest(b"invalid"),
            "hash": digest(b"invalid"),
        }
        entry[
            {
                "sequence": "version",
                "path": "path",
                "predecessor": "predecessor",
                "hash": "hash",
            }[fault]
        ] = bad_values[fault]
        index_path.write_bytes(canonical(index))
    elif fault == "empty":
        object_value(index["policies"])[LINKS] = []
        index_path.write_bytes(canonical(index))
    elif fault == "missing":
        (root / "authored" / str(entry["path"])).unlink()
    elif fault == "orphan":
        (root / "authored/digital-name-policies/orphan.yaml").write_text("{}")
    else:
        raw = policy(root, "links")
        if fault == "identity":
            raw["policy_id"] = "synthetic-other"
        elif fault == "content_keys":
            object_value(raw["content"])["unsupported"] = True
        elif fault == "content_id":
            object_value(raw["content"])["policy_id"] = "synthetic-other"
        else:
            object_value(raw["content"])["rule"] = "Unsupported rule"
            if fault == "semantics":
                raw["approved_document_hash"] = digest(b"synthetic new document")
        rewrite(root, "links", raw)
    reject(root, message)


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("reviewer", "Invalid digital-name policy fields"),
        ("precision", "Invalid digital-name policy fields"),
        ("doc_hash", "Digital-name approval hash closure mismatch"),
        ("initial_hash", "Digital-name approval hash closure mismatch"),
        ("list_hash", "Digital-name approved exclusion hash mismatch"),
        ("event_order", "Digital-name approval events must be sorted and unique"),
        ("evidence_path", "Unsafe digital-name approval evidence key"),
        ("evidence_hash", "Digital-name approval evidence hash mismatch"),
        (
            "no_button",
            "Digital-name approval requires exactly one explicit policy button",
        ),
        (
            "button_value",
            "Digital-name approval button does not approve this policy text",
        ),
        (
            "button_text",
            "Digital-name approval button does not approve this policy text",
        ),
        (
            "button_time",
            "Digital-name approval button does not approve this policy text",
        ),
        ("message", "Digital-name approval message evidence mismatch"),
        ("disclosure_order", "Digital-name disclosures must be sorted and unique"),
        ("disclosure_uuid", "Digital-name disclosed change lacks timely acceptance"),
        ("disclosure_time", "Digital-name disclosed change lacks timely acceptance"),
    ],
)
def test_approval_is_not_a_background_event(  # ruff: ignore[complex-structure,too-many-branches] -- one small immutable receipt is independently corrupted along each approval edge
    baseline: tuple[Path, str], tmp_path: Path, fault: str, message: str
) -> None:
    root = copied(baseline, tmp_path / "repo")
    approval = receipt(root, "links")
    events = array(approval["approval_events"])
    button, oral = object_value(events[0]), object_value(events[1])
    changes = array(approval["disclosed_changes"])
    if fault == "reviewer":
        approval["reviewed_by"] = "Another reviewer"
    elif fault == "precision":
        approval["reviewed_precision"] = "day"
    elif fault == "doc_hash":
        approval["approved_document_hash"] = digest(b"other")
    elif fault == "initial_hash":
        approval["initial_exclusions_hash"] = digest(b"other")
    elif fault == "list_hash":
        excluded = exclusion(root, "links")
        excluded["approved_list_hash"] = digest(b"other")
        rewrite(root, "links", excluded=excluded)
        reject(root, message)
        return
    elif fault == "event_order":
        approval["approval_events"] = list(reversed(events))
    elif fault == "evidence_path":
        object_value(approval["evidence_hashes"])["/private/path"] = digest(b"other")
    elif fault == "evidence_hash":
        object_value(approval["evidence_hashes"])["links-policy.plain.md"] = digest(
            b"other"
        )
    elif fault == "no_button":
        approval["approval_events"] = [oral]
    elif fault == "button_value":
        object_value(button["value"])["value"] = "submitted_answers"
    elif fault == "button_text":
        object_value(button["value"])["text_sha256"] = digest(b"other")
    elif fault == "button_time":
        button["at"] = "2026-10-02T20:32:21.779Z"
    elif fault == "message":
        oral["locator"] = "not-this-uuid"
    elif fault == "disclosure_order":
        approval["disclosed_changes"] = list(reversed(changes))
    elif fault == "disclosure_uuid":
        object_value(changes[0])["accepted_message_uuid"] = (
            "00000000-0000-0000-0000-000000000000"
        )
    elif fault == "disclosure_time":
        object_value(changes[0])["disclosed_at"] = "2026-10-02T20:34:00Z"
    rewrite(root, "links", approval=approval, close=False)
    reject(root, message)


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("revision", "Policy authored revision must be a full Git SHA"),
        ("absent", "Digital-name policy index is missing"),
        ("disk", "Digital-name policy bytes differ from authored revision"),
        ("extra_disk", "Digital-name policy disk closure differs from immutable tree"),
        ("git_symlink", "Unsafe immutable digital-name policy file"),
        ("disk_symlink", "Symlink digital-name policy input"),
    ],
)
def test_git_and_disk_are_both_immutable(
    baseline: tuple[Path, str], tmp_path: Path, fault: str, message: str
) -> None:
    root = copied(baseline, tmp_path / "repo")
    revision = baseline[1]
    target = root / "authored/digital-name-policies" / LINKS / "001.policy.yaml"
    if fault == "revision":
        revision = "HEAD"
    elif fault == "absent":
        (root / "authored" / INDEX).unlink()
        revision = commit(root)
    elif fault == "disk":
        target.write_bytes(target.read_bytes() + b"\n# different bytes\n")
    elif fault == "extra_disk":
        (target.parent / "orphan.yaml").write_text("{}")
    else:
        copy = tmp_path / "file.yaml"
        copy.write_bytes(target.read_bytes())
        target.unlink()
        target.symlink_to(copy)
        if fault == "git_symlink":
            revision = commit(root)
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        load(root / "authored", root, revision)


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("batches", "Digital-name policy catalogue batch closure mismatch"),
        ("recipes", "Digital-name policy catalogue recipes are incomplete"),
        ("provider", "Unsupported digital-name policy catalogue recipe"),
        ("registry", "Digital-name link registry pins mismatch"),
        ("reason", "Digital-name exclusion reason is blank"),
        ("kind", "Invalid digital-name policy fields"),
        ("target", "Digital-name exclusion target ID mismatch"),
        ("duplicate", "Digital-name exclusions must be sorted and unique"),
        ("receipt_changed", "Previously adopted digital-name approval changed"),
    ],
)
def test_remaining_closed_fields(
    baseline: tuple[Path, str], tmp_path: Path, fault: str, message: str
) -> None:
    root = copied(baseline, tmp_path / "repo")
    purpose = "links"
    raw = policy(root, purpose)
    excluded = exclusion(root, purpose)
    approval = receipt(root, purpose)
    content = object_value(raw["content"])
    if fault == "receipt_changed":
        approval["note"] = "Unapproved replacement note"
    else:
        raw["approved_document_hash"] = digest(b"synthetic replacement")
        pins = object_value(content["catalogue_pins"])
        if fault == "batches":
            pins["source_batches"] = []
        elif fault == "recipes":
            object_value(
                object_value(pins["parser_and_registry_configuration"])[
                    "translation_recipes"
                ]
            ).pop("translation-sv1-v1")
        elif fault == "provider":
            object_value(
                object_value(
                    object_value(pins["parser_and_registry_configuration"])[
                        "translation_recipes"
                    ]
                )["translation-sv1-v1"]
            )["config"] = {"provider": "svwb"}
        elif fault == "registry":
            object_value(content["registry_pins"])["index_hash"] = digest(
                b"different index"
            )
        else:
            item: dict[str, JsonValue] = {
                "source_lang": "ja",
                "source_name_hash": digest(b"Synthetic card"),
                "reason": "Synthetic exclusion",
                "kind": "name",
            }
            if fault == "reason":
                item["reason"] = " "
            elif fault == "kind":
                item["kind"] = "unsupported"
            elif fault == "target":
                item = {
                    "kind": "card_target",
                    "card_id": "c:" + "1" * 32,
                    "game": "sv1",
                    "official_id": "123",
                    "reason": "Synthetic exclusion",
                }
            excluded["entries"] = [item, item] if fault == "duplicate" else [item]
    rewrite(root, purpose, raw, approval, excluded)
    reject(root, message)


def test_missing_immutable_revision_is_rejected(baseline: tuple[Path, str]) -> None:
    root, _ = baseline
    with pytest.raises(ValueError, match=r"^Policy immutable tree is unavailable$"):
        load(root / "authored", root, "0" * 40)


def test_purpose_must_be_explicit_and_unique(baseline: tuple[Path, str]) -> None:
    root, revision = baseline
    snapshot = load(root / "authored", root, revision)
    from dataclasses import replace  # ruff: ignore[import-outside-top-level] -- this fault only alters the detached snapshot

    with pytest.raises(
        ValueError,
        match=r"^Digital-name policy purpose must select exactly one policy$",
    ):
        replace(snapshot, policies=()).effective("links")


def test_versioned_names_are_rejected_while_links_remain_valid(
    baseline: tuple[Path, str],
) -> None:
    raw = policy(baseline[0], "links")
    assert model(Policy, raw).purpose == "links"
    raw["purpose"] = "names"
    with pytest.raises(ValueError, match=r"^Invalid digital-name policy fields$"):
        model(Policy, raw)
