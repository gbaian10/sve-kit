"""Persist generated anchors for handwritten current-reader test fixtures."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_anchors import materialize_inputs
from sve_carddb.domains.effects.author_set import load_author_set

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.effects.author_current import CurrentAuthorSources
    from sve_carddb.domains.effects.author_expansion import AuthorExpander


def write_current_anchors(
    authored: Path,
    current: CurrentAuthorSources,
    expander: AuthorExpander,
    number: str,
) -> None:
    """Refresh explicitly before testing ordinary read-only source admission."""
    result = materialize_inputs(
        load_author_set(authored, expander.schemas),
        current,
        expander,
        refresh=frozenset({number}),
    )
    assert result.failures == {}
    for owner, anchor in result.anchors.items():
        path = authored / f"rules/effect-anchors/{owner}.yaml"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(canonical(anchor))
