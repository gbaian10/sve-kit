"""Opt-in names composition; an absent link entry means no human links, not an empty file."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.authored import authored_root
from sve_carddb.core.json import canonical, object_value, parse
from sve_carddb.digital_links.importer import Inputs as LinkInputs
from sve_carddb.digital_links.importer import populate_links
from sve_carddb.digital_name_policies.application import Inputs as NameInputs
from sve_carddb.digital_name_policies.application import populate
from sve_carddb.translations.digital import configuration as digital_configuration
from sve_carddb.translations.importer import _refs

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database
    from sve_carddb.core.provenance import BuildContext
    from sve_carddb.digital_links.importer import Result as LinkResult
    from sve_carddb.digital_name_policies.application import Result as NameResult
    from sve_carddb.text_observations.plan import TextPlan
    from sve_carddb.translations.current_names import Names
    from sve_carddb.translations.sources import Sources
    from sve_carddb.workflows.offline import Inputs


@dataclass(frozen=True)
class Composer:
    inputs: NameInputs
    links: LinkInputs | None

    def configuration(self, recipe: Inputs) -> dict[str, JsonValue]:
        """Declare current registry and frozen source scopes separately from policy pins."""
        links = self.inputs.load().links
        batches = (
            []
            if links is None
            else [pin.model_dump(mode="json") for pin in links.content.source_batches]
        )
        batches.extend({"batch_id": pin.card_batch} for pin in recipe.sources)
        config = self.inputs.configuration() | {
            "catalog_registry": {
                "authored_revision": recipe.revision,
                "index_path": "authored/ids/index.yaml",
            },
            "digital_link_sources": [
                object_value(parse(value))
                for value in sorted({canonical(b) for b in batches})
            ],
            "digital_name_link_entry": {
                "authored_revision": recipe.revision,
                "present": self.links is not None,
            },
        }
        if self.links is not None:
            snapshot = self.links.load()
            refs = tuple(
                sorted(
                    {
                        r
                        for record in snapshot.records()
                        for r in _refs(record.model_dump(mode="json"))
                        if r.parser in {"translation-sv1-v1", "translation-svwb-v1"}
                    },
                    key=lambda r: canonical(r.model_dump(mode="json")),
                )
            )
            targets = tuple(
                sorted(
                    {
                        (r.subject.game, r.subject.official_id)
                        for r in snapshot.records()
                    }
                )
            )
            config |= self.links.configuration() | digital_configuration(refs, targets)
        return config

    def populate_links(
        self,
        db: Database,
        *,
        context: BuildContext,
        stores: dict[str, Path],
        sources: Sources,
    ) -> LinkResult | None:
        """Import existing human data into the private DB only when its entry exists."""
        return (
            None
            if self.links is None
            else populate_links(
                db, self.links, build=context, stores=stores, sources=sources
            )
        )

    def populate(
        self,
        db: Database,
        texts: TextPlan,
        *,
        replay: Names,
        links: LinkResult | None,
        sources: Sources,
    ) -> NameResult:
        """Compose owner-local names without enabling public digital browse data."""
        return populate(
            db, self.inputs, texts, sources=sources, replay=replay, links=links
        )


def composer(recipe: Inputs) -> Composer | None:
    """Human links are optional; the link loader validates an existing entry."""
    if recipe.name_policy is None:
        return None
    entry = authored_root(recipe.repo) / "digital-links"
    links = (
        LinkInputs(authored_root(recipe.repo), recipe.repo, recipe.revision)
        # A dangling symlink still reaches the loader, which rejects symlinks.
        if entry.exists() or entry.is_symlink()
        else None
    )
    return Composer(
        NameInputs(
            authored_root(recipe.repo),
            recipe.repo,
            recipe.revision,
            recipe.published_at,
        ),
        links,
    )
