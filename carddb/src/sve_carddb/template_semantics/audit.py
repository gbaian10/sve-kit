"""Single-host F1 provenance and independent source plans for historical groups."""

from pathlib import Path
from tempfile import TemporaryDirectory
from typing import TYPE_CHECKING
from urllib.parse import quote

from sve_carddb.build_inputs import BuildContext, SourceUse, uses_sorted
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.registry.records import PrintingData
from sve_carddb.snapshot.values import array, canonical, object_value
from sve_carddb.template_parameter_rules.replay import _glossary_files
from sve_carddb.template_semantics.registry import ROOT, installed, regular, verify
from sve_carddb.template_semantics.v1.urls import canonicalize
from sve_carddb.template_translations.replay_models import FlavorReplayInputs
from sve_carddb.translations.loader import load_glossary
from sve_carddb.translations.models import TermRecord
from sve_carddb.translations.name_replay import IdentityEvidence
from sve_carddb.translations.sources import Sources

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build_inputs import Source
    from sve_carddb.catalog.adoption_sources import PinnedRepository
    from sve_carddb.template_sources.models import Recipe
    from sve_carddb.template_translations.replay_models import ReplayContext


class ExpectedPlan:
    def __init__(self, repository: PinnedRepository, stores: dict[str, Path]) -> None:
        self.repository = repository
        self.stores = stores
        self.batches: dict[tuple[str, str], FrozenSources] = {}

    def batch(self, store: str, batch: str) -> FrozenSources:
        """Resolve only explicit sealed inputs, independently of the actual use collector."""
        key = store, batch
        if key not in self.batches:
            self.batches[key] = FrozenSources(self.stores[store], store, batch)
        return self.batches[key]

    def source(self, store: str, batch: str, version: str, parser: str) -> Source:
        """Read metadata from the sealed plan, never from collected actual uses."""
        return self.batch(store, batch).read(version, parser_version=parser)[0]

    def expected(  # ruff: ignore[too-many-locals,complex-structure,too-many-branches,too-many-statements] -- separate input declarations establish the F1 plan before execution
        self, pins: tuple[Recipe, ...], context: ReplayContext, main_revision: str
    ) -> tuple[SourceUse, ...]:
        """Declare raw, field, glossary and identity inputs before any replay runs."""
        from sve_carddb.template_translations.semantic_replay import evidence_context  # ruff: ignore[import-outside-top-level] -- orchestration is separate from fixed computations

        flavor = isinstance(context.inputs, FlavorReplayInputs)
        checked = verify(
            self.repository,
            context.semantic_bindings,
            flavor=flavor,
            environment=context.environment,
        )
        by_id = {p.id: p for p in pins}
        if flavor:
            assert isinstance(context.inputs, FlavorReplayInputs)
            batch = context.inputs.source_batch.model_dump()
        else:
            batch = object_value(
                by_id["template-parameters-jp-candidate-v1"].config["source_batch"]
            )
        frozen = self.batch(str(batch["store_id"]), str(batch["batch_id"]))
        planned = []
        current = {c.source_version_id for c in frozen.inventory.current}
        for version in sorted(frozen.entries):
            source, raw, _descriptor = frozen.read(
                version, parser_version="translation-jp-v1"
            )
            planned.append(
                SourceUse(source=source, usage="template_frozen_batch", locator="/")
            )
            if version not in current:
                continue
            _lang, document = checked.parser.project(raw, source.url, "jp")
            for i, face in enumerate(array(object_value(document)["faces"])):
                locators = (
                    [f"/faces/{i}/flavor"]
                    if flavor
                    else [
                        f"/faces/{i}/text",
                        *(
                            f"/faces/{i}/sections/{j}"
                            for j in range(len(array(object_value(face)["sections"])))
                        ),
                    ]
                )
                planned.extend(
                    SourceUse(source=source, usage="template_field", locator=locator)
                    for locator in locators
                )
        if flavor:
            assert isinstance(context.inputs, FlavorReplayInputs)
            basis = context.inputs.identity_basis
            if basis is not None:
                evidence = IdentityEvidence(
                    Sources(
                        self.stores,
                        self.repository.root,
                        evidence_context(
                            self.repository, pins[0].code_revision, checked
                        ),
                        semantics=checked,
                    ),
                    main_revision,
                )
                registry = evidence.registry(basis)
                by_url: dict[str, list[tuple[Source, bytes]]] = {}
                for batch_pin in context.inputs.identity_batches:
                    archive = self.batch(batch_pin.store_id, batch_pin.batch_id)
                    for version in sorted(archive.entries):
                        descriptor = archive.descriptor(version)
                        parser = "translation-" + descriptor.provider + "-v1"
                        source, raw, _ = archive.read(version, parser_version=parser)
                        by_url.setdefault(source.url, []).append((source, raw))
                for record in registry.records.values():
                    if not isinstance(record.data, PrintingData):
                        continue
                    printing = record.data
                    provider = (
                        "https://shadowverse-evolve.com/cardlist/?cardno="
                        if printing.region == "jp"
                        else "https://en.shadowverse-evolve.com/cards/?cardno="
                    )
                    url = canonicalize(provider + quote(printing.card_no, safe=""))
                    found = []
                    for source, raw in by_url.get(url, ()):
                        card = checked.parser.card(
                            source, raw, printing.region, printing.card_no
                        )
                        if card.observation == printing.observation:
                            found.append(
                                SourceUse(
                                    source=source,
                                    usage="name_identity_observation",
                                    locator=canonical(
                                        [basis.authored_revision, printing.id]
                                    ).decode(),
                                )
                            )
                    if not found:
                        raise ValueError(
                            "Replay expected identity observation closure is absent"
                        )
                    planned.extend(found)

        else:
            config = by_id["template-parameters-jp-candidate-v1"].config
            pin = object_value(object_value(config["references"])["glossary"])
            expected: dict[str, JsonValue] = {
                "authored/translations/index.yaml": pin["index_hash"]
            }
            for value in array(pin["shards"]):
                shard = object_value(value)
                expected["authored/" + str(shard["path"])] = shard["exact_hash"]
            content = _glossary_files(
                self.repository, str(pin["authored_revision"]), expected
            )
            with TemporaryDirectory(prefix="replay-plan-glossary-") as folder:
                for name, raw in content.items():
                    path = Path(folder) / name.removeprefix("authored/")
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(raw)
                glossary = load_glossary(Path(folder))
                for term_record, _ in glossary.effective():
                    if (
                        isinstance(term_record, TermRecord)
                        and term_record.data.source_ref is not None
                    ):
                        ref = term_record.data.source_ref
                        planned.append(
                            SourceUse(
                                source=self.source(
                                    ref.store_id,
                                    ref.batch_id,
                                    ref.source_version_id,
                                    ref.parser,
                                ),
                                usage="translation_evidence",
                                locator=ref.locator,
                            )
                        )
        return uses_sorted(planned)


def host_context(
    repository: PinnedRepository, host: str, configuration: JsonValue
) -> BuildContext:
    """F1 H is the executing complete host, not any inventory's producer R."""
    names = {"carddb/uv.lock", "carddb/pyproject.toml"}
    for path in (ROOT / "carddb/src/sve_carddb").rglob("*"):
        if path.suffix in {".py", ".json"} and path.name != "_version.py":
            names.add(path.relative_to(ROOT).as_posix())
    content = regular(repository, host, tuple(sorted(names)))
    if any(installed(name) != raw for name, raw in content.items()):
        raise ValueError(
            "Replay host revision differs from actual installed program bytes"
        )
    return BuildContext.from_inputs(host, content, configuration)
