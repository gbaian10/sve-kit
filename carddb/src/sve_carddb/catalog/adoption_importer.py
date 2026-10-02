"""Atomic authored adoption projection; the caller-only staging API stays fail closed."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_db import Json
from sve_carddb.build_db.rows import insert_exact
from sve_carddb.build_inputs import input_record, insert_raw_sources
from sve_carddb.catalog import adoption_validation as validate
from sve_carddb.catalog.adoption_loader import AdoptionSnapshot, load_adoptions
from sve_carddb.catalog.adoption_models import (
    AliasRecord,
    DefaultRecord,
    LanguageRecord,
    NameRecord,
    RouteRecord,
    SourceText,
    SymbolRecord,
    TextEvidence,
    VocabularyRecord,
)
from sve_carddb.catalog.adoption_sources import AdoptionSources, PinnedRepository
from sve_carddb.catalog.languages import register_languages
from sve_carddb.catalog.projection import CatalogProjection, project_catalog
from sve_carddb.catalog.rules_names import populate_rules_names, register_name
from sve_carddb.extract import official_en, official_jp
from sve_carddb.extract.compare_jp import legacy_projection
from sve_carddb.registry.preview import plan_preview
from sve_carddb.registry.preview.evidence import (
    CardEvidence,
    FaceEvidence,
    MemoryEvidence,
)
from sve_carddb.registry.preview.importer import populate_identity_rows
from sve_carddb.registry.records import Observation, PrintingData
from sve_carddb.registry.review import observation
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.routes import populate_routes
from sve_carddb.routes.defaults import select_defaults
from sve_carddb.routes.rarity_policy import APPROVED_GENERAL_RARITIES
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.text_observations.importer import populate_text_observations
from sve_carddb.text_observations.intern import TextInterner
from sve_carddb.text_observations.plan import verify_plan
from sve_carddb.text_observations.type_binding import type_spelling
from sve_carddb.translations.importer import Inputs as TranslationInputs
from sve_carddb.translations.importer import populate_glossary

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database, Value
    from sve_carddb.build_inputs import BuildContext, InputRecord
    from sve_carddb.catalog.adoption_loader import Entry
    from sve_carddb.catalog.adoption_models import Record, ReviewContext
    from sve_carddb.products.models import LocalizedText
    from sve_carddb.routes.defaults import DefaultPrinting
    from sve_carddb.text_observations.plan import TextPlan


_MIN_VARIANTS = 2


@dataclass(frozen=True)
class AdoptionInputs:
    root: Path
    repository: Path
    authored_revision: str
    entries: tuple[Entry, ...]
    include_translations: bool = False

    def translation_inputs(self) -> TranslationInputs | None:
        """Enable the complete existing entry, never a filtered glossary subset."""
        if type(self.include_translations) is not bool:
            raise ValueError("Translation composition flag must be boolean")
        if not self.include_translations:
            return None
        return TranslationInputs(self.root, self.repository, self.authored_revision)

    def load(self) -> tuple[AdoptionSnapshot, ...]:
        """Reload immutable bytes instead of accepting a caller-made approval object."""
        if not re.fullmatch(r"[0-9a-f]{40}", self.authored_revision):
            raise ValueError("Adoption authored revision must be a full Git SHA")
        if not self.entries or self.entries != tuple(sorted(set(self.entries))):
            raise ValueError(
                "Adoption entries must be explicitly enabled, sorted and unique"
            )
        snapshots = tuple(
            load_adoptions(self.root, entry=entry) for entry in self.entries
        )
        repository = PinnedRepository(self.repository)
        for snapshot in snapshots:
            files = [(snapshot.entry + "/index.yaml", snapshot.index_exact)]
            files.extend((s.path, s.exact) for s in snapshot.shards)
            for name, content in files:
                if (
                    repository.read(self.authored_revision, "authored/" + name)
                    != content
                ):
                    raise ValueError(
                        "Adoption bytes differ from immutable authored revision"
                    )
        return snapshots

    def configuration(self) -> dict[str, JsonValue]:
        """Pin both enabled entries, complete byte inventories, and the authored commit."""
        snapshots = self.load()
        translation = self.translation_inputs()
        return {
            **({} if translation is None else translation.configuration()),
            "catalog_adoptions": {
                "include_translations": self.include_translations,
                "authored_revision": self.authored_revision,
                "inputs": [s.pins() for s in snapshots],
                "effective": [
                    {
                        "record_key": r.record_key,
                        "record_hash": digest(canonical(r.model_dump(mode="json"))),
                        "decision_id": decision,
                        "dependencies": [
                            d.model_dump(mode="json") for d in r.data.dependencies
                        ],
                    }
                    for snapshot in snapshots
                    for r, decision in snapshot.effective()
                ],
            },
        }


@dataclass(frozen=True)
class PreparedAdoptions:
    snapshots: tuple[AdoptionSnapshot, ...]
    sources: AdoptionSources
    effective: tuple[tuple[Record, str], ...]
    reviews: dict[str, ReviewContext]


def _prepare_adoptions(  # ruff: ignore[complex-structure] -- validate complete immutable inputs, history and current frozen evidence before projection
    db: Database,
    inputs: AdoptionInputs,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    text_plan: TextPlan | None,
) -> PreparedAdoptions:
    """Share the full read-only verification boundary with projection and import."""
    snapshots = inputs.load()
    configuration = object_value(parse(build.configuration.encode()))
    if any(
        configuration.get(key) != value for key, value in inputs.configuration().items()
    ):
        raise ValueError("Build configuration does not pin adoption inputs")
    repository = PinnedRepository(inputs.repository)
    repository.context(build)
    current = AdoptionSources(stores, repository)
    _current_recipes(snapshots, current, build)
    sources = AdoptionSources(stores, repository, historical=True)
    sources.batches = current.batches
    sources.uses = current.uses
    if text_plan is not None:
        verify_plan(text_plan)
        _verify_identity(inputs, text_plan, current, build)
        if any(item.card.raw is None for item in text_plan.observations):
            raise ValueError("Adoption identity/text inputs require frozen raw bytes")
        for item in text_plan.observations:
            archive = item.card.source.archive
            source, raw, _ = sources.batch(archive.store_id, archive.batch_id).read(
                item.card.source.id, parser_version=item.card.source.parser_version
            )
            if source != item.card.source or raw != item.card.raw:
                raise ValueError("Adoption identity/text raw source closure mismatch")
            legacy = (
                legacy_projection(official_jp.extract_card(raw, number=item.card_no))
                if item.region == "jp"
                else official_en.legacy_projection(
                    official_en.extract_card(raw, number=item.card_no)
                )
            )
            actual = Observation.model_validate_json(
                canonical(observation(legacy, item.region))
            )
            if actual != item.card.observation:
                raise ValueError(
                    "Adoption identity observation cannot be reproduced from frozen raw"
                )

        if configuration.get("text_observations") != text_plan.configuration():
            raise ValueError(
                "Build configuration does not pin complete text/identity inputs"
            )
        sources.uses.extend(text_plan.source_uses())
        sources.uses.extend(text_plan.identity.source_uses())
        for use in {use.source for use in text_plan.source_uses()}:
            archive = use.archive
            source, _, _ = sources.batch(archive.store_id, archive.batch_id).read(
                use.id, parser_version=use.parser_version
            )
            if source != use:
                raise ValueError("Adoption text source-use frozen closure mismatch")
    _validate_histories(snapshots, sources)
    translation_path = inputs.root / "translations"
    if (
        translation_path.exists() or translation_path.is_symlink()
    ) and not inputs.include_translations:
        raise ValueError(
            "Translation entry must be explicitly enabled for catalog composition"
        )
    effective = tuple(pair for snapshot in snapshots for pair in snapshot.effective())
    reviews = {
        record.record_key: shard.envelope().review_context
        for snapshot in snapshots
        for shard in snapshot.shards
        for record in shard.envelope().records
    }
    _dependencies(effective, db, text_plan)
    _freshness(effective, reviews, sources, db, text_plan)
    return PreparedAdoptions(snapshots, sources, effective, reviews)


def _current_recipes(
    snapshots: tuple[AdoptionSnapshot, ...],
    sources: AdoptionSources,
    build: BuildContext,
) -> None:
    """Include reviewed image/identity recipes even when no text ref names the parser."""
    parsers = {
        evidence.source_ref.parser
        for snapshot in snapshots
        for record, _ in snapshot.records()
        for evidence in record.evidence
        if isinstance(evidence, TextEvidence)
    }
    for snapshot in snapshots:
        for shard in snapshot.shards:
            reviewed = object_value(
                parse(shard.envelope().review_context.context.configuration.encode())
            )
            parsers.update(object_value(reviewed.get("catalog_source_recipes", {})))
    for parser in sorted(parsers):
        sources.recipe(parser, build)


def derive_catalog(
    db: Database,
    inputs: AdoptionInputs,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    text_plan: TextPlan | None = None,
) -> CatalogProjection:
    """Rebuild memory objects from immutable effective receipts without writing rows."""
    prepared = _prepare_adoptions(
        db, inputs, build=build, stores=stores, text_plan=text_plan
    )
    return _derive_prepared(prepared, text_plan)


def _derive_prepared(
    prepared: PreparedAdoptions, text_plan: TextPlan | None
) -> CatalogProjection:
    projection = project_catalog(
        prepared.effective, prepared.reviews, prepared.sources, text_plan
    )
    if text_plan is not None:
        required = set()
        for item in (*text_plan.observations, *text_plan.candidates()):
            raw, _ = type_spelling(text_plan, item)
            required.add((item.region, "type", raw))
            if item.content.class_raw != "-":
                required.add((item.region, "class", item.content.class_raw))
        known = {
            (binding.region, binding.kind, binding.raw)
            for binding in projection.vocabulary.bindings
        }
        missing: list[JsonValue] = [
            {
                "region": region,
                "kind": kind,
                "raw": raw,
                "candidates": list[JsonValue](
                    sorted(
                        {
                            b.code
                            for b in projection.vocabulary.bindings
                            if b.kind == kind
                        }
                    )
                ),
            }
            for region, kind, raw in sorted(required - known)
        ]
        if missing:
            raise ValueError(
                "Unadopted vocabulary spellings: "
                + canonical(missing).decode()
                + "; new spellings require maintainer confirmation"
            )
    return projection


def populate_adoptions(
    db: Database,
    inputs: AdoptionInputs,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    text_plan: TextPlan | None = None,
) -> InputRecord:
    """Validate all histories and freshness before writing any catalog/display rows."""
    prepared = _prepare_adoptions(
        db, inputs, build=build, stores=stores, text_plan=text_plan
    )
    return _populate_prepared(
        db, inputs, prepared, build=build, stores=stores, text_plan=text_plan
    )


def _populate_prepared(
    db: Database,
    inputs: AdoptionInputs,
    prepared: PreparedAdoptions,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    text_plan: TextPlan | None,
) -> InputRecord:
    snapshots, sources = prepared.snapshots, prepared.sources
    effective, reviews = prepared.effective, prepared.reviews
    _audit(snapshots, db, inputs.authored_revision)
    insert_raw_sources(db, (use.source for use in sources.uses))
    _audit_evidence(snapshots, db, sources)
    registered = {
        r.data.subject.code
        for r, _ in effective
        if isinstance(r, LanguageRecord) and r.data.value is not None
    }
    languages = tuple(
        language_item
        for r, _ in effective
        if isinstance(r, LanguageRecord)
        if (language_item := validate.language(r, registered)) is not None
    )
    register_languages(db, languages)
    texts = TextInterner(db, published=())
    _retained_vocabulary(snapshots, effective, reviews, sources, db, texts)
    for record, decision in effective:
        _project(
            record, decision, reviews[record.record_key], sources, db, texts, text_plan
        )
    db.verify_alias_targets()
    translation = inputs.translation_inputs()
    translation_uses = (
        ()
        if translation is None
        else populate_glossary(db, translation, build=build, stores=stores).uses
    )
    uses = (*sources.uses, *translation_uses)
    result = input_record(build, uses)
    result.verify(db, build, uses, complete=False)
    return result


def _verify_identity(
    inputs: AdoptionInputs,
    plan: TextPlan,
    sources: AdoptionSources,
    build: BuildContext,
) -> None:
    """Rebuild eligibility and physical metadata, never borrow caller classification facts."""
    snapshot = load_registry(inputs.root)
    if snapshot.files != plan.identity.snapshot.files:
        raise ValueError("Adoption current registry differs from immutable plan")
    paths = ["ids/index.yaml", *(s.path for s in snapshot.files.shards)]
    for path in paths:
        if (
            sources.repository.read(inputs.authored_revision, "authored/" + path)
            != (inputs.root / path).read_bytes()
        ):
            raise ValueError(
                "Current registry bytes differ from immutable authored revision"
            )
    cards = {}
    sources.repository.context(build)
    for (region, number), evidence in plan.identity.evidence.items():
        parser = "official-jp-exact-v1" if region == "jp" else "official-en-exact-v1"
        pin = sources.recipe(parser, build)
        expected_path = f"carddb/src/sve_carddb/extract/official_{region}.py"
        if (
            pin.code_path != expected_path
            or pin.config
            or evidence.source.parser_version != parser
        ):
            raise ValueError(
                "Current identity source recipe does not match pinned parser"
            )
        archive = evidence.source.archive
        source, raw, _ = sources.batch(archive.store_id, archive.batch_id).read(
            evidence.source.id, parser_version=evidence.source.parser_version
        )
        if source != evidence.source:
            raise ValueError("Current identity source metadata mismatch")
        if region == "jp":
            parsed = official_jp.extract_card(raw, number=number)
            legacy = legacy_projection(parsed)
            faces = tuple(FaceEvidence(f.rarity, f.illustrator) for f in parsed.faces)
        else:
            parsed_en = official_en.extract_card(raw, number=number)
            legacy = official_en.legacy_projection(parsed_en)
            faces = tuple(
                FaceEvidence(f.info["Rarity"], f.illustrator) for f in parsed_en.faces
            )
        cards[region, number] = CardEvidence.from_card(source, region, legacy, faces)
    rebuilt = plan_preview(
        inputs.root, MemoryEvidence(cards), regions=plan.identity.regions
    )
    if rebuilt != plan.identity:
        raise ValueError(
            "Adoption identity eligibility/physical evidence cannot be reproduced"
        )


def _retained_vocabulary(
    snapshots: tuple[AdoptionSnapshot, ...],
    effective: tuple[tuple[Record, str], ...],
    reviews: dict[str, ReviewContext],
    sources: AdoptionSources,
    db: Database,
    texts: TextInterner,
) -> None:
    for record, _ in effective:
        if not isinstance(record, VocabularyRecord) or record.data.value is not None:
            continue
        previous = [
            r
            for snapshot in snapshots
            for r, _ in snapshot.records()
            if isinstance(r, VocabularyRecord)
            and r.data.subject == record.data.subject
            and r.data.value is not None
        ]
        original = max(previous, key=lambda r: r.data.adoption_no)
        term = validate.term(original, reviews[original.record_key], sources)
        assert term is not None
        insert_exact(
            db,
            "vocabulary",
            {
                "kind": term.kind,
                "code": term.code,
                "label_unit_id": texts.intern(term.label),
                "active": False,
            },
            ("kind", "code"),
        )


def import_adoptions(
    db: Database,
    inputs: AdoptionInputs,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    text_plan: TextPlan | None = None,
) -> InputRecord:
    """Own one transaction for catalog, display overrides and adoption provenance."""
    with db.transaction():
        return populate_adoptions(
            db, inputs, build=build, stores=stores, text_plan=text_plan
        )


def import_adopted_text(
    db: Database,
    inputs: AdoptionInputs,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    text_plan: TextPlan,
    published: tuple[LocalizedText, ...] = (),
) -> InputRecord:
    """Re-derive bindings and atomically import catalog, glossary and exact observations."""
    with db.transaction():
        identity = populate_identity_rows(
            db,
            text_plan.publication_identity(),
            authored_revision=inputs.authored_revision,
            build=build,
        )
        prepared = _prepare_adoptions(
            db, inputs, build=build, stores=stores, text_plan=text_plan
        )
        projection = _derive_prepared(prepared, text_plan)
        adoption = _populate_prepared(
            db, inputs, prepared, build=build, stores=stores, text_plan=text_plan
        )
        observations = populate_text_observations(
            db,
            text_plan,
            build=build,
            vocabulary=projection.vocabulary,
            published=published,
        )
        uses = (*identity.uses, *adoption.uses, *observations.uses)
        result = input_record(build, uses)
        result.verify(db, build, uses, complete=False)
        return result


def import_adoption_build(
    db: Database,
    inputs: AdoptionInputs,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    text_plan: TextPlan,
) -> tuple[InputRecord, tuple[DefaultPrinting, ...]]:
    """Compose checked identities, overrides, routes and defaults atomically."""
    configuration = object_value(parse(build.configuration.encode()))
    if any(
        configuration.get(key) != value
        for key, value in APPROVED_GENERAL_RARITIES.configuration().items()
    ):
        raise ValueError("Build configuration does not pin approved rarity policy")
    with db.transaction():
        identity = populate_identity_rows(
            db,
            text_plan.publication_identity(),
            authored_revision=inputs.authored_revision,
            build=build,
        )
        adoption = populate_adoptions(
            db, inputs, build=build, stores=stores, text_plan=text_plan
        )
        populate_routes(db)
        defaults = select_defaults(db, rarity_whitelist=APPROVED_GENERAL_RARITIES)
        uses = (*identity.uses, *adoption.uses)
        result = input_record(build, uses)
        result.verify(db, build, uses, complete=False)
        return result, defaults


def _validate_histories(  # ruff: ignore[complex-structure] -- dispatch every historical kind before any effective projection
    snapshots: tuple[AdoptionSnapshot, ...], sources: AdoptionSources
) -> None:
    registered = {
        r.data.subject.code
        for snapshot in snapshots
        for r, _ in snapshot.records()
        if isinstance(r, LanguageRecord) and r.data.value is not None
    }
    for snapshot in snapshots:
        for shard in snapshot.shards:
            envelope = shard.envelope()
            for record in envelope.records:
                sources.verify(record, envelope.review_context)
                review = envelope.review_context
                validate.image_faces(record, review, sources)
                if isinstance(record, VocabularyRecord):
                    term = validate.term(record, review, sources)
                    lang = (
                        term.label.lang
                        if term is not None
                        and record.data.value is not None
                        and isinstance(record.data.value.label, SourceText)
                        else None
                    )
                    validate.verify_dependencies(record, source_lang=lang)
                if not isinstance(record, VocabularyRecord):
                    validate.verify_dependencies(record)
                if isinstance(record, AliasRecord):
                    validate.alias(record, review, sources)
                elif isinstance(record, LanguageRecord):
                    validate.language(record, registered)
                elif isinstance(record, NameRecord):
                    validate.reviewed_names(record, review, sources)
                elif isinstance(record, RouteRecord):
                    _reviewed_route(record, review, sources)
                elif isinstance(record, DefaultRecord):
                    _reviewed_default(record, review, sources)
                elif isinstance(record, SymbolRecord):
                    validate.symbol(
                        record, review, sources, envelope.default_decision_id
                    )


def _reviewed_default(
    record: DefaultRecord, review: ReviewContext, sources: AdoptionSources
) -> None:
    value = record.data.value
    if value is None:
        return
    subject = record.data.subject
    physical = [
        r.data
        for r in sources.registry(review).records.values()
        if isinstance(r.data, PrintingData)
        and (r.data.card_id, r.data.region) == (subject.card_id, subject.region)
    ]
    expected = tuple(sorted(p.id for p in physical))
    if value.candidates != expected or value.candidates_hash != digest(
        canonical(list[JsonValue](expected))
    ):
        raise ValueError("Reviewed default printing candidate closure/hash mismatch")
    if value.printing_id not in expected:
        raise ValueError(
            "Reviewed default target is not displayable in its card/region"
        )
    for printing in physical:
        for version in sources.printing_sources(printing, review):
            sources.verify_printing(printing, review, version)


def _reviewed_route(
    record: RouteRecord, review: ReviewContext, sources: AdoptionSources
) -> None:
    value = record.data.value
    if value is None:
        return
    registry = sources.registry(review)
    subject = record.data.subject
    candidates = [
        r
        for r in registry.records.values()
        if isinstance(r.data, PrintingData) and r.data.card_no == subject.route_key
    ]
    if {r.data.model_dump()["region"] for r in candidates} != {subject.region} or len(
        candidates
    ) < _MIN_VARIANTS:
        raise ValueError("Reviewed route scope is not same-region official variants")
    if sorted(str(r.data.model_dump()["id"]) for r in candidates) != [
        c.printing_id for c in value.candidates
    ]:
        raise ValueError("Reviewed route candidate closure mismatch")
    for candidate in value.candidates:
        original = registry.records.get(candidate.identity_ref.record_key)
        if (
            original is None
            or original.kind != "printing"
            or original.data.model_dump().get("id") != candidate.printing_id
            or (digest(original.content), original.decision_id)
            != (candidate.identity_ref.record_hash, candidate.identity_ref.decision_id)
        ):
            raise ValueError("Reviewed route identity reference mismatch")
        assert isinstance(original.data, PrintingData)
        for version in sources.printing_sources(original.data, review):
            sources.verify_printing(original.data, review, version)


def _dependency_key(table: str, key: dict[str, JsonValue]) -> bytes:
    return canonical({"table": table, "key": key})


def _owned(record: Record) -> bytes | None:
    subject = record.data.subject.model_dump(mode="json")
    tables = {
        "vocabulary_adoption": "vocabulary",
        "language_adoption": "language",
        "text_symbol_adoption": "text_symbol",
    }
    table = tables.get(record.kind)
    return None if table is None else _dependency_key(table, subject)


def _dependencies(
    effective: tuple[tuple[Record, str], ...], db: Database, plan: TextPlan | None
) -> None:
    owners = {_owned(r): r for r, _ in effective if _owned(r) is not None}
    graph: dict[bytes, set[bytes]] = {}
    for record, _ in effective:
        if record.data.value is None:
            continue
        key = canonical([record.kind, record.data.subject.model_dump(mode="json")])
        graph[key] = set()
        for dependency in record.data.dependencies:
            owner = owners.get(_dependency_key(dependency.table, dependency.key))
            if owner is not None:
                if owner.data.value is None or (
                    isinstance(owner, VocabularyRecord) and not owner.data.value.active
                ):
                    raise ValueError("Adoption dependency is withdrawn or inactive")
                # UI fallback is an ordered lookup list, not recursive dependency evaluation.
                if not isinstance(record, LanguageRecord):
                    graph[key].add(
                        canonical(
                            [owner.kind, owner.data.subject.model_dump(mode="json")]
                        )
                    )
            else:
                _external(dependency.table, dependency.key, db, plan)
    _acyclic(graph)


def _acyclic(graph: dict[bytes, set[bytes]]) -> None:
    done: set[bytes] = set()
    active: set[bytes] = set()

    def visit(key: bytes) -> None:
        if key in active:
            raise ValueError("Adoption dependency cycle or self-reference")
        if key in done:
            return
        active.add(key)
        for target in graph.get(key, set()):
            visit(target)
        active.remove(key)
        done.add(key)

    for key in graph:
        visit(key)


def _external(
    table: str, key: dict[str, JsonValue], db: Database, plan: TextPlan | None
) -> None:
    if table not in {"card", "face", "printing"} or plan is None:
        raise ValueError(
            "Adoption dependency lacks its contract's verified effective projection"
        )
    records = [
        r
        for r in plan.publication_identity().included(table)
        if r.data.model_dump(mode="json").get("id") == key["id"]
    ]
    rows = [r for r in db.rows(table) if r.values["id"] == key["id"]]
    if len(records) != 1 or len(rows) != 1:
        raise ValueError(
            "Adoption dependency is absent/retired from current projection"
        )
    data = records[0].data.model_dump(mode="json")
    fields = {
        "card": ("identity_state", "home_set_id"),
        "face": ("card_id", "ordinal", "side"),
        "printing": ("card_id", "region", "card_no", "variant_key"),
    }[table]
    if any(rows[0].values[field] != data[field] for field in fields):
        raise ValueError(
            "Adoption dependency current identity differs from checked registry"
        )


def _freshness(
    effective: tuple[tuple[Record, str], ...],
    reviews: dict[str, ReviewContext],
    sources: AdoptionSources,
    db: Database,
    plan: TextPlan | None,
) -> None:
    languages = {
        r.data.subject.code
        for r, _ in effective
        if isinstance(r, LanguageRecord) and r.data.value is not None
    }
    for record, decision in effective:
        review = reviews[record.record_key]
        if isinstance(record, LanguageRecord):
            validate.language(record, languages)
        elif isinstance(record, NameRecord):
            validate.names(record, review, sources, plan, decision)
        elif isinstance(record, RouteRecord):
            validate.route_value(record, db)
            _route_identity(record, plan)
        elif isinstance(record, DefaultRecord):
            validate.default_value(record, db)
            _default_closure(record, db, plan)


def _default_closure(
    record: DefaultRecord, db: Database, plan: TextPlan | None
) -> None:
    if record.data.value is None:
        return
    if plan is None:
        raise ValueError(
            "Default adoption requires independently checked display inputs"
        )
    subject = record.data.subject
    expected = sorted(
        str(r.data.model_dump()["id"])
        for r in plan.publication_identity().included("printing")
        if r.data.model_dump()["card_id"] == subject.card_id
        and r.data.model_dump()["region"] == subject.region
    )
    actual = sorted(
        str(r.values["id"])
        for r in db.rows("printing")
        if (r.values["card_id"], r.values["region"])
        == (subject.card_id, subject.region)
    )
    if actual != expected:
        raise ValueError(
            "Default build display closure differs from independent projection"
        )


def _route_identity(record: RouteRecord, plan: TextPlan | None) -> None:
    value = record.data.value
    if value is None:
        return
    if plan is None:
        raise ValueError("Route adoption requires the complete checked registry")
    for candidate in value.candidates:
        ref = candidate.identity_ref
        original = plan.identity.snapshot.records.get(ref.record_key)
        if (
            original is None
            or original.kind != "printing"
            or original.data.model_dump().get("id") != candidate.printing_id
            or (digest(original.content), original.decision_id)
            != (ref.record_hash, ref.decision_id)
        ):
            raise ValueError("Stale route candidate exact identity reference")


def _audit(
    snapshots: tuple[AdoptionSnapshot, ...], db: Database, revision: str
) -> None:
    for snapshot in snapshots:
        for path, content in [
            (snapshot.entry + "/index.yaml", snapshot.index_content),
            *((s.path, s.content) for s in snapshot.shards),
        ]:
            source = "authored:catalog:" + digest(
                canonical([revision, path, digest(content)])
            ).removeprefix("sha256:")
            insert_exact(
                db,
                "source_record",
                {
                    "id": source,
                    "kind": "authored",
                    "sha256": digest(content),
                    "authored_path": "authored/" + path,
                    "authored_revision": revision,
                    "parser_version": "catalog-display-adoption-v1",
                    "url": None,
                    "raw_locator": None,
                    "etag": None,
                    "last_modified": None,
                    "fetched_at": None,
                },
                ("id",),
            )
            for shard in snapshot.shards:
                if shard.path != path:
                    continue
                decision = shard.envelope().decisions[0]
                values: dict[str, Value] = {
                    key: value
                    for key, value in decision.model_dump(
                        mode="json",
                        exclude={"members", "reviewed_precision", "sample_ids"},
                    ).items()
                    if isinstance(value, str) or value is None
                }
                values["sample_ids"] = Json(list[JsonValue](decision.sample_ids))
                values["confidence"] = None
                insert_exact(db, "decision", values, ("id",))
                insert_exact(
                    db,
                    "decision_source",
                    {
                        "decision_id": decision.id,
                        "source_id": source,
                        "role": "catalog_adoption_envelope",
                        "locator": path,
                        "quote": None,
                    },
                    ("decision_id", "source_id", "role"),
                )


def _audit_evidence(
    snapshots: tuple[AdoptionSnapshot, ...], db: Database, sources: AdoptionSources
) -> None:
    for snapshot in snapshots:
        for shard in snapshot.shards:
            envelope = shard.envelope()
            for record in envelope.records:
                for evidence in record.evidence:
                    source = (
                        sources.text(evidence.source_ref, envelope.review_context)[1]
                        if isinstance(evidence, TextEvidence)
                        else sources.image(evidence)
                    )
                    role = "catalog_evidence:" + digest(
                        canonical(evidence.model_dump(mode="json"))
                    ).removeprefix("sha256:")
                    insert_exact(
                        db,
                        "decision_source",
                        {
                            "decision_id": envelope.default_decision_id,
                            "source_id": source.id,
                            "role": role,
                            "locator": canonical(
                                evidence.model_dump(mode="json")
                            ).decode(),
                            "quote": None,
                        },
                        ("decision_id", "source_id", "role"),
                    )


def _project(  # ruff: ignore[too-many-arguments,too-many-positional-arguments,complex-structure,too-many-branches] -- explicit checked record, provenance, text and identity boundaries
    record: Record,
    decision: str,
    review: ReviewContext,
    sources: AdoptionSources,
    db: Database,
    texts: TextInterner,
    plan: TextPlan | None,
) -> None:
    if isinstance(record, VocabularyRecord):
        item = validate.term(record, review, sources)
        if item is None:
            # Withdrawn vocabulary keys remain registered but unavailable for active references.
            previous = [
                r
                for r in db.rows("vocabulary")
                if r.values["kind"] == record.data.subject.kind
                and r.values["code"] == record.data.subject.code
            ]
            if previous:
                db.update(
                    "vocabulary", record.data.subject.model_dump(), {"active": False}
                )
            return
        insert_exact(
            db,
            "vocabulary",
            {
                "kind": item.kind,
                "code": item.code,
                "label_unit_id": texts.intern(item.label),
                "active": item.active,
            },
            ("kind", "code"),
        )
    elif isinstance(record, AliasRecord):
        item_alias = validate.alias(record, review, sources)
        if item_alias is not None:
            value = record.data.value
            assert value is not None
            insert_exact(
                db,
                "search_alias",
                item_alias.model_dump()
                | {
                    "decision_id": decision,
                    "normalizer_version": value.normalizer.version,
                },
                ("kind", "code", "lang", "text"),
            )
    elif isinstance(record, SymbolRecord):
        item_symbol = validate.symbol(record, review, sources, decision)
        if item_symbol is not None:
            data = item_symbol.model_dump(mode="json")
            insert_exact(
                db,
                "text_symbol",
                {
                    "id": item_symbol.id,
                    "code": item_symbol.code,
                    "parameter_schema": Json(item_symbol.parameter_schema),
                    "keyword_id": item_symbol.keyword_id,
                    "spellings": Json(data["spellings"]),
                    "localizations": Json(data["localizations"]),
                    "decision_id": decision,
                },
                ("id",),
            )
    elif isinstance(record, NameRecord):
        populate_rules_names(db)
        for binding in validate.names(record, review, sources, plan, decision):
            register_name(db, binding)
    elif isinstance(record, RouteRecord):
        pid = validate.route_value(record, db)
        if pid is not None:
            insert_exact(
                db,
                "route_override",
                {
                    "namespace": "official",
                    "route_key": record.data.subject.route_key,
                    "printing_id": pid,
                    "decision_id": decision,
                },
                ("route_key",),
            )
    elif isinstance(record, DefaultRecord):
        pid = validate.default_value(record, db)
        if pid is not None:
            insert_exact(
                db,
                "default_printing_override",
                {
                    "card_id": record.data.subject.card_id,
                    "region": record.data.subject.region,
                    "printing_id": pid,
                    "decision_id": decision,
                },
                ("card_id", "region"),
            )
