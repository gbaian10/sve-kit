"""Public workspace for inspecting, writing and checking exact author sources."""

from contextlib import contextmanager
from dataclasses import dataclass
from hashlib import file_digest
from pathlib import Path
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue

from sve_carddb.build import create_database
from sve_carddb.build.database import open_database
from sve_carddb.build.t1 import MINIMUM_CAPABILITIES, compile_build
from sve_carddb.contracts.four_layer import CardNameReference, GlossaryReference
from sve_carddb.core.json import array, canonical, object_value, parse, string
from sve_carddb.core.models import RecordData
from sve_carddb.core.provenance import BuildContext, Revision
from sve_carddb.domains.catalog.adoption_loader import load_adoptions
from sve_carddb.domains.catalog.rules_names import populate_rules_names
from sve_carddb.domains.effects.author_anchors import review_states
from sve_carddb.domains.effects.author_current import CardRejected, CurrentAuthorSources
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_handwritten import ConfigurationError, positions
from sve_carddb.domains.effects.author_inputs import AuthorInputError, Diagnostic
from sve_carddb.domains.effects.author_set import load_author_set
from sve_carddb.domains.effects.author_source_hash import source_version
from sve_carddb.domains.effects.author_sources import SourceError
from sve_carddb.domains.parsed_cards.reader import read as read_parsed
from sve_carddb.domains.parsed_cards.sources import ParsedBatches
from sve_carddb.domains.products import (
    FrozenProducts,
    load_product_identities,
    load_products,
    plan_official_products,
    populate_product_preview,
)
from sve_carddb.domains.registry.preview import FrozenJP, plan_preview
from sve_carddb.domains.registry.records import PrintingData
from sve_carddb.domains.text_observations import FrozenTexts, TextObservations
from sve_carddb.domains.text_observations.closure import close_preview
from sve_carddb.domains.translations.four_layer_authored import FrameRecord
from sve_carddb.domains.translations.glossary.records import TermRecord
from sve_carddb.domains.translations.inputs import Snapshot, load_glossary
from sve_carddb.domains.translations.recognition.rules import load as load_rules
from sve_carddb.domains.translations.sources import Sources
from sve_carddb.workflows.authoring_storage import WriteError, format_yaml, write

if TYPE_CHECKING:
    from collections.abc import Generator

    from sve_carddb.domains.effects.author_inputs import AuthorSchemas
    from sve_carddb.domains.effects.author_set import AuthorInputSet
    from sve_carddb.domains.products import OfficialProducts
    from sve_carddb.domains.registry.preview import PreviewPlan

__all__ = [
    "AuthorInputError",
    "ConfigurationError",
    "Inputs",
    "SourceError",
    "Workspace",
    "WriteError",
    "check",
    "format_yaml",
    "inspect",
    "prepare",
    "write",
]


class Inputs(RecordData):
    """Every profile pins its own sources; no live manifest is consulted."""

    profile: Literal["public", "database"]
    repo: Path
    jp_batch_id: str
    revision: Revision
    authored: Path | None = None
    card_data: Path | None = None
    database: Path | None = None
    database_sha256: str | None = None
    recipe: Path | None = None
    frozen_inputs: Path | None = None


@dataclass(frozen=True)
class Workspace:
    inputs: Inputs
    current: CurrentAuthorSources
    expander: AuthorExpander
    authored: Path


def _configuration(inputs: Inputs) -> None:
    for path in (
        inputs.repo,
        inputs.authored,
        inputs.card_data,
        inputs.database,
        inputs.recipe,
        inputs.frozen_inputs,
    ):
        if path is not None and (not path.is_absolute() or ".." in path.parts):
            raise ConfigurationError(
                "Authoring inputs require absolute paths without traversal"
            )
    database = (
        inputs.database,
        inputs.database_sha256,
        inputs.recipe,
        inputs.frozen_inputs,
    )
    if (inputs.profile == "database" and any(value is None for value in database)) or (
        inputs.profile == "public" and any(value is not None for value in database)
    ):
        raise ConfigurationError(
            "Database profile requires database, SHA-256, recipe and frozen_inputs"
        )


def author_numbers(authored: Path, expander: AuthorExpander) -> frozenset[str]:
    """Include all indexed authors when preparing a whole-set check."""
    inputs = load_author_set(authored, expander.schemas)
    return frozenset(record.record_key for record in inputs.index.records)


def _public_snapshot(snapshot: Snapshot, batch: str) -> Snapshot:
    # Public authoring admits only concepts whose source is available in this profile.
    terms = tuple(
        record
        for record in snapshot.current_records()
        if isinstance(record, TermRecord)
        and (record.data.source_ref is None or record.data.source_ref.batch_id == batch)
    )
    identifiers = {record.data.id for record in terms}
    frames = tuple(
        record
        for record in snapshot.four_layer.records
        if isinstance(record, FrameRecord)
        and all(
            (
                reference.key
                if isinstance(reference, GlossaryReference)
                else reference.term_id
            )
            in identifiers
            for slot in record.data.leaf_schema.slots
            for reference in slot.domain.values
            if isinstance(reference, GlossaryReference | CardNameReference)
        )
    )

    frame_ids = {record.data.id for record in frames}
    files = []
    for path, raw, content in snapshot.closure:
        if path.startswith("translations/semantic-definitions/"):
            files.append((path, raw, content))
            continue
        tree = object_value(parse(content))
        records = [
            record
            for value in array(tree["records"])
            if (record := object_value(value))["kind"]
            in {"glossary_term", "sentence_template"}
            and object_value(record["data"]).get("id") in (identifiers | frame_ids)
        ]
        if records:
            encoded = canonical({**tree, "records": list[JsonValue](records)})
            files.append((path, encoded, encoded))
    closure = tuple(files)
    return Snapshot(
        tuple(file for file in closure if file[0].startswith("translations/glossary/")),
        closure,
    )


def _public_products(
    inputs: Inputs,
    authored: Path,
    identity: PreviewPlan,
    batches: ParsedBatches,
    store: Path,
) -> OfficialProducts:
    identities = load_product_identities(
        authored,
        authored_revision=inputs.revision,
        catalog=load_products(authored, registry=identity.snapshot),
        stores={"public-authoring": store},
        batches=batches,
    )
    return plan_official_products(
        identities,
        FrozenProducts(
            store,
            "public-authoring",
            inputs.jp_batch_id,
            region="jp",
            sources=batches.batch(store, "public-authoring", inputs.jp_batch_id),
        ).pages(),
        identity,
    )


@contextmanager
def prepare(  # ruff: ignore[too-many-locals, too-many-statements] -- keep the complete ordered boundary in one operation
    inputs: Inputs, *, cards: frozenset[str] = frozenset(), main: Path | None = None
) -> Generator[Workspace]:
    """Prepare production identity, vocabulary, sources and the immutable JP reader."""
    from sve_carddb.domains.catalog.loader import prepare as prepare_catalog  # ruff: ignore[import-outside-top-level] -- initialize the production text interner before catalog imports

    _configuration(inputs)
    authored = inputs.authored or inputs.repo / "authored"
    expander = AuthorExpander(inputs.repo / "dsl")
    batches = ParsedBatches(read_parsed(inputs.card_data or inputs.repo / "card-data"))
    snapshot = load_glossary(authored)
    rules = load_rules(inputs.repo)
    capabilities = (*MINIMUM_CAPABILITIES, "en", "translation_evidence")
    if inputs.profile == "database":
        assert inputs.database is not None
        assert inputs.recipe is not None
        assert inputs.frozen_inputs is not None
        with inputs.database.open("rb") as stream:
            actual = file_digest(stream, "sha256").hexdigest()
        if actual != inputs.database_sha256:
            raise ConfigurationError("Authoring database SHA-256 mismatch")
        recipe = object_value(parse(inputs.recipe.read_bytes()))
        context = BuildContext.model_validate_json(
            canonical(object_value(parse(inputs.frozen_inputs.read_bytes()))["context"])
        )
        if (
            context.program_revision != inputs.revision
            or recipe["revision"] != inputs.revision
        ):
            raise ConfigurationError("Authoring frozen revision mismatch")
        if [
            pin["card_batch"]
            for value in array(recipe["sources"])
            if (pin := object_value(value))["region"] == "jp"
        ] != [inputs.jp_batch_id]:
            raise ConfigurationError("Authoring JP batch mismatch")
        stores = {string(recipe["store_id"]): Path(string(recipe["archive"]))}
        prepared = prepare_catalog(
            (load_adoptions(authored, entry="catalog/adoptions"),),
            inputs.repo,
            context,
            stores,
            batches=batches,
        )
        sources = Sources(stores, inputs.repo, context, batches=batches)
        schema = compile_build(
            (
                *capabilities,
                "translation_names",
                "translation_templates",
                "rulings",
                "keyword",
            )
        )
        with open_database(schema, inputs.database) as db:
            yield Workspace(
                inputs,
                CurrentAuthorSources(
                    db,
                    sources,
                    snapshot,
                    prepared.projection.vocabulary,
                    rules,
                    inputs.jp_batch_id,
                ),
                expander,
                authored,
            )
        return
    snapshot = _public_snapshot(snapshot, inputs.jp_batch_id)
    store = inputs.card_data or inputs.repo / "card-data"
    frozen = batches.batch(store, "public-authoring", inputs.jp_batch_id)
    identity = plan_preview(
        authored,
        FrozenJP(
            store,
            "public-authoring",
            inputs.jp_batch_id,
            parser_version="official-jp-exact-v1",
            sources=frozen,
        ),
        regions=("jp",),
    )
    from sve_carddb.domains.effects.author_handwritten import read as read_handwritten  # ruff: ignore[import-outside-top-level] -- only public profile needs to discover shorthand card identities

    numbers = (
        cards
        | author_numbers(authored, expander)
        | (
            frozenset(read_handwritten(main, None).cards)
            if main is not None
            else frozenset()
        )
    )
    printings = tuple(
        record.data
        for record in identity.included("printing")
        if isinstance(record.data, PrintingData)
    )
    identifiers = {
        printing.card_id for printing in printings if printing.card_no in numbers
    }
    selected = close_preview(
        identity,
        frozenset(
            (printing.card_id, printing.region)
            for printing in printings
            if printing.card_id not in identifiers
        ),
    )
    observations = TextObservations(
        selected,
        FrozenTexts(
            store,
            "public-authoring",
            inputs.jp_batch_id,
            region="jp",
            parser_version="official-jp-exact-v1",
            sources=frozen,
        ),
    )
    stores = {"public-authoring": store}
    prepared = prepare_catalog(
        (load_adoptions(authored, entry="catalog/adoptions"),),
        inputs.repo,
        BuildContext.from_inputs(inputs.revision, {}),
        stores,
        identity.snapshot,
        batches=batches,
    )
    vocabulary = prepared.projection.vocabulary
    official = _public_products(inputs, authored, identity, batches, store)
    context = BuildContext.from_inputs(
        inputs.revision,
        observations.configuration(vocabulary, ())
        | {"product_identity": official.identities.configuration()},
    )
    sources = Sources(stores, inputs.repo, context, identity.snapshot, batches=batches)
    with create_database(compile_build(capabilities)) as db:
        with db.transaction():
            populate_product_preview(
                db,
                official.identities.catalog,
                identity,
                authored_revision=inputs.revision,
                build=context,
                languages=prepared.projection.catalog.languages,
                stores=stores,
                batches=batches,
                official=official,
            )
            observations.populate(
                db, build=context, vocabulary=vocabulary, published=()
            )
            populate_rules_names(db)
        yield Workspace(
            inputs,
            CurrentAuthorSources(
                db, sources, snapshot, vocabulary, rules, inputs.jp_batch_id
            ),
            expander,
            authored,
        )


def inspect(workspace: Workspace, card: str) -> dict[str, JsonValue]:
    """Report all source lines, including unsupported candidates, without admission."""
    current = workspace.current
    card_id = current.printing_card(card)
    payload = current.card_payload(card_id, inspect=True)
    fields: list[JsonValue] = [
        {
            "owner": field.field.source.model_dump(mode="json"),
            "lines": [
                {"line": index + 1, "text": text}
                for index, text in enumerate(field.source.text.splitlines())
            ],
        }
        for field in payload.fields
    ]
    candidates = positions(payload, current.classifier)
    inputs = load_author_set(workspace.authored, workspace.expander.schemas)
    stored_inputs = inputs
    inputs, failures = current.resolve_inputs(inputs, workspace.expander)
    uses: list[JsonValue] = []
    record_keys = {
        record.record_key
        for record in inputs.index.records
        if record.card_id == card_id and record.kind == "author_effect_set"
    }
    for file in inputs.author_files:
        tree = object_value(workspace.expander.schemas.author_tree(file.raw))
        uses.extend(
            {
                "file": file.path,
                "record_key": key,
                "card": object_value(tree["cards"])[key],
            }
            for key in object_value(tree["cards"]).keys() & record_keys
        )
    bodies = [
        workspace.expander.schemas.body_tree(file.raw) for file in inputs.body_files
    ]
    stored = _stored_inspection(
        stored_inputs, workspace.expander.schemas, record_keys, failures
    )
    card_gap = None
    try:
        current.keyword_obligations(card_id)
    except SourceError as error:
        card_gap = error.reason
    return {
        "card_no": card,
        "card_id": card_id,
        "source": source_version(payload.payload),
        "payload": payload.payload.model_dump(mode="json"),
        "fields": fields,
        "candidates": [candidate.report() for candidate in candidates],
        "existing": uses,
        **stored,
        "bodies": bodies,
        "gaps": [
            candidate.report() for candidate in candidates if not candidate.supported
        ],
        "card_gap": card_gap,
    }


def _stored_inspection(
    inputs: AuthorInputSet,
    schemas: AuthorSchemas,
    record_keys: set[str],
    failures: dict[str, Diagnostic],
) -> dict[str, JsonValue]:
    stored_anchors: list[JsonValue] = []
    handwritten: list[JsonValue] = []
    for file in inputs.anchor_files:
        tree = object_value(schemas.storage_tree(file.raw, "effect_anchor_set"))
        stored_anchors.extend(
            {
                "file": file.path,
                "record_key": key,
                "card": object_value(tree["cards"])[key],
            }
            for key in object_value(tree["cards"]).keys() & record_keys
        )
    for file in inputs.author_files:
        tree = object_value(schemas.storage_tree(file.raw, "author_effect_set"))
        handwritten.extend(
            {
                "file": file.path,
                "record_key": key,
                "card": object_value(tree["cards"])[key],
            }
            for key in object_value(tree["cards"]).keys() & record_keys
        )
    failure = next(
        (failure for number, failure in failures.items() if number in record_keys), None
    )
    anchor_diagnostic: JsonValue = (
        None
        if failure is None
        else {
            "stage": failure.stage,
            "reason": failure.reason,
            "card_no": failure.card_no,
            "card_line": failure.card_line,
            "path": list[JsonValue](failure.path),
            "expected": failure.expected,
            "actual": failure.actual,
        }
    )
    return {
        "handwritten": handwritten,
        "stored_anchors": stored_anchors,
        "anchor_diagnostic": anchor_diagnostic,
        "body_definitions": [
            schemas.storage_tree(file.raw, "effect_body_shard")
            for file in inputs.body_files
        ],
    }


def check(
    workspace: Workspace,
    *,
    compare_public: Inputs | None = None,
    base: str | None = None,
    baseline_root: Path | None = None,
) -> dict[str, JsonValue]:
    """Check every authored card and full inventory, independently of display filters."""
    results = workspace.current.validate(workspace.authored, workspace.expander)
    rejected = [
        result
        for result in results
        if isinstance(result, CardRejected)
        and result.record.kind == "author_effect_set"
    ]
    if rejected:
        raise AuthorInputError(rejected[0].diagnostic)
    regression: JsonValue = None
    if (base is None) != (baseline_root is None):
        raise ConfigurationError(
            "Prototype regression requires base and baseline_root together"
        )
    if base is not None and baseline_root is not None:
        from sve_carddb.workflows.authoring_regression import check_regression  # ruff: ignore[import-outside-top-level] -- optional fixed-tree prototype regression

        regression = check_regression(
            workspace,
            base,
            baseline_root,
            load_author_set(workspace.authored, workspace.expander.schemas),
            results,
        )
    selection = workspace.current.selection_input(
        workspace.authored, workspace.expander
    )
    if compare_public is not None:
        if workspace.inputs.profile != "database" or compare_public.profile != "public":
            raise ConfigurationError(
                "Selection comparison requires independent database/public profiles"
            )
        with prepare(compare_public) as public:
            other = check(public)
        if canonical(selection) != canonical(other["selection"]):
            raise AuthorInputError(Diagnostic("source", "selection_mismatch"))
    return {
        "status": "ok",
        "selection": selection,
        "compared_public": compare_public is not None,
        "regression": regression,
        "reviews": review_states(
            load_author_set(workspace.authored, workspace.expander.schemas)
        ),
    }
