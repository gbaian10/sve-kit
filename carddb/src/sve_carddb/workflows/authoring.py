"""Public workspace for inspecting, writing and checking exact author sources."""

import subprocess  # ruff: ignore[suspicious-subprocess-import] -- fixed local Git argv records provenance without author pins
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from shutil import which
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue

from sve_carddb.build import create_database
from sve_carddb.build.database import open_database
from sve_carddb.build.t1 import MINIMUM_CAPABILITIES, compile_build
from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField
from sve_carddb.core.json import array, canonical, object_value, parse, string
from sve_carddb.core.models import RecordData
from sve_carddb.core.provenance import BuildContext, Revision
from sve_carddb.domains.catalog.adoption_loader import load_adoptions
from sve_carddb.domains.catalog.rules_name_adoptions import collab_hashes, evaluate
from sve_carddb.domains.catalog.rules_names import populate_rules_names
from sve_carddb.domains.effects.author_counters import check_counter_registration
from sve_carddb.domains.effects.author_current import CardRejected, CurrentAuthorSources
from sve_carddb.domains.effects.author_direct import inventory, materialize
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_handwritten import ConfigurationError
from sve_carddb.domains.effects.author_inputs import (
    AuthorInputError,
    Diagnostic,
    SourceError,
)
from sve_carddb.domains.effects.author_resolution import (
    body_usage,
    handwriting,
    review_states,
)
from sve_carddb.domains.effects.author_set import load_author_set
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
from sve_carddb.domains.registry.transitions.renewal import load_identity_registry
from sve_carddb.domains.text_observations import FrozenTexts, TextObservations
from sve_carddb.domains.text_observations.closure import close_preview
from sve_carddb.domains.translations.four_layer_sources import card_field
from sve_carddb.workflows.authoring_storage import WriteError, format_yaml, write

if TYPE_CHECKING:
    from collections.abc import Generator

    from sve_carddb.domains.parsed_cards.storage import Catalog
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
    """Read reviewed source context without consulting a live manifest."""

    profile: Literal["public", "database"]
    repo: Path
    jp_batch_id: str
    revision: Revision | None = None
    authored: Path | None = None
    card_data: Path | None = None
    database: Path | None = None
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
        inputs.recipe,
        inputs.frozen_inputs,
    )
    if (inputs.profile == "database" and any(value is None for value in database)) or (
        inputs.profile == "public" and any(value is not None for value in database)
    ):
        raise ConfigurationError(
            "Database profile requires database, recipe and frozen_inputs"
        )


def author_numbers(authored: Path, expander: AuthorExpander) -> frozenset[str]:
    """Include all indexed authors when preparing a whole-set check."""
    inputs = load_author_set(authored, expander.schemas)
    return frozenset(record.record_key for record in inputs.index.records)


def _public_products(
    inputs: Inputs,
    authored: Path,
    identity: PreviewPlan,
    batches: ParsedBatches,
    store: Path,
) -> OfficialProducts:
    assert inputs.revision is not None
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
def prepare(  # ruff: ignore[too-many-locals] -- keep the complete ordered boundary in one operation
    inputs: Inputs,
    *,
    cards: frozenset[str] = frozenset(),
    main: Path | None = None,
    parsed: Catalog | None = None,
) -> Generator[Workspace]:
    """Prepare the current JP identities, source texts and construction names."""
    from sve_carddb.domains.catalog.loader import prepare as prepare_catalog  # ruff: ignore[import-outside-top-level] -- initialize the production text interner before catalog imports

    _configuration(inputs)
    if inputs.revision is None:
        executable = which("git")
        if executable is None:
            raise ConfigurationError("Git is required to record repository revision")
        try:
            revision = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed Git argv reports provenance, not an admission pin
                [executable, "-C", str(inputs.repo), "rev-parse", "HEAD"],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
        except OSError, subprocess.CalledProcessError:
            raise ConfigurationError("Cannot read repository revision") from None
        inputs = inputs.model_copy(update={"revision": revision})
    assert inputs.revision is not None
    authored = inputs.authored or inputs.repo / "authored"
    expander = AuthorExpander(inputs.repo / "dsl")
    batches = ParsedBatches(
        parsed
        if parsed is not None
        else read_parsed(inputs.card_data or inputs.repo / "card-data")
    )
    capabilities = (*MINIMUM_CAPABILITIES, "en", "translation_evidence")
    if inputs.profile == "database":
        assert inputs.database is not None
        assert inputs.recipe is not None
        assert inputs.frozen_inputs is not None
        recipe = object_value(parse(inputs.recipe.read_bytes()))
        BuildContext.model_validate_json(
            canonical(object_value(parse(inputs.frozen_inputs.read_bytes()))["context"])
        )
        if [
            pin["card_batch"]
            for value in array(recipe["sources"])
            if (pin := object_value(value))["region"] == "jp"
        ] != [inputs.jp_batch_id]:
            raise ConfigurationError("Authoring JP batch mismatch")
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
                CurrentAuthorSources(db),
                expander,
                authored,
            )
        return
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
        snapshot=load_identity_registry(authored, repository=inputs.repo),
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
    adoptions = load_adoptions(authored, entry="catalog/adoptions")
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
        collab_names=collab_hashes((adoptions,)),
    )
    stores = {"public-authoring": store}
    prepared = prepare_catalog(
        (adoptions,),
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
            populate_rules_names(db, evaluate(prepared.names, observations.plan)[0])
        yield Workspace(
            inputs,
            CurrentAuthorSources(db),
            expander,
            authored,
        )


def _inspection_lines(
    current: CurrentAuthorSources, payload: dict[str, JsonValue]
) -> list[JsonValue]:
    fields: list[JsonValue] = []
    for raw_face in array(payload["faces"]):
        face = object_value(raw_face)
        owner = FaceRevisionOwner(
            kind="face_revision", revision_id=string(face["revision_id"])
        )
        for raw_field in array(face["fields"]):
            field = object_value(raw_field)
            ordinal = field["ordinal"]
            assert ordinal is None or type(ordinal) is int
            address = OwnerField(
                owner=owner,
                field="effect" if ordinal is None else "section",
                ordinal=ordinal,
            )
            text = card_field(current.db, address).text
            fields.append(
                {
                    "owner": address.model_dump(mode="json"),
                    "face_id": face["face_id"],
                    "lines": [
                        {"line": index + 1, "text": line}
                        for index, line in enumerate(text.splitlines())
                    ],
                }
            )

    return fields


def inspect(workspace: Workspace, card: str) -> dict[str, JsonValue]:
    """Report current rule lines and the existing author records for a permanent card."""
    current = workspace.current
    payload = inventory(current, card)
    card_id = string(payload["card_id"])

    schemas = workspace.expander.schemas
    inputs = load_author_set(workspace.authored, schemas)
    authored = handwriting(inputs)
    handwritten: list[JsonValue] = []
    existing: list[JsonValue] = []
    resolution_diagnostic: JsonValue = None
    for record in inputs.index.records:
        if record.card_id != card_id or record.kind != "author_effect_set":
            continue
        number = record.record_key
        entry: dict[str, JsonValue] = {"file": record.file, "record_key": number}
        handwritten.append({**entry, "card": authored.cards[number]})
        try:
            existing.append({**entry, "card": materialize(current, number, authored)})
        except SourceError as error:
            resolution_diagnostic = {
                "stage": "source",
                "reason": error.reason,
                "card_no": number,
            }
        except AuthorInputError as error:
            diagnostic = error.diagnostic
            resolution_diagnostic = {
                "stage": diagnostic.stage,
                "reason": diagnostic.reason,
                "card_no": number,
                "card_line": diagnostic.card_line,
                "path": list[JsonValue](diagnostic.path),
            }
    return {
        "card_no": card,
        "card_id": card_id,
        "payload": payload,
        "fields": _inspection_lines(current, payload),
        "existing": existing,
        "handwritten": handwritten,
        "resolution_diagnostic": resolution_diagnostic,
        "body_definitions": [
            schemas.storage_tree(file.raw, "effect_body_shard")
            for file in inputs.body_files
        ],
    }


def check(
    workspace: Workspace,
    *,
    compare_public: Inputs | None = None,
) -> dict[str, JsonValue]:
    """Check every authored card and full inventory, independently of display filters."""
    inputs = load_author_set(workspace.authored, workspace.expander.schemas)
    results = workspace.current.validate_inputs(inputs, workspace.expander)
    rejected = [
        result
        for result in results
        if isinstance(result, CardRejected)
        and result.record.kind == "author_effect_set"
    ]
    if rejected:
        raise AuthorInputError(rejected[0].diagnostic)
    check_counter_registration(workspace.inputs.repo / "authored", results)
    selection = workspace.current.selection_from_results(inputs, results)
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
        "regression": None,
        "reviews": review_states(inputs),
        "body_usage": body_usage(inputs),
    }
