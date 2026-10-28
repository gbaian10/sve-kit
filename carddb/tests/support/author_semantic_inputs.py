"""Read synthetic internal semantic inventories for source-admission tests."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import digest
from sve_carddb.domains.effects.author_set import (
    AuthorInputSet,
    InputFile,
    merge_index_files,
)

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.effects.author_expansion import AuthorExpander


def load_semantic_inputs(root: Path, expander: AuthorExpander) -> AuthorInputSet:
    """Keep semantic fixture envelopes separate from persisted author storage."""

    def read(path: Path) -> InputFile:
        raw = path.read_bytes()
        return InputFile(path.relative_to(root).as_posix(), raw, digest(raw))

    indexes = tuple(
        read(path) for path in sorted((root / "rules/effects-index").glob("*.yaml"))
    )

    def files(area: str) -> tuple[InputFile, ...]:
        return tuple(read(path) for path in sorted((root / area).rglob("*.yaml")))

    return AuthorInputSet(
        merge_index_files(indexes, expander.schemas),
        indexes,
        files("rules/author-effects"),
        files("rules/effect-bodies"),
        files("rules/effects"),
    )
