"""Offline parameter/source-span candidates; this command does not adopt or translate templates."""

import argparse
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from sve_carddb.build_inputs import BuildContext
from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.snapshot.values import canonical, digest, object_value
from sve_carddb.source_archive import ArchiveError
from sve_carddb.template_parameters.analysis import VERSION_PARAMETERS
from sve_carddb.template_parameters.inventory import build, summary
from sve_carddb.template_parameters.numeric_rules import configuration
from sve_carddb.template_parameters.output import write
from sve_carddb.template_parameters.references import adopted
from sve_carddb.template_parameters.rule_candidates import BY_ID, selection
from sve_carddb.template_parameters.rule_candidates import (
    configuration as candidate_configuration,
)
from sve_carddb.template_sources.checkpoint import compare, parse_legacy
from sve_carddb.template_sources.inventory import coverage, scan_batch
from sve_carddb.template_sources.models import Recipe
from sve_carddb.template_sources.pins import recipes
from sve_carddb.text_observations.vocabulary import Vocabulary
from sve_carddb.translations.sources import CODE_PATH, RUNTIME, Sources

if TYPE_CHECKING:
    from pydantic import JsonValue


def _evidence(args: argparse.Namespace) -> Sources:
    dependencies = PinnedRepository(args.repository).read_many(
        args.code_revision, RUNTIME
    )
    parsers: dict[str, JsonValue] = {}
    for provider in ("jp", "sv1", "svwb"):
        config: dict[str, JsonValue] = {"provider": provider}
        parsers["translation-" + provider + "-v1"] = {
            "version": "translation-" + provider + "-v1",
            "program_revision": args.code_revision,
            "code_path": CODE_PATH,
            "code_hash": digest(dependencies[CODE_PATH]),
            "config": config,
            "config_hash": digest(canonical(config)),
        }
    context = BuildContext.from_inputs(
        args.code_revision, dependencies, {"translation_recipes": parsers}
    )
    return Sources({args.store_id: args.store}, args.repository, context)


def run(args: argparse.Namespace) -> dict[str, JsonValue]:
    """Pin full first-party code, exact adopted concept closure and proposed vocabulary inputs."""
    pins = recipes(args.repository, args.code_revision)
    enabled = selection(tuple(getattr(args, "enable_candidate_rule", ())))
    sources = FrozenSources(args.store, args.store_id, args.batch_id)
    refs = adopted(args.authored, _evidence(args))
    proposal_bytes = args.vocabulary_proposals.read_bytes()
    refs.vocabulary = Vocabulary.model_validate_json(proposal_bytes)
    refs.vocabulary.verify()
    refs.pins["vocabulary_proposals_hash"] = digest(proposal_bytes)
    refs.pins["vocabulary_basis_hash"] = digest(args.vocabulary_basis.read_bytes())
    config: dict[str, JsonValue] = {
        "source_recipes": [p.model_dump(mode="json") for p in pins],
        "references": refs.pins,
        "numeric_classifier": configuration(),
        "candidate_classifier": candidate_configuration(enabled),
        "adoption_status": "candidate_only",
    }
    code_path = "carddb/src/sve_carddb/template_parameters/analysis.py"
    content = PinnedRepository(args.repository).read_many(
        args.code_revision, (code_path,)
    )
    parameter_pin = Recipe(
        id=VERSION_PARAMETERS,
        code_revision=args.code_revision,
        code_path=code_path,
        code_hash=digest(content[code_path]),
        config=config,
        config_hash=digest(canonical(config)),
    )
    scan = scan_batch(sources, repository=args.repository, pins=pins)
    candidates = build(sources, scan, refs, enabled_rules=enabled)
    legacy_bytes = args.legacy.read_bytes()
    report: dict[str, JsonValue] = {
        "parameter_candidates_format": 1,
        "code_revision": args.code_revision,
        "parameter_recipe": parameter_pin.model_dump(mode="json"),
        "legacy_file_hash": digest(legacy_bytes),
        "source_coverage": coverage(scan),
        "parameters": summary(candidates),
        **compare(scan, parse_legacy(legacy_bytes)),
    }
    report["complete"] = (
        all(
            object_value(report[key])["complete"] is True
            for key in ("source_coverage", "fingerprints", "legacy_member_coverage")
        )
        and object_value(report["parameters"])["parameter_complete"] is True
    )
    write(
        args.output,
        candidates,
        scan,
        report,
        inputs=(
            args.store,
            args.repository,
            args.authored,
            args.legacy,
            args.vocabulary_proposals,
            args.vocabulary_basis,
        ),
    )
    return report


def main() -> None:
    """Safe diagnostics contain only booleans; unresolved candidates return exit one."""
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "store",
        "repository",
        "authored",
        "legacy",
        "vocabulary-proposals",
        "vocabulary-basis",
        "output",
    ):
        parser.add_argument("--" + name, required=True, type=Path)
    for name in ("store-id", "batch-id", "code-revision"):
        parser.add_argument("--" + name, required=True)
    parser.add_argument(
        "--enable-candidate-rule",
        action="append",
        choices=sorted(BY_ID),
        default=[],
        help="Emit a pending recognition proposal; this switch grants no approval",
    )
    args = parser.parse_args()
    try:
        report = run(args)
    except ValueError, LookupError, TypeError, OSError, ArchiveError:
        sys.stderr.write(
            "Template parameter candidates failed; immutable inputs or evidence could not be verified\n"
        )
        raise SystemExit(2) from None
    sys.stdout.write(
        canonical(
            {
                "source_coverage": object_value(report["source_coverage"])["complete"],
                "parameter_complete": object_value(report["parameters"])[
                    "parameter_complete"
                ],
                "complete": report["complete"],
            }
        ).decode()
        + "\n"
    )
    raise SystemExit(0 if report["complete"] is True else 1)


if __name__ == "__main__":
    main()
