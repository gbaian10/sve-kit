"""Offline synthetic archive integration, output privacy and whole-checkpoint exit gates."""

import argparse
import copy
import json
import re
import shutil
import sys
from dataclasses import dataclass
from typing import TYPE_CHECKING

import pytest

import sve_carddb.template_parameters.__main__ as cli
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.template_parameters import inventory
from sve_carddb.template_parameters.inventory import Candidates, build, summary
from sve_carddb.template_parameters.models import Candidate
from sve_carddb.template_parameters.numeric_rules import configuration
from sve_carddb.template_parameters.output import write
from sve_carddb.template_parameters.references import References
from sve_carddb.template_parameters.rule_candidates import BY_ID
from sve_carddb.template_parameters.rule_candidates import (
    configuration as candidate_configuration,
)

from .template_source_fixtures import template_case as template_case  # ruff: ignore[useless-import-alias] -- reusable immutable offline Git/archive fixture

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.template_parameters.inventory import Field
    from sve_carddb.template_parameters.spans import Located
    from sve_carddb.template_sources.inventory import Scan
    from sve_carddb.template_sources.normalizer import Part

    from .template_source_fixtures import Case


@dataclass(frozen=True)
class Result:
    args: argparse.Namespace
    report: dict[str, JsonValue]


@pytest.fixture(scope="module")
def parameter_result(
    template_case: Case, tmp_path_factory: pytest.TempPathFactory
) -> Result:
    root = tmp_path_factory.mktemp("parameter-candidates")
    authored = template_case.repository / "authored"
    proposals = root / "proposals.json"
    proposals.write_bytes(canonical({"bindings": []}))
    basis = root / "basis.md"
    basis.write_text("Synthetic confirmation basis\n")
    args = argparse.Namespace(
        store=template_case.store,
        store_id=template_case.scan.entries[0].source_ref.store_id,
        batch_id=template_case.batch,
        repository=template_case.repository,
        code_revision=template_case.revision,
        authored=authored,
        legacy=template_case.legacy_path,
        vocabulary_proposals=proposals,
        vocabulary_basis=basis,
        output=root / "result",
    )
    return Result(args, cli.run(args))


def test_candidate_inventory_replays_every_first_checkpoint_entry_and_field(
    template_case: Case, parameter_result: Result
) -> None:
    report = parameter_result.report
    parameters = object_value(report["parameters"])
    assert parameters["entry_count"] == len(template_case.scan.entries)
    assert parameters["field_count"] == len(template_case.scan.fields) - 1
    assert parameters["source_span_roundtrip_complete"] is True
    assert parameters["candidate_only"] is True
    assert parameters["parameter_complete"] is False
    assert object_value(report["source_coverage"])["complete"] is True
    assert report["complete"] is False
    assert (
        object_value(report["parameter_recipe"])["code_revision"]
        == template_case.revision
    )
    assert object_value(report["fingerprints"])["complete"] is True
    assert object_value(report["legacy_member_coverage"])["complete"] is True
    config = object_value(object_value(report["parameter_recipe"])["config"])
    assert config["numeric_classifier"] == configuration()
    assert config["candidate_classifier"] == candidate_configuration()
    glossary = object_value(object_value(config["references"])["glossary"])
    assert glossary["authored_revision"] == template_case.revision
    assert glossary["index_hash"] == digest(
        (template_case.repository / "authored/translations/index.yaml").read_bytes()
    )
    assert (parameter_result.args.output / "rule-candidates.jsonl").read_bytes() == b""


def test_cli_passes_the_exact_opt_in_set_without_changing_the_original_candidates(
    parameter_result: Result, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    args = copy.copy(parameter_result.args)
    args.enable_candidate_rule = list(BY_ID)
    args.output = tmp_path / "enabled"
    original = inventory.build
    seen: list[tuple[str, ...]] = []

    def record(
        sources: FrozenSources,
        scan: Scan,
        refs: References,
        *,
        enabled_rules: tuple[str, ...] = (),
    ) -> Candidates:
        seen.append(enabled_rules)
        return original(sources, scan, refs, enabled_rules=enabled_rules)

    monkeypatch.setattr(cli, "build", record)
    report = cli.run(args)
    assert seen == [tuple(sorted(BY_ID))]
    config = object_value(object_value(report["parameter_recipe"])["config"])
    assert config["candidate_classifier"] == candidate_configuration(tuple(BY_ID))
    assert report["complete"] is False
    for name in ("candidates.jsonl", "legacy-lineage.jsonl", "field-spans.jsonl"):
        assert (args.output / name).read_bytes() == (
            parameter_result.args.output / name
        ).read_bytes()


def test_outputs_have_only_hashes_ranges_schemas_and_fixed_reasons(
    parameter_result: Result,
) -> None:
    root = parameter_result.args.output
    files = object_value(parameter_result.report["files"])
    for name, pin in files.items():
        raw = (root / name).read_bytes()
        assert digest(raw) == object_value(pin)["exact_hash"]
        assert len(raw.splitlines()) == object_value(pin)["rows"]
        for forbidden in (
            b"Synthetic token",
            b"Alpha",
            b"Gamma",
            b"Synthetic reminder",
            b"Header only",
        ):
            assert forbidden not in raw
    entries = [
        Candidate.model_validate_json(line)
        for line in (root / "candidates.jsonl").read_bytes().splitlines()
    ]
    assert all(
        c.model_dump()["adoption_status"] == "candidate_only"
        and c.model_dump()["supersedes_id"] is None
        for c in entries
    )
    parents = [
        object_value(parse(line))
        for line in (root / "legacy-lineage.jsonl").read_bytes().splitlines()
    ]
    assert all(p["supersedes_id"] is None for p in parents)


@pytest.mark.parametrize(
    "change", ["missing_entry", "duplicate_entry", "missing_field"]
)
def test_inventory_cannot_shrink_or_duplicate_first_checkpoint_inputs(
    template_case: Case, change: str
) -> None:
    scan = copy.deepcopy(template_case.scan)
    if change == "missing_entry":
        scan.entries.pop(0)
        message = "Parameter candidates must locate every first-checkpoint entry"
    elif change == "duplicate_entry":
        scan.entries.append(scan.entries[0])
        message = "Parameter candidates must locate every first-checkpoint entry"
    else:
        scan.fields.pop(0)
        message = (
            "Parameter span proofs must cover every exact first-checkpoint text field"
        )
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        build(
            FrozenSources(
                template_case.store,
                template_case.scan.entries[0].source_ref.store_id,
                template_case.batch,
            ),
            scan,
            References(),
        )


def test_glossary_revision_cannot_be_a_label_for_different_exact_bytes(
    parameter_result: Result, tmp_path: Path
) -> None:
    args = copy.copy(parameter_result.args)
    args.authored = tmp_path / "authored"
    shutil.copytree(parameter_result.args.authored, args.authored)
    index = args.authored / "translations/index.yaml"
    index.write_bytes(index.read_bytes() + b"\n")
    args.output = tmp_path / "result"
    with pytest.raises(
        ValueError,
        match=r"\ACandidate glossary inputs must match their exact Git revision\Z",
    ):
        cli.run(args)
    assert not args.output.exists()


@pytest.mark.parametrize("location", ["existing", "inside", "symlink"])
def test_atomic_output_rejects_existing_immutable_or_symlink_targets(
    template_case: Case, tmp_path: Path, location: str
) -> None:
    if location == "existing":
        output = tmp_path
    elif location == "inside":
        output = template_case.repository / "output"
    else:
        link = tmp_path / "link"
        link.symlink_to(template_case.store, target_is_directory=True)
        output = link / "output"
    with pytest.raises(
        ValueError,
        match=r"\AParameter output must be new and outside all immutable inputs\Z",
    ):
        write(
            output,
            Candidates(),
            template_case.scan,
            {},
            inputs=(template_case.repository, template_case.store),
        )


@pytest.mark.parametrize(
    ("source_complete", "parameter_complete", "expected"),
    [(True, True, 0), (False, True, 1), (True, False, 1), (False, False, 1)],
)
def test_cli_exit_is_not_success_when_only_one_independent_gate_passes(
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    source_complete: bool,
    parameter_complete: bool,
    expected: int,
) -> None:
    report = {
        "complete": source_complete and parameter_complete,
        "source_coverage": {"complete": source_complete},
        "parameters": {"parameter_complete": parameter_complete},
    }
    monkeypatch.setattr(cli, "run", lambda _: report)
    monkeypatch.setattr(
        sys,
        "argv",
        [
            "parameters",
            *[
                v
                for name in (
                    "store",
                    "repository",
                    "authored",
                    "legacy",
                    "vocabulary-proposals",
                    "vocabulary-basis",
                    "output",
                    "store-id",
                    "batch-id",
                    "code-revision",
                )
                for v in ("--" + name, "synthetic")
            ],
        ],
    )
    with pytest.raises(SystemExit) as raised:
        cli.main()
    assert raised.value.code == expected
    assert json.loads(capsys.readouterr().out) == {
        "source_coverage": source_complete,
        "parameter_complete": parameter_complete,
        "complete": source_complete and parameter_complete,
    }


def test_term_diagnostics_do_not_turn_into_bindings_or_false_completion() -> None:
    result = summary(Candidates())
    assert result["term_mentions_are_bindings"] is False


def test_run_cannot_ignore_failed_source_coverage_even_when_parameters_are_complete(
    template_case: Case,
    parameter_result: Result,
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    scan = copy.deepcopy(template_case.scan)
    scan.failures.append({"reason": "unknown_effect_presence"})
    monkeypatch.setattr(cli, "scan_batch", lambda *_args, **_kwargs: scan)
    monkeypatch.setattr(cli, "summary", lambda _: {"parameter_complete": True})
    args = copy.copy(parameter_result.args)
    args.output = tmp_path / "result"
    report = cli.run(args)
    assert object_value(report["source_coverage"])["complete"] is False
    assert object_value(report["parameters"])["parameter_complete"] is True
    assert report["complete"] is False


def test_cli_error_output_never_prints_source_or_validation_details(
    parameter_result: Result,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    def invalid(_: argparse.Namespace) -> None:
        raise ValueError("Synthetic forbidden source detail")

    monkeypatch.setattr(cli, "run", invalid)
    args = parameter_result.args
    monkeypatch.setattr(
        sys,
        "argv",
        [
            "parameters",
            *[
                v
                for name, value in vars(args).items()
                for v in ("--" + name.replace("_", "-"), str(value))
            ],
        ],
    )
    with pytest.raises(SystemExit) as raised:
        cli.main()
    assert raised.value.code == 2
    output = capsys.readouterr()
    assert not output.out
    assert (
        output.err
        == "Template parameter candidates failed; immutable inputs or evidence could not be verified\n"
    )


@pytest.mark.parametrize("change", ["missing", "extra", "duplicate"])
def test_candidate_output_must_cover_all_and_only_first_checkpoint_entries(
    template_case: Case, monkeypatch: pytest.MonkeyPatch, change: str
) -> None:
    original = inventory._candidate

    def damaged(
        result: Candidates,
        context: Field,
        part: Part,
        position: Located,
        refs: References,
    ) -> None:
        original(result, context, part, position, refs)
        if change == "missing":
            result.entries.pop()
        elif change == "extra":
            result.entries.append(
                result.entries[-1].model_copy(update={"inventory_id": "invented-entry"})
            )
        else:
            result.entries.append(result.entries[-1])

    monkeypatch.setattr(inventory, "_candidate", damaged)
    with pytest.raises(
        ValueError,
        match=r"\AParameter candidates must cover all and only first-checkpoint entries\Z",
    ):
        build(
            FrozenSources(
                template_case.store,
                template_case.scan.entries[0].source_ref.store_id,
                template_case.batch,
            ),
            template_case.scan,
            References(),
        )


def test_legacy_normalized_replay_cannot_be_replaced_with_a_different_value(
    template_case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(inventory, "replay", lambda *_: "Synthetic wrong replay")
    with pytest.raises(
        ValueError,
        match=r"\AParameter source recipe must reproduce exact legacy normalized bytes\Z",
    ):
        build(
            FrozenSources(
                template_case.store,
                template_case.scan.entries[0].source_ref.store_id,
                template_case.batch,
            ),
            template_case.scan,
            References(),
        )
