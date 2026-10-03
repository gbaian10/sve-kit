"""Every refusal targets one complete message with a fully valid surrounding fixture."""

import re
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.template_parameter_rules import loader as recognition_loader
from sve_carddb.template_parameter_rules.loader import (
    Loaded,
    load_config,
    load_pairs,
    parse_approval,
    parse_policy,
)
from sve_carddb.template_parameter_rules.repository import immutable

from .adoption_fixtures import commit, git
from .recognition_policy_fixtures import (
    ROOT,
    GitCase,
    pair,
    policy_git,
    policy_repository,
    publish,
)

if TYPE_CHECKING:
    from pathlib import Path

__all__ = ("policy_git", "policy_repository")


def load(repository: Path, revision: str, main: str) -> dict[str, Loaded]:
    return load_pairs(PinnedRepository(repository), revision, main_revision=main)


def test_three_rules_one_conversation_without_samples(
    policy_repository: Path, policy_git: GitCase
) -> None:
    policy, receipt = pair(policy_git.main)
    revision = publish(policy_repository, policy, receipt)
    loaded = load(policy_repository, revision, policy_git.main)["synthetic-v1"]
    assert len(loaded.policy.rules) == 3
    assert {r.event_id for r in loaded.approval.rules} == {"event_20261002_2"}
    assert "sample_ids" not in loaded.approval.model_dump(mode="json")
    pin = loaded.pin.model_dump(mode="json")
    assert (
        load_config(
            {"recognition_policy": pin},
            PinnedRepository(policy_repository),
            main_revision=policy_git.main,
        )
        == loaded
    )
    loaded.policy.rules[0].match_conditions.clear()
    assert loaded.policy.rules[0].match_conditions
    assert len(loaded.dependencies) > 2


def test_full_eight_and_seventeen_historical_bridge(
    policy_repository: Path, policy_git: GitCase
) -> None:
    policy, receipt = pair(policy_git.main, bridge=True)
    revision = publish(policy_repository, policy, receipt)
    loaded = load(policy_repository, revision, policy_git.main)["synthetic-v1"]
    assert loaded.historical_revision == policy_git.historical
    assert len(loaded.policy.rules) == 25
    assert len(loaded.approval.events["event_20261002_2"].authorized_rules) == 17
    old = loaded.approval.events["event_20261002_1"]
    assert all(
        i.condition_hash is None and i.matcher_commit is None
        for i in old.authorized_rules
    )


@pytest.mark.parametrize("kind", ["policy", "approval"])
@pytest.mark.parametrize("mutation", ["format", "extra", "kind"])
def test_closed_envelopes(policy_git: GitCase, kind: str, mutation: str) -> None:
    policy, receipt = pair(policy_git.main)
    value = policy if kind == "policy" else receipt
    if mutation == "format":
        value["parameter_rule_" + kind + "_format"] = True
    elif mutation == "kind":
        value["kind"] = "wording_rule_policy"
    else:
        value["sample_ids"] = []
    parser = parse_policy if kind == "policy" else parse_approval
    with pytest.raises(
        ValueError, match=r"^Invalid recognition " + kind + " envelope$"
    ):
        parser(canonical(value))


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        (
            "condition",
            "Recognition policy conditions differ from the registered matcher",
        ),
        ("role", "Recognition policy conditions differ from the registered matcher"),
        ("version", "Recognition policy conditions differ from the registered matcher"),
        (
            "condition_hash",
            "Recognition policy conditions differ from the registered matcher",
        ),
        ("cases", "Recognition policy must use the registered complete fixed cases"),
        ("rule", "Recognition policy rule is not registered"),
        ("filename", "Recognition policy filename and ID disagree"),
        ("policy_hash", "Recognition receipt policy hash mismatch"),
        ("parser", "Recognition policy parser or normalizer scope is unsupported"),
    ],
)
def test_policy_matcher_refusals(  # ruff: ignore[complex-structure] -- each branch isolates one matcher refusal with valid event evidence
    policy_repository: Path, policy_git: GitCase, mutation: str, message: str
) -> None:
    policy, receipt = pair(policy_git.main)
    rules = array(policy["rules"])
    first = object_value(rules[0])
    if mutation == "condition":
        object_value(first["match_conditions"])["unknown"] = True
    elif mutation == "cases":
        positive = object_value(array(object_value(first["examples"])["positive"])[0])
        positive["case_id"] = "different_fixed_case"
    elif mutation in {"role", "version", "condition_hash"}:
        key = {
            "role": "recognized_role",
            "version": "matcher_version",
            "condition_hash": "condition_hash",
        }[mutation]
        first[key] = digest(b"wrong") if mutation == "condition_hash" else "different"
    elif mutation == "rule":
        first["rule_id"] = "aaa_unknown"
    elif mutation == "parser":
        object_value(policy["scope"])["parser_ids"] = ["other_parser"]
    # Keep event/row identities valid so the matcher guard is the first failing layer.
    if mutation in {"condition_hash", "version", "rule"}:
        row = object_value(array(receipt["rules"])[0])
        event = object_value(object_value(receipt["events"])["event_20261002_2"])
        for key in ("rule_id", "matcher_version", "condition_hash", "matcher_commit"):
            row[key] = first[key] if key != "matcher_version" else row.get(key)
            for items in (
                array(event["authorized_rules"]),
                array(object_value(event["presentation"])["presented_rules"]),
            ):
                object_value(items[0])[key] = first[key]
        row.pop("matcher_version", None)
    revision = publish(policy_repository, policy, receipt)
    if mutation == "filename":
        policy["policy_id"] = "other-v1"
        (policy_repository / (ROOT + "synthetic-v1.policy.yaml")).write_bytes(
            canonical(policy)
        )
        revision = commit(policy_repository)
        # Validate the initial renamed-ID tree directly; history mutation has its own tests.
        git(policy_repository, "checkout", "--orphan", "renamed")
        revision = commit(policy_repository)
    elif mutation == "policy_hash":
        receipt["policy_hash"] = digest(b"wrong")
        (policy_repository / (ROOT + "synthetic-v1.approval.yaml")).write_bytes(
            canonical(receipt)
        )
        git(policy_repository, "checkout", "--orphan", "wrong_hash")
        revision = commit(policy_repository)
    if mutation in {"condition_hash", "version"}:
        message = "Recognition presented identity differs from the registered matcher"
    elif mutation == "rule":
        message = "Recognition presented rule is not registered"
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        load(policy_repository, revision, policy_git.main)


@pytest.mark.parametrize(
    "missing",
    [
        "recognition_policy",
        "hash",
        "approval_receipt_hash",
        "authored_revision",
        "path",
        "policy_id",
    ],
)
def test_five_pin_fields_are_mandatory(
    policy_repository: Path, policy_git: GitCase, missing: str
) -> None:
    policy, receipt = pair(policy_git.main)
    revision = publish(policy_repository, policy, receipt)
    pin = load(policy_repository, revision, policy_git.main)[
        "synthetic-v1"
    ].pin.model_dump(mode="json")
    config: dict[str, JsonValue] = {"recognition_policy": pin}
    if missing == "recognition_policy":
        config.clear()
        message = "Recognition recipe must explicitly declare its policy pin or null"
    else:
        pin.pop(missing)
        message = "Recognition policy requires the complete five-field pin"
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        load_config(
            config, PinnedRepository(policy_repository), main_revision=policy_git.main
        )


def test_explicit_null_is_not_candidate_authorization(policy_git: GitCase) -> None:
    assert (
        load_config(
            {"recognition_policy": None, "enabled_rules": ["suffix_damage_amount"]},
            PinnedRepository(policy_git.repository),
            main_revision=policy_git.main,
        )
        is None
    )
    with pytest.raises(ValueError, match=r"^Recognition pinned policy pair is absent$"):
        load_config(
            {
                "recognition_policy": {
                    "policy_id": "synthetic-v1",
                    "authored_revision": policy_git.main,
                    "path": ROOT + "synthetic-v1.policy.yaml",
                    "hash": digest(b"none"),
                    "approval_receipt_hash": digest(b"none"),
                }
            },
            PinnedRepository(policy_git.repository),
            main_revision=policy_git.main,
        )


@pytest.mark.parametrize("pin_key", ["hash", "approval_receipt_hash"])
def test_pin_hashes(policy_repository: Path, policy_git: GitCase, pin_key: str) -> None:
    policy, receipt = pair(policy_git.main)
    revision = publish(policy_repository, policy, receipt)
    pin = load(policy_repository, revision, policy_git.main)[
        "synthetic-v1"
    ].pin.model_dump(mode="json")
    pin[pin_key] = digest(b"different")
    with pytest.raises(
        ValueError, match=r"^Recognition policy or approval receipt pin hash mismatch$"
    ):
        load_config(
            {"recognition_policy": pin},
            PinnedRepository(policy_repository),
            main_revision=policy_git.main,
        )


@pytest.mark.parametrize("operation", ["formatting", "receipt", "delete", "revert"])
def test_published_pairs_are_immutable(
    policy_repository: Path, policy_git: GitCase, operation: str
) -> None:
    policy, receipt = pair(policy_git.main)
    publish(policy_repository, policy, receipt)
    path = policy_repository / (ROOT + "synthetic-v1.approval.yaml")
    original = path.read_bytes()
    if operation == "delete":
        path.unlink()
        (policy_repository / (ROOT + "synthetic-v1.policy.yaml")).unlink()
    else:
        path.write_bytes(
            original + b"\n"
            if operation != "receipt"
            else canonical({**receipt, "note": "changed"})
        )
    revision = commit(policy_repository)
    if operation == "revert":
        path.write_bytes(original)
        revision = commit(policy_repository)
    with pytest.raises(
        ValueError,
        match=r"^Recognition published policy pairs are immutable exact bytes$",
    ):
        immutable(PinnedRepository(policy_repository), revision)


def test_merge_cannot_hide_a_receipt_modification_followed_by_revert(
    policy_repository: Path, policy_git: GitCase
) -> None:
    policy, receipt = pair(policy_git.main)
    publish(policy_repository, policy, receipt)
    git(policy_repository, "checkout", "-b", "modified-side")
    path = policy_repository / (ROOT + "synthetic-v1.approval.yaml")
    original = path.read_bytes()
    path.write_bytes(original + b"\n")
    commit(policy_repository)
    path.write_bytes(original)
    commit(policy_repository)
    git(policy_repository, "checkout", "main")
    git(
        policy_repository,
        "-c",
        "user.name=Synthetic Reviewer",
        "-c",
        "user.email=synthetic@example.invalid",
        "merge",
        "--no-ff",
        "modified-side",
        "-m",
        "synthetic merge",
    )
    revision = git(policy_repository, "rev-parse", "HEAD")
    assert path.read_bytes() == original
    with pytest.raises(
        ValueError,
        match=r"^Recognition published policy pairs are immutable exact bytes$",
    ):
        immutable(PinnedRepository(policy_repository), revision)


def test_load_pairs_checks_event_history_across_the_entire_directory(
    policy_repository: Path, policy_git: GitCase
) -> None:
    first, first_receipt = pair(policy_git.main)
    publish(policy_repository, first, first_receipt)
    second, second_receipt = pair(policy_git.main, policy_id="synthetic-v2")
    event = object_value(object_value(second_receipt["events"])["event_20261002_2"])
    event["reviewed_by"] = "Different synthetic reviewer"
    revision = publish(policy_repository, second, second_receipt)
    with pytest.raises(
        ValueError,
        match=r"^Recognition immutable event changed across policy versions$",
    ):
        load(policy_repository, revision, policy_git.main)


def test_loader_replays_fixed_cases_after_catalog_equality(
    policy_repository: Path, policy_git: GitCase, monkeypatch: pytest.MonkeyPatch
) -> None:
    policy, receipt = pair(policy_git.main)
    revision = publish(policy_repository, policy, receipt)
    monkeypatch.setattr(recognition_loader, "evaluate", lambda *_: False)
    with pytest.raises(
        ValueError,
        match=r"^Recognition fixed case does not replay its declared answer$",
    ):
        load(policy_repository, revision, policy_git.main)


@pytest.mark.parametrize(
    ("operation", "message"),
    [
        ("half", "Recognition policy directory must contain complete pairs"),
        ("extra", "Recognition policy directory contains unsafe or extra entries"),
        ("nested", "Recognition policy directory contains unsafe or extra entries"),
        ("link", "Recognition policy directory contains unsafe or extra entries"),
        (
            "parent_link",
            "Recognition policy directory contains unsafe or extra entries",
        ),
        ("root_link", "Recognition policy directory contains unsafe or extra entries"),
        ("limit", "Recognition policy files must be smaller than one MiB"),
    ],
)
def test_whole_directory_is_validated(
    policy_repository: Path, operation: str, message: str
) -> None:
    root = policy_repository / ROOT
    root.mkdir(parents=True)
    path = root / "synthetic-v1.policy.yaml"
    path.write_bytes(b"{}")
    (root / "synthetic-v1.approval.yaml").write_bytes(b"{}")
    if operation == "half":
        path.unlink()
    elif operation == "limit":
        path.write_bytes(b" " * 1048576)
    elif operation == "link":
        path.unlink()
        path.symlink_to("synthetic-v1.approval.yaml")
    elif operation in {"parent_link", "root_link"}:
        path.unlink()
        (root / "synthetic-v1.approval.yaml").unlink()
        root.rmdir()
        if operation == "parent_link":
            root.symlink_to("../outside")
        else:
            (policy_repository / "authored").rmdir()
            (policy_repository / "authored").symlink_to("outside")
    else:
        target = root / ("more/item.yaml" if operation == "nested" else "README.md")
        target.parent.mkdir(exist_ok=True)
        target.write_bytes(b"{}")
    revision = commit(policy_repository)
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        immutable(PinnedRepository(policy_repository), revision)


def test_branch_only_matcher_is_rejected(
    policy_repository: Path, policy_git: GitCase
) -> None:
    (policy_repository / "new-file").write_text("synthetic")
    branch_commit = commit(policy_repository)
    policy, receipt = pair(branch_commit)
    revision = publish(policy_repository, policy, receipt)
    with pytest.raises(
        ValueError,
        match=r"^Recognition matcher commit must be reachable from pinned main$",
    ):
        load(policy_repository, revision, policy_git.main)


def test_changed_matcher_bytes_are_rejected(policy_repository: Path) -> None:
    path = (
        policy_repository
        / "carddb/src/sve_carddb/template_parameters/candidate_matching.py"
    )
    path.write_bytes(path.read_bytes() + b"\n# Synthetic modification\n")
    matcher = commit(policy_repository)
    policy, receipt = pair(matcher)
    revision = publish(policy_repository, policy, receipt)
    with pytest.raises(
        ValueError,
        match=r"^Recognition presented matcher bytes differ from the supported runtime$",
    ):
        load(policy_repository, revision, matcher)


@pytest.mark.parametrize(
    "path", ["../synthetic-v1.policy.yaml", ROOT + "other.policy.yaml"]
)
def test_pin_path_is_derived_from_id(policy_git: GitCase, path: str) -> None:
    pin: dict[str, JsonValue] = {
        "policy_id": "synthetic-v1",
        "authored_revision": policy_git.main,
        "path": path,
        "hash": digest(b"none"),
        "approval_receipt_hash": digest(b"none"),
    }
    with pytest.raises(
        ValueError, match=r"^Recognition policy requires the complete five-field pin$"
    ):
        load_config(
            {"recognition_policy": pin},
            PinnedRepository(policy_git.repository),
            main_revision=policy_git.main,
        )


@pytest.mark.parametrize(
    ("value", "message"),
    [
        ("main", "Recognition revision must be a full Git commit SHA"),
        ("a" * 40, "Recognition immutable Git history is unavailable"),
    ],
)
def test_missing_immutable_revision(
    policy_git: GitCase, value: str, message: str
) -> None:
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        immutable(PinnedRepository(policy_git.repository), value)


def test_blob_is_not_a_commit(policy_git: GitCase) -> None:
    blob = git(
        policy_git.repository, "rev-parse", policy_git.main + ":carddb/pyproject.toml"
    )
    with pytest.raises(
        ValueError, match=r"^Recognition revision must identify a Git commit$"
    ):
        immutable(PinnedRepository(policy_git.repository), blob)


def test_shallow_repository_is_not_immutable_history(
    policy_git: GitCase, tmp_path: Path
) -> None:
    shallow = tmp_path / "shallow"
    git(
        tmp_path,
        "clone",
        "--depth=1",
        "file://" + str(policy_git.repository),
        str(shallow),
    )
    with pytest.raises(
        ValueError,
        match=r"^Recognition immutable history requires a complete Git repository$",
    ):
        immutable(PinnedRepository(shallow), policy_git.main)


def test_half_pair_in_history_is_not_repaired_by_current_tree(
    policy_repository: Path, policy_git: GitCase
) -> None:
    policy, receipt = pair(policy_git.main)
    root = policy_repository / ROOT
    root.mkdir(parents=True)
    (root / "synthetic-v1.policy.yaml").write_bytes(canonical(policy))
    commit(policy_repository)
    revision = publish(policy_repository, policy, receipt)
    with pytest.raises(
        ValueError, match=r"^Recognition policy directory must contain complete pairs$"
    ):
        load(policy_repository, revision, policy_git.main)
