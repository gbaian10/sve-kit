"""Reusable offline commands for explicit confirmed-none review renewal."""

import fcntl
from contextlib import contextmanager
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves path annotations at runtime.
from typing import TYPE_CHECKING, Annotated

import typer

from sve_carddb.core.json import canonical
from sve_carddb.domains import physical_sources
from sve_carddb.domains.registry.projection import jp_card
from sve_carddb.domains.registry.storage import read_base_files
from sve_carddb.domains.registry.transitions.review_sources import printings
from sve_carddb.domains.registry.transitions.writer import (
    ArtDecisions,
    Decisions,
    Recipe,
    plan,
    write,
)
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.parse.pages import extract_jp
from sve_carddb.parse.pages.official_jp import card_url

if TYPE_CHECKING:
    from collections.abc import Generator

app = typer.Typer(
    no_args_is_help=True, help="Renew explicitly reviewed EN-only identities offline."
)


@contextmanager
def registry_lock(repo: Path) -> Generator[None]:
    """Use the allocator's existing global lock for the complete read/plan/write."""
    cache = repo / "carddb" / ".cache"
    cache.mkdir(parents=True, exist_ok=True)
    with (cache / "identity-registry.lock").open("w", encoding="utf-8") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield


def _recipe(path: Path) -> Recipe:
    try:
        return Recipe.model_validate_json(path.read_bytes())
    except ValueError:
        raise ValueError("Invalid registry renewal recipe") from None


@app.command("review-jp-input")
def jp_input_command(
    inputs: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    output: Annotated[Path, typer.Option(dir_okay=False)],
) -> None:
    """Export all currently registered JP cards from frozen sources, without adopting a review."""
    recipe = _recipe(inputs)
    if not output.is_absolute() or any(
        output.resolve().is_relative_to(root.resolve())
        for root in (recipe.repo, recipe.archive)
    ):
        raise ValueError("Review JP output must be absolute and outside the repository")
    with registry_lock(recipe.repo):
        files = read_base_files(recipe.repo / "authored")
        selected = next(source for source in recipe.sources if source.region == "jp")
        sources = FrozenSources(recipe.archive, recipe.store_id, selected.card_batch)
        versions = {
            item.url: item.source_version_id for item in sources.current_versions
        }
        cards = []
        for number in sorted(p.card_no for p in printings(files) if p.region == "jp"):
            version = versions.get(card_url(number))
            if version is None:
                raise ValueError(
                    "Registered JP printing is missing from frozen source batch"
                )
            page = physical_sources.page(
                sources,
                version,
                region="jp",
                parser_version=selected.parser_version,
            )
            record = page.record.get()
            if not isinstance(record, extract_jp.CardRecord):
                raise TypeError("Expected frozen JP card record")
            cards.append(jp_card(record))
        raw = b"".join(
            canonical(card.model_dump(mode="json")) + b"\n" for card in cards
        )
        output.parent.mkdir(parents=True, exist_ok=True)
        with output.open("xb") as stream:
            stream.write(raw)
    typer.echo(canonical({"jp_cards": len(cards), "bytes": len(raw)}).decode())


def _run(
    inputs: Path, decisions: Path, install: bool, check: bool, *, art: bool
) -> None:
    if install and check:
        raise ValueError("Choose --write or --check, not both")
    recipe = _recipe(inputs)
    try:
        model = ArtDecisions if art else Decisions
        reviewed = model.model_validate_json(decisions.read_bytes())
    except ValueError:
        raise ValueError("Invalid explicit registry renewal decisions") from None
    with registry_lock(recipe.repo):
        planned = plan(recipe, reviewed)
        if check and planned.shard is not None:
            raise ValueError("Registry reviews need a new complete renewal")
        if install:
            write(recipe.repo / "authored", planned)
    typer.echo(
        canonical(
            {
                "reviewed_records": planned.reviewed_records,
                "transition": None if planned.shard is None else planned.shard.path,
                "written": install and planned.shard is not None,
            }
        ).decode()
    )


@app.command("renew-reviews")
def renew_command(
    inputs: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    decisions: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    install: Annotated[
        bool, typer.Option("--write", help="Append the verified immutable transition.")
    ] = False,
    check: Annotated[
        bool,
        typer.Option(help="Require that all selected reviews are already current."),
    ] = False,
) -> None:
    """Plan a complete explicit review renewal; no writes unless --write is supplied."""
    _run(inputs, decisions, install, check, art=False)


@app.command("extend-art")
def extend_art_command(
    inputs: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    decisions: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    install: Annotated[
        bool, typer.Option("--write", help="Append the verified immutable transition.")
    ] = False,
    check: Annotated[
        bool,
        typer.Option(help="Require that all selected reviews are already current."),
    ] = False,
) -> None:
    """Append explicitly reviewed uses to existing art; never infer same-image membership."""
    _run(inputs, decisions, install, check, art=True)
