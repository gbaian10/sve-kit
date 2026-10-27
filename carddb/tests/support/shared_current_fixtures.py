"""Build real-current databases once and map their immutable bytes across workers."""

import mmap
from contextlib import ExitStack, contextmanager
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.domains.parsed_cards.sources import ParsedBatches
from sve_carddb.domains.translations.inputs import Snapshot
from sve_carddb.domains.translations.sources import Sources

from .author_current_fixtures import (
    REPOSITORY,
    CurrentSourceFixture,
    CurrentSourceInputs,
)
from .database_template import DatabaseTemplate
from .prepared_values import freeze, restore, shared_preparation

if TYPE_CHECKING:
    from collections.abc import Callable, Generator
    from pathlib import Path

    from sve_carddb.build import CompiledSchema
    from sve_carddb.core.provenance import BuildContext


@dataclass(frozen=True)
class PreparedCurrent:
    schema: CompiledSchema
    snapshot: Snapshot
    build: BuildContext
    stores: dict[str, Path]


@dataclass
class CurrentFixtureCache:
    root: Path
    inputs: CurrentSourceInputs
    mappings: ExitStack

    def get(
        self, name: str, build: Callable[[Path], CurrentSourceFixture]
    ) -> CurrentSourceFixture:
        cache = self.root / name
        cache.mkdir(exist_ok=True)
        database = cache / "template.sqlite"

        def prepare() -> bytes:
            fixture = build(cache / "sources")
            assert isinstance(fixture.template.content, bytes)
            database.write_bytes(fixture.template.content)
            return freeze(
                PreparedCurrent(
                    fixture.template.schema,
                    Snapshot(fixture.snapshot.shards, fixture.snapshot.closure),
                    fixture.sources.build,
                    fixture.sources.stores,
                )
            )

        prepared = restore(
            shared_preparation(cache / "prepared.pickle", prepare), PreparedCurrent
        )
        # Only the small metadata graph is restored; the catalog stays worker-local.
        with database.open("rb") as stream:
            content = self.mappings.enter_context(
                mmap.mmap(stream.fileno(), 0, access=mmap.ACCESS_READ)
            )
        return CurrentSourceFixture(
            DatabaseTemplate(prepared.schema, content),
            Sources(
                prepared.stores,
                REPOSITORY,
                prepared.build,
                self.inputs.identity.snapshot,
                batches=ParsedBatches(self.inputs.catalog),
            ),
            prepared.snapshot,
            self.inputs.vocabulary,
            self.inputs.rules,
            self.inputs.batch_id,
        )


@contextmanager
def current_fixture_cache(
    root: Path, inputs: CurrentSourceInputs
) -> Generator[CurrentFixtureCache]:
    with ExitStack() as mappings:
        yield CurrentFixtureCache(root, inputs, mappings)
