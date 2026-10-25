"""Compose independent name-policy and adopted-link capabilities in one transaction."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.authored import authored_root
from sve_carddb.core.json import canonical, object_value, parse
from sve_carddb.domains.digital.links.importer import Inputs as LinkInputs
from sve_carddb.domains.digital.links.importer import populate_links
from sve_carddb.domains.digital.name_policies.application import Inputs as NameInputs
from sve_carddb.domains.digital.name_policies.application import populate
from sve_carddb.domains.translations.digital import (
    configuration as digital_configuration,
)

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database
    from sve_carddb.core.provenance import BuildContext
    from sve_carddb.domains.catalog.adoption_models import SourceRef
    from sve_carddb.domains.digital.links.importer import Result as LinkResult
    from sve_carddb.domains.digital.name_policies.results import Result as NameResult
    from sve_carddb.domains.text_observations.plan import TextPlan
    from sve_carddb.domains.translations.names.resolve import Names
    from sve_carddb.domains.translations.sources import Sources
    from sve_carddb.workflows.offline import Inputs


@dataclass(frozen=True)
class Composer:
    inputs: NameInputs | None
    links: LinkInputs | None

    def configuration(self, recipe: Inputs) -> dict[str, JsonValue]:
        """Declare current registry and frozen source scopes separately from policy pins."""
        links = None if self.inputs is None else self.inputs.load().links
        batches = (
            []
            if links is None
            else [pin.model_dump(mode="json") for pin in links.content.source_batches]
        )
        batches.extend({"batch_id": pin.card_batch} for pin in recipe.sources)
        config = ({} if self.inputs is None else self.inputs.configuration()) | {
            "catalog_registry": {
                "authored_revision": recipe.revision,
                "index_path": "authored/ids/index.yaml",
            },
            "digital_name_link_entry": {
                "authored_revision": recipe.revision,
                "present": self.links is not None,
            },
        }
        if self.links is not None:
            snapshot = self.links.load()
            references = sorted(
                {
                    name.name_ref
                    for record in snapshot.records()
                    for name in record.value.digital_names
                },
                key=lambda r: canonical(r.model_dump(mode="json")),
            )
            # A SourceRef selects a whole API page for the current inventory.
            pages: dict[tuple[str, str, str], SourceRef] = {}
            for ref in references:
                pages.setdefault((ref.batch_id, ref.source_version_id, ref.parser), ref)
            refs = tuple(pages.values())
            batches.extend(
                {"batch_id": ref.batch_id}
                for record in snapshot.records()
                for ref in (
                    *(n.name_ref for n in record.value.sve_names),
                    *(n.name_ref for n in record.value.digital_names),
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
        config["digital_link_sources"] = [
            object_value(parse(value))
            for value in sorted({canonical(b) for b in batches})
        ]
        return config

    def populate_links(
        self,
        db: Database,
        *,
        context: BuildContext,
        stores: dict[str, Path],
        sources: Sources,
    ) -> LinkResult | None:
        """Import adopted browse relations independently of name-policy selection."""
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
        """Compose owner-local names only with the explicitly enabled policy."""
        if self.inputs is None:
            raise ValueError("Name policy capability is not enabled")
        return populate(
            db, self.inputs, texts, sources=sources, replay=replay, links=links
        )


def composer(recipe: Inputs) -> Composer | None:
    """An existing link entry enables browse intake even without a name policy."""
    entry = authored_root(recipe.repo) / "digital/links"
    links = (
        LinkInputs(authored_root(recipe.repo), recipe.repo, recipe.revision)
        # A dangling symlink still reaches the loader, which rejects symlinks.
        if entry.exists() or entry.is_symlink()
        else None
    )
    if recipe.name_policy is None and links is None:
        return None
    return Composer(
        None
        if recipe.name_policy is None
        else NameInputs(
            authored_root(recipe.repo),
            recipe.repo,
            recipe.revision,
            recipe.published_at,
        ),
        links,
    )
