"""Opt-in names composition; an absent link entry means no human links, not an empty file."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.digital_links.importer import Inputs as LinkInputs
from sve_carddb.digital_links.importer import populate_links
from sve_carddb.digital_name_policies.application import Inputs as NameInputs
from sve_carddb.digital_name_policies.application import populate, prepare
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.translations.digital import configuration as digital_configuration
from sve_carddb.translations.importer import _refs
from sve_carddb.translations.sources import Sources

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import BuildContext, SourceUse
    from sve_carddb.digital_links.importer import Result as LinkResult
    from sve_carddb.digital_name_policies.application import Result as NameResult
    from sve_carddb.snapshot.offline import Inputs
    from sve_carddb.text_observations.plan import TextPlan
    from sve_carddb.translations.current_names import Names


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
        registry = read_yaml(recipe.repo / "authored/ids/index.yaml")
        config = self.inputs.configuration() | {
            "catalog_registry": {
                "authored_revision": recipe.revision,
                "index_path": "authored/ids/index.yaml",
                "index_hash": digest(canonical(registry)),
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

    def dependencies(self) -> dict[str, bytes]:
        """Include exact policy bytes, not only selected eligibility hashes."""
        return {"authored/" + name: raw for name, raw in self.inputs.load().files}

    def populate_links(
        self, db: Database, *, context: BuildContext, stores: dict[str, Path]
    ) -> LinkResult | None:
        """Import existing human data into the private DB only when its entry exists."""
        return (
            None
            if self.links is None
            else populate_links(db, self.links, build=context, stores=stores)
        )

    def expected(
        self,
        db: Database,
        texts: TextPlan,
        *,
        context: BuildContext,
        stores: dict[str, Path],
        replay: Names,
        links: LinkResult | None,
    ) -> tuple[SourceUse, ...]:
        """Replay expectations independently before the writing stage."""
        sources = Sources(stores, self.inputs.repository, context)
        return prepare(
            db, self.inputs, texts, sources=sources, replay=replay, links=links
        ).uses

    def populate(
        self,
        db: Database,
        texts: TextPlan,
        *,
        context: BuildContext,
        stores: dict[str, Path],
        replay: Names,
        links: LinkResult | None,
    ) -> NameResult:
        """Compose owner-local names without enabling public digital browse data."""
        sources = Sources(stores, self.inputs.repository, context)
        return populate(
            db, self.inputs, texts, sources=sources, replay=replay, links=links
        )


def composer(recipe: Inputs) -> Composer | None:
    """Human links are optional; the link loader validates an existing entry."""
    if recipe.name_policy is None:
        return None
    entry = recipe.repo / "authored/digital-links"
    links = (
        LinkInputs(recipe.repo / "authored", recipe.repo, recipe.revision)
        # A dangling symlink still reaches the loader, which rejects symlinks.
        if entry.exists() or entry.is_symlink()
        else None
    )
    return Composer(
        NameInputs(
            recipe.repo / "authored", recipe.repo, recipe.revision, recipe.published_at
        ),
        links,
    )
