"""Offline C+hash candidates and independently pinned historical F1 replay."""

import argparse
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import canonical, object_value, parse
from sve_carddb.source_archive import ArchiveError
from sve_carddb.template_parameter_rules.replay import ProposalInputs
from sve_carddb.template_semantics import candidates
from sve_carddb.template_semantics.audit import host_context
from sve_carddb.template_semantics.budget import (
    Budget,
    Monitor,
    ReplayBudgetExceededError,
)
from sve_carddb.template_semantics.generation import generate
from sve_carddb.template_semantics.session import replay
from sve_carddb.template_translations.replay_models import (
    EffectInputs,
    FlavorReplayInputs,
)
from sve_carddb.template_translations.sources import TemplateSources

if TYPE_CHECKING:
    from pydantic import JsonValue


def run(args: argparse.Namespace) -> dict[str, JsonValue]:
    """Explicit immutable business inputs and host pins never fall back to live data."""
    budget = (
        Budget()
        if args.budget is None
        else Budget.model_validate_json(args.budget.read_bytes())
    )
    with Monitor(budget).watchdog():
        return _run(args, budget)


def _run(args: argparse.Namespace, budget: Budget) -> dict[str, JsonValue]:
    """Candidate reload and input I/O share the same budget as source replay."""
    repository = PinnedRepository(args.repository)
    stores = {}
    for value in args.store:
        identifier, separator, location = value.partition("=")
        if not separator or not identifier or identifier in stores:
            raise ValueError("Replay stores require distinct explicit ID=PATH values")
        stores[identifier] = Path(location)
    if (args.vocabulary is None) != (args.vocabulary_basis is None):
        raise ValueError("Replay proposed vocabulary requires both explicit files")
    proposals = (
        None
        if args.vocabulary is None
        else ProposalInputs(
            args.vocabulary.read_bytes(), args.vocabulary_basis.read_bytes()
        )
    )
    sources = TemplateSources(
        repository,
        stores,
        main_revision=args.host,
        legacy_bytes=args.legacy.read_bytes(),
        proposals=proposals,
    )
    if args.command == "generate":
        with Monitor(budget).watchdog():
            host_context(repository, args.host, {})
            inputs = (
                EffectInputs(kind="effect")
                if args.kind == "effect"
                else FlavorReplayInputs.model_validate_json(args.inputs.read_bytes())
            )
            config = (
                object_value(parse(args.inputs.read_bytes()))
                if args.kind == "effect"
                else None
            )
            pins, context, result = generate(
                sources, args.host, inputs, effect_config=config
            )
            checksum = candidates.write(
                args.output,
                pins,
                context,
                result,
                inputs=(args.repository, *stores.values(), args.legacy, args.inputs),
            )
        return {
            "candidate_index_hash": checksum,
            "expected_outputs": context.expected_outputs.model_dump(mode="json"),
            "adopted": False,
        }
    inventories = candidates.read(args.candidate, args.index_hash)
    if args.output.resolve().is_relative_to(args.candidate.resolve()) or any(
        args.output.resolve().is_relative_to(p.resolve())
        for p in (args.repository, *stores.values())
    ):
        raise ValueError("Replay F1 output must be outside immutable inputs")
    return replay(inventories, sources, args.host, args.output, budget=budget)


def main() -> None:
    """No raw source or validation input is ever printed on refusal."""
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command", required=True)
    for name in ("generate", "replay"):
        command = subcommands.add_parser(name)
        command.add_argument("--repository", required=True, type=Path)
        command.add_argument("--host", required=True)
        command.add_argument("--store", action="append", default=[], metavar="ID=PATH")
        command.add_argument("--legacy", required=True, type=Path)
        command.add_argument("--vocabulary", type=Path)
        command.add_argument("--vocabulary-basis", type=Path)
        command.add_argument("--budget", type=Path)
        command.add_argument("--output", required=True, type=Path)
        if name == "generate":
            command.add_argument("--kind", choices=("effect", "flavor"), required=True)
            command.add_argument("--inputs", required=True, type=Path)
        else:
            command.add_argument("--candidate", required=True, type=Path)
            command.add_argument("--index-hash", required=True)
    try:
        result = run(parser.parse_args())
    except ReplayBudgetExceededError:
        sys.stderr.write("replay_budget_exceeded\n")
        raise SystemExit(2) from None
    except ValueError, OSError, TypeError, LookupError, ArchiveError:
        sys.stderr.write(
            "Template replay refused; no result was adopted or published.\n"
        )
        raise SystemExit(2) from None
    sys.stdout.write(canonical(result).decode() + "\n")


if __name__ == "__main__":
    main()
