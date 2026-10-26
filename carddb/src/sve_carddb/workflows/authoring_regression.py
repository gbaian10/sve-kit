"""Verify prototype removals against a fixed Git tree and frozen AST positions."""

import re
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- read a fixed Git tree without executing a shell
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.authored import check_path
from sve_carddb.core.json import array, canonical, digest, object_value, parse, string
from sve_carddb.domains.effects.author_current import CardAccepted
from sve_carddb.domains.effects.author_handwritten import ConfigurationError
from sve_carddb.domains.effects.author_inputs import (
    AuthorInputError,
    Diagnostic,
    parse_document,
)

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_current import CardResult, CurrentCard
    from sve_carddb.domains.effects.author_set import AuthorInputSet
    from sve_carddb.workflows.authoring import Workspace


def _git(repo: Path, *args: str) -> bytes:
    executable = shutil.which("git")
    if executable is None:
        raise FileNotFoundError("Git is required for prototype regression")
    try:
        return subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed git operations and validated SHA, with no shell
            [executable, "-C", str(repo), *args], check=True, capture_output=True
        ).stdout
    except subprocess.CalledProcessError:
        raise FileNotFoundError(
            "Prototype regression Git base or file unavailable"
        ) from None


def _reject(reason: str, number: str | None = None) -> None:
    raise AuthorInputError(Diagnostic("source", reason, card_no=number))


def _positions(
    number: str, baseline: dict[str, JsonValue], result: CardAccepted, card: CurrentCard
) -> None:
    inventory = object_value(parse(result.source_inventory))
    uses = [
        object_value(object_value(item)["source_use"])
        for item in array(inventory["abilities"])
    ]
    uses.extend(
        object_value(object_value(item)["source_use"])
        for item in array(inventory["non_body"])
        if object_value(item)["kind"] == "printed_keywords"
    )
    expected: list[dict[str, JsonValue]] = []
    for item in array(baseline["positions"]):
        position = object_value(item)
        for fragment_value in array(position["fragments"]):
            fragment = object_value(fragment_value)
            expected.append(
                {
                    "binding_id": fragment["id"],
                    "face": position["face"],
                    "source_hash": fragment["source_sha256"],
                    "segments": object_value(fragment["span"])["segments"],
                    "field": position["field"],
                    "ordinal": position["section"],
                    "line_ordinal": int(str(position["line"])) - 1,
                }
            )
    actual = [
        {
            "binding_id": use["binding_id"],
            "face": card.face_positions[
                card.revisions[
                    string(
                        object_value(object_value(use["occurrence"])["owner"])[
                            "revision_id"
                        ]
                    )
                ]
            ][0]
            - 1,
            **{
                key: object_value(use["occurrence"])[key]
                for key in (
                    "source_hash",
                    "segments",
                    "field",
                    "ordinal",
                    "line_ordinal",
                )
            },
        }
        for use in uses
    ]
    # A fragment may declare several roots; the frozen baseline tracks fragments.
    if {canonical(item) for item in actual} != {
        canonical(item) for item in expected
    } or baseline.get("unclassified_positions"):
        _reject("prototype_position_mismatch", number)


def check_regression(  # ruff: ignore[complex-structure, too-many-branches, too-many-locals] -- keep the complete ordered boundary in one operation
    workspace: Workspace,
    base: str,
    baseline_root: Path,
    inputs: AuthorInputSet,
    results: tuple[CardResult, ...],
    *,
    expected_removed: frozenset[str] | None = None,
) -> dict[str, JsonValue]:
    """Only removed target cards may differ; baseline bytes never update in place."""
    repo = workspace.inputs.repo
    if re.fullmatch(r"[0-9a-f]{40}", base) is None:
        raise ConfigurationError("Prototype regression requires a full Git SHA")
    if not baseline_root.is_absolute() or not baseline_root.is_dir():
        raise FileNotFoundError("Prototype regression baseline is missing")
    check_path(Path(baseline_root.anchor), baseline_root)
    relative = baseline_root.relative_to(repo).as_posix()
    names = (
        _git(repo, "ls-tree", "-r", "--name-only", base, "--", relative)
        .decode()
        .splitlines()
    )
    actual_names = {
        path.relative_to(repo).as_posix()
        for path in baseline_root.rglob("*")
        if path.is_file()
    }
    if not names:
        raise FileNotFoundError("Prototype regression baseline is absent at base")
    if set(names) != actual_names:
        _reject("prototype_baseline_files_changed")
    for name in names:
        path = repo / name
        check_path(repo, path)
        if path.read_bytes() != _git(repo, "show", f"{base}:{name}"):
            _reject("prototype_baseline_bytes_changed")
    old: dict[str, JsonValue] = {}
    before_files: dict[str, dict[str, JsonValue]] = {}
    for name in (
        _git(repo, "ls-tree", "-r", "--name-only", base, "--", "authored/rules/effects")
        .decode()
        .splitlines()
    ):
        tree = object_value(parse_document(_git(repo, "show", f"{base}:{name}")).value)
        old.update(object_value(tree["cards"]))
        before_files[name.removeprefix("authored/")] = tree
    now: dict[str, JsonValue] = {}
    after_files = {
        file.path: object_value(parse_document(file.raw).value)
        for file in inputs.prototype_files
    }
    if before_files.keys() != after_files.keys():
        _reject("prototype_file_inventory_changed")
    for prototype_path, tree in after_files.items():
        before = before_files[prototype_path]
        if {key: item for key, item in tree.items() if key != "cards"} != {
            key: item for key, item in before.items() if key != "cards"
        }:
            _reject("prototype_envelope_changed")
        previous = object_value(before["cards"])
        for number, value in object_value(tree["cards"]).items():
            if previous.get(number) != value:
                _reject("prototype_nontarget_changed", number)
        now.update(object_value(tree["cards"]))
    removed = frozenset(old.keys() - now.keys())
    if now.keys() - old.keys() or (
        expected_removed is not None and removed != expected_removed
    ):
        _reject("prototype_removal_set_mismatch")
    accepted = {
        result.record.record_key: result
        for result in results
        if isinstance(result, CardAccepted)
    }
    for number in sorted(removed):
        if number not in accepted:
            _reject("prototype_replacement_not_current", number)
        pack = baseline_root / f"{number.split('-', 1)[0]}.json"
        legacy = old[number]
        baseline = object_value(object_value(parse(pack.read_bytes()))[number])
        if legacy != old[number] or digest(canonical(legacy)).removeprefix(
            "sha256:"
        ) != string(baseline["ast_sha256"]):
            _reject("prototype_ast_mismatch", number)
        result = accepted[number]
        _positions(
            number,
            baseline,
            result,
            workspace.current.card_payload(result.record.card_id),
        )
    return {
        "base": base,
        "removed": list[JsonValue](sorted(removed)),
        "baseline_bytes_unchanged": True,
    }
