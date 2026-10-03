"""Small adopted name overrides over one shared synthetic frozen registry."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import BuildContext
from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.extract import official_en, official_jp
from sve_carddb.extract.compare_jp import legacy_projection
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.manifest import Kind, Region
from sve_carddb.registry.build import build as build_registry
from sve_carddb.registry.inputs import Mapping
from sve_carddb.registry.records import PrintingData
from sve_carddb.registry.review import Inputs as RegistryInputs
from sve_carddb.registry.review import Receipt
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.registry.storage import plan_files, read_yaml, write_files
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.source_archive import ArchiveStore, seal_batch
from sve_carddb.sources.official_en import card_url as en_url
from sve_carddb.text_observations.archive import FrozenTexts
from sve_carddb.text_observations.models import FaceObservation, candidate_revision_id
from sve_carddb.translations.importer import Inputs
from sve_carddb.translations.loader import load_glossary
from sve_carddb.translations.name_replay import NameReplay, replay_names
from sve_carddb.translations.sources import Sources

from .adoption_fixtures import commit
from .digital_link_import_fixtures import Fixture, make_fixture
from .en_extract_fixtures import page
from .test_glossary_adoption import authored
from .test_registry_preview_archive import RAW
from .test_source_archive import _put, _resource
from .translation_fixtures import envelope, write

if TYPE_CHECKING:
    from pathlib import Path


def name_term(
    key: str = "name.synthetic", text: str = "Synthetic card"
) -> dict[str, JsonValue]:
    record = authored(key, category="card_name")
    object_value(record["data"])["authored_source_ja"] = text
    return record


def human(records: list[dict[str, JsonValue]]) -> dict[str, JsonValue]:
    shard = envelope(records)
    decision = object_value(array(shard["decisions"])[0])
    decision["reviewed_by"] = "gbaian10"
    if records[0]["kind"] == "card_name_concept":
        decision["policy_id"] = "card-name-concept-v1"
    return shard


@dataclass(frozen=True)
class Case:
    frozen: Fixture
    revision_id: str
    basis: dict[str, JsonValue]

    def concept(
        self,
        *,
        key: str = "name.synthetic",
        number: int = 1,
        previous: JsonValue = None,
    ) -> dict[str, JsonValue]:
        subject: dict[str, JsonValue] = {
            "card_id": self.frozen.card.id,
            "face_id": self.frozen.face.id,
            "source_lang": "ja",
            "source_hash": self.frozen.jp.text_hash,
        }
        return {
            "kind": "card_name_concept",
            "filing_key": "concepts",
            "record_key": canonical(["card_name_concept", subject, number]).decode(),
            "data": {
                "subject": subject,
                "term_id": "term:" + key,
                "source_ref": self.frozen.jp.model_dump(mode="json"),
                "identity_basis": parse(canonical(self.basis)),
                "reason": "Synthetic disambiguated concept.",
                "adoption_no": number,
                "predecessor": previous,
            },
            "evidence": [],
        }

    def assignment(
        self,
        *,
        variant: str = "synthetic",
        key: str = "name.synthetic",
        number: int = 1,
        previous: JsonValue = None,
    ) -> dict[str, JsonValue]:
        owner: dict[str, JsonValue] = {
            "kind": "face_revision",
            "revision_id": self.revision_id,
        }
        return {
            "kind": "context_assignment",
            "filing_key": "assignments",
            "record_key": canonical(
                ["context_assignment", owner, "name", None, number]
            ).decode(),
            "data": {
                "owner": owner,
                "field": "name",
                "ordinal": None,
                "source_hash": self.frozen.jp.text_hash,
                "variant": variant,
                "concept_key": key,
                "reason": "Synthetic true homonym.",
                "adoption_no": number,
                "predecessor": previous,
            },
            "evidence": [
                {"source_ref": self.frozen.jp.model_dump(mode="json"), "role": "name"}
            ],
        }

    def stage(
        self, shards: dict[str, dict[str, JsonValue]]
    ) -> tuple[Inputs, BuildContext]:
        write(self.frozen.root / "authored", shards)
        revision = commit(self.frozen.root)
        inputs = Inputs(self.frozen.root / "authored", self.frozen.root, revision)
        config = (
            object_value(parse(self.frozen.build.configuration.encode()))
            | inputs.configuration()
        )
        return inputs, BuildContext.from_inputs(
            self.frozen.program,
            {
                p.name: (self.frozen.root / p.name).read_bytes()
                for p in self.frozen.build.dependencies
            },
            config,
        )

    def replay(
        self, shards: dict[str, dict[str, JsonValue]]
    ) -> tuple[NameReplay, Inputs, BuildContext]:
        inputs, build = self.stage(shards)
        snapshot = load_glossary(inputs.root)
        originals = {
            r.data.id: str(r.data.authored_source_ja)
            for r, _ in snapshot.records()
            if r.kind == "glossary_term"
        }
        sources = Sources({"test-store": self.frozen.store}, inputs.repository, build)
        replay, _ = replay_names(snapshot, originals, inputs, sources)
        return replay, inputs, build


def make_case(root: Path) -> Case:
    frozen = make_fixture(root)
    ref = frozen.jp
    texts = FrozenTexts(
        frozen.store, ref.store_id, ref.batch_id, region="jp", parser_version=ref.parser
    )

    card = texts.version("jp", frozen.printing.card_no, ref.source_version_id)
    item = FaceObservation(
        card_id=frozen.card.id,
        printing_id=frozen.printing.id,
        face_id=frozen.face.id,
        region="jp",
        card_no=frozen.printing.card_no,
        source_index=0,
        card=card,
        content=card.projected(0),
    )
    return Case(
        frozen,
        candidate_revision_id(item),
        {
            "authored_revision": frozen.authored,
            "registry_index_hash": digest(
                canonical(read_yaml(root / "authored/ids/index.yaml"))
            ),
            "transition_index_hash": None,
        },
    )


@dataclass(frozen=True)
class Mixed:
    case: Case
    printing: PrintingData
    name_ref: SourceRef
    revision_id: str


def make_mixed_case(root: Path) -> Mixed:  # ruff: ignore[too-many-locals] -- one module baseline seals both regions and builds their real identity records
    case = make_case(root)
    data = case.frozen.store.parent / "data"
    store = ArchiveStore(
        data,
        data / "manifest/manifest.sqlite",
        data / "manifest/.lock",
        case.frozen.store,
        "test-store",
        (),
    )
    number = "SYN-EN001"
    raw = page(number).replace(b"Synthetic front", b"Synthetic card")
    _put(
        store,
        replace(
            _resource(en_url(number), "raw/en.html", raw, Kind.CARD), region=Region.EN
        ),
        raw,
    )
    sealed = seal_batch(store)
    registry_inputs = RegistryInputs(
        jp={
            "SYN-001": legacy_projection(
                official_jp.extract_card(RAW, number="SYN-001")
            )
        },
        en={
            number: official_en.legacy_projection(
                official_en.extract_card(raw, number=number)
            )
        },
        mapping=Mapping(targets={number: None}, original_art=set(), reskins={}),
        receipt=Receipt(
            policy="identity-init-2026-09-28-v1",
            reviewed_by="Synthetic human",
            reviewed_on="2026-09-28",
            input_hashes={"jp": digest(b"synthetic cards")},
        ),
    )
    write_files(
        plan_files(
            root / "authored",
            build_registry(registry_inputs, {}),
            "Synthetic human",
            "2026-09-28",
        )
    )
    registry = load_registry(root / "authored")
    printing = next(
        r.data
        for r in registry.records.values()
        if isinstance(r.data, PrintingData) and r.data.region == "en"
    )
    inventory = FrozenSources(store.root, store.store_id, sealed.batch_id)
    version = next(
        item.source_version_id
        for item in inventory.inventory.current
        if item.url == en_url(number)
    )
    ref = SourceRef(
        store_id=store.store_id,
        batch_id=sealed.batch_id,
        source_version_id=version,
        parser="translation-en-v1",
        locator="/faces/0/name",
        text_hash=digest(b"Synthetic card"),
    )
    texts = FrozenTexts(
        store.root,
        store.store_id,
        sealed.batch_id,
        region="en",
        parser_version=ref.parser,
    )
    card = texts.version("en", number, version)
    face = printing.source_face_map[0].face_id
    item = FaceObservation(
        card_id=printing.card_id,
        printing_id=printing.id,
        face_id=face,
        region="en",
        card_no=number,
        source_index=0,
        card=card,
        content=card.projected(0),
    )
    revision = commit(root)
    config = object_value(parse(case.frozen.build.configuration.encode()))
    object_value(config["translation_recipes"])[ref.parser] = {
        "version": ref.parser,
        "program_revision": case.frozen.program,
        "code_path": "carddb/src/sve_carddb/translations/sources.py",
        "code_hash": digest(
            (root / "carddb/src/sve_carddb/translations/sources.py").read_bytes()
        ),
        "config": {"provider": "en"},
        "config_hash": digest(canonical({"provider": "en"})),
    }
    frozen = replace(
        case.frozen, jp=case.frozen.jp.model_copy(update={"batch_id": sealed.batch_id})
    )
    frozen = replace(frozen, build=frozen.changed(config))
    case = replace(
        case,
        frozen=frozen,
        basis={
            "authored_revision": revision,
            "registry_index_hash": digest(
                canonical(read_yaml(root / "authored/ids/index.yaml"))
            ),
            "transition_index_hash": None,
        },
    )
    return Mixed(case, printing, ref, candidate_revision_id(item))


def copied(case: Case, root: Path) -> Case:
    import shutil  # ruff: ignore[import-outside-top-level] -- clones stay local to this synthetic fixture

    shutil.copytree(
        case.frozen.root,
        root,
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc"),
    )
    return replace(
        case,
        frozen=replace(
            case.frozen,
            root=root,
            store=root / case.frozen.store.relative_to(case.frozen.root),
        ),
    )
