"""Opt-in names composition; immutable entry absence is evidence, never a fake empty link set."""

import subprocess  # ruff: ignore[suspicious-subprocess-import] -- fixed immutable Git paths, no shell or network
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.catalog.adoption_sources import PinnedRepository
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
    from sve_carddb.translations.name_replay import NameReplay


@dataclass(frozen=True)
class Composer:
    inputs: NameInputs
    links: LinkInputs | None

    def configuration(self, recipe: Inputs) -> dict[str, JsonValue]:
        """Declare current registry and frozen source scopes separately from policy pins."""
        loaded = self.inputs.load().effective("names")
        batches = [
            pin.model_dump(mode="json") for pin in loaded.catalogue().source_batches
        ]
        batches.extend(
            {"store_id": recipe.store_id, "batch_id": pin.card_batch}
            for pin in recipe.sources
        )
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
                        for record, _ in snapshot.records()
                        for r in _refs(record.model_dump(mode="json"))
                        if r.parser in {"translation-sv1-v1", "translation-svwb-v1"}
                    },
                    key=lambda r: canonical(r.model_dump(mode="json")),
                )
            )
            targets = tuple(
                sorted(
                    {
                        (r.data.subject.game, r.data.subject.official_id)
                        for r, _ in snapshot.effective()
                        if r.data.value is not None
                    }
                )
            )
            config |= self.links.configuration() | digital_configuration(refs, targets)
        return config

    def dependencies(self) -> dict[str, bytes]:
        """Include exact policy/receipt/list bytes, not only selected eligibility hashes."""
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
        replay: NameReplay,
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
        replay: NameReplay,
        links: LinkResult | None,
    ) -> NameResult:
        """Compose owner-local names without enabling public digital browse data."""
        sources = Sources(stores, self.inputs.repository, context)
        return populate(
            db, self.inputs, texts, sources=sources, replay=replay, links=links
        )


def composer(recipe: Inputs) -> Composer | None:
    """Absence must agree on disk and in the declared immutable authored tree."""
    if recipe.name_policy is None:
        return None
    repository = PinnedRepository(recipe.repo)
    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- full Git SHA and a fixed entry path, no shell
        [
            repository.executable,
            "-C",
            str(recipe.repo),
            "ls-tree",
            "-r",
            "--name-only",
            recipe.revision,
            "--",
            "authored/digital-links",
        ],
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise ValueError("Name composition digital-link entry tree is unavailable")
    present = bool(result.stdout)
    path = recipe.repo / "authored/digital-links"
    if path.is_symlink() or present != path.exists():
        raise ValueError(
            "Name composition digital-link entry differs from immutable tree"
        )
    links = (
        LinkInputs(recipe.repo / "authored", recipe.repo, recipe.revision)
        if present
        else None
    )
    return Composer(
        NameInputs(
            recipe.repo / "authored", recipe.repo, recipe.revision, recipe.published_at
        ),
        links,
    )
