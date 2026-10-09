"""One sealed synthetic JP/API and authored link baseline per module."""

import shutil
from dataclasses import dataclass, replace
from datetime import date
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.core.json import array, canonical, digest, object_value, parse
from sve_carddb.core.provenance import BuildContext
from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.catalog.adoption_models import SourceRef
from sve_carddb.domains.digital.links.importer import Inputs
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.registry.build import build as build_registry
from sve_carddb.domains.registry.inputs import Mapping
from sve_carddb.domains.registry.records import CardData, FaceData, PrintingData
from sve_carddb.domains.registry.review import InitDecisions
from sve_carddb.domains.registry.review import Inputs as RegistryInputs
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.registry.storage import plan_files, read_yaml, write_files
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.translations.digital import configuration
from sve_carddb.domains.translations.sources import Sources
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import seal_batch
from sve_carddb.parse.pages.extract_jp import extract_card
from sve_carddb.parse.pages.official_jp import card_url

from .adoption_fixtures import commit, git
from .digital_link_fixtures import envelope, record, write
from .registry_observation_fixtures import parsed_card
from .test_registry_preview_archive import RAW
from .test_source_archive import _put, _resource, _store
from .translation_fixtures import INSTANT

CODE_PATH = "carddb/src/sve_carddb/domains/translations/sources.py"
RUNTIME = (CODE_PATH,)


if TYPE_CHECKING:
    from collections.abc import Callable

    from sve_carddb.build import Database


@dataclass(frozen=True)
class Fixture:
    root: Path
    store: Path
    program: str
    authored: str
    build: BuildContext
    record: bytes
    shard: bytes
    card: CardData
    face: FaceData
    printing: PrintingData
    refs: tuple[SourceRef, ...]
    jp: SourceRef
    others: tuple[tuple[CardData, FaceData, PrintingData, SourceRef], ...] = ()

    def inputs(self) -> Inputs:
        return Inputs(self.root / "authored", self.root, self.authored)

    def sources(self) -> Sources:
        return Sources({"test-store": self.store}, self.root, self.build)

    def changed(self, config: dict[str, JsonValue]) -> BuildContext:
        return BuildContext.from_inputs(
            self.program,
            config,
        )

    def publish(self, db: Database) -> None:
        with db.transaction():
            family = db.rows("product_family")[0].values
            if self.card.home_set_id != family["id"]:
                db.insert(
                    "product_family",
                    dict(family)
                    | {
                        "id": self.card.home_set_id,
                        "code": "link-fixture",
                        "public_code": "LINKFIXTURE",
                    },
                )
            db.insert("card", self.card.model_dump(mode="json", round_trip=True))
            db.insert("face", self.face.model_dump(mode="json", round_trip=True))
            source = self.sources().text(self.jp)[2]
            insert_raw_sources(db, (source,))
            unit = TextInterner(db).intern(
                LocalizedText(lang="ja", text="Synthetic card")
            )
            if not any(
                r.values["code"] == "follower" and r.values["kind"] == "type"
                for r in db.rows("vocabulary")
            ):
                db.insert(
                    "vocabulary",
                    {
                        "kind": "type",
                        "code": "follower",
                        "label_unit_id": unit,
                        "active": True,
                    },
                )
            db.insert(
                "face_revision",
                {
                    "id": "link-revision",
                    "face_id": self.face.id,
                    "region": "jp",
                    "revision": 1,
                    "temporal_status": "unknown",
                    "observed_at": INSTANT,
                    "change_kind": "initial",
                    "name_unit_id": unit,
                    "effect_unit_id": unit,
                    "type_code": "follower",
                    "source_id": source.id,
                },
            )

            for index, (card, face, _, ref) in enumerate(self.others, 1):
                db.insert("card", card.model_dump(mode="json", round_trip=True))
                db.insert("face", face.model_dump(mode="json", round_trip=True))
                source = self.sources().text(ref)[2]
                insert_raw_sources(db, (source,))
                primary = next(
                    r.values
                    for r in db.rows("face_revision")
                    if r.values["id"] == "link-revision"
                )
                db.insert(
                    "face_revision",
                    dict(primary)
                    | {
                        "id": f"other-link-revision-{index}",
                        "face_id": face.id,
                        "source_id": source.id,
                    },
                )


def make_fixture(  # ruff: ignore[complex-structure,too-many-statements,too-many-locals] -- one reusable sealed baseline avoids expensive per-test setup
    root: Path,
    *,
    dual: bool = False,
) -> Fixture:
    repository = Path(__file__).resolve().parents[2]
    for name in RUNTIME:
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(repository / name, target)
    git(root, "init")
    store = _store(root / "fixture-store")
    _put(store, _resource(card_url("SYN-001"), "raw/jp.html", RAW, Kind.CARD), RAW)
    if dual:
        _put(
            store, _resource(card_url("SYN-002"), "raw/jp-2.html", RAW, Kind.CARD), RAW
        )
    for lang, name in (("ja", "Synthetic card"), ("cht", "合成測試名")):
        raw = canonical(
            {
                "data_headers": {"result_code": 1},
                "data": {
                    "count": 1,
                    "cards": {"22345678": {}},
                    "card_details": {
                        "22345678": {
                            "common": {
                                "card_id": 22345678,
                                "name": name,
                                "class": 1,
                                "type": 1,
                                "is_token": False,
                                "base_card_id": 22345678,
                                "original_card_id": None,
                            },
                            "evo": {"skill_text": "Synthetic evolved effect"},
                        }
                    },
                },
            }
        )
        if dual:
            document = object_value(parse(raw))
            data = object_value(document["data"])
            details = object_value(data["card_details"])
            other = object_value(parse(canonical(details["22345678"])))
            object_value(other["common"]).update(
                card_id=22345679,
                base_card_id=22345679,
                name=name if lang == "ja" else "第二個測試譯名",
            )
            details["22345679"] = other
            data["count"] = 2
            raw = canonical(document)
        resource = replace(
            _resource(
                "https://shadowverse-wb.com/web/CardList/cardList?lang="
                + lang
                + "&include_token=1&offset=0",
                "raw/" + lang + ".json",
                raw,
                Kind.API,
            ),
            region=SourceRegion.SVWB,
            content_type="application/json",
        )
        _put(store, resource, raw)
    sealed = seal_batch(store)
    frozen = FrozenSources(store.root, store.store_id, sealed.batch_id)
    refs = []
    jp = None
    jp_refs = {}
    for current in frozen.inventory.current:
        descriptor = frozen.descriptor(current.source_version_id)
        if descriptor.provider == "jp":
            jp = SourceRef(
                batch_id=sealed.batch_id,
                source_version_id=current.source_version_id,
                parser="translation-jp-v1",
                locator="/faces/0/name",
                text_hash=digest(b"Synthetic card"),
            )
            jp_refs[descriptor.url] = jp
        else:
            _, raw, _ = frozen.read(
                current.source_version_id, parser_version="translation-svwb-v1"
            )
            text = object_value(
                object_value(
                    object_value(
                        object_value(object_value(parse(raw))["data"])["card_details"]
                    )["22345678"]
                )["common"]
            )["name"]
            assert isinstance(text, str)
            refs.append(
                SourceRef(
                    batch_id=sealed.batch_id,
                    source_version_id=current.source_version_id,
                    parser="translation-svwb-v1",
                    locator="/data/card_details/22345678/common/name",
                    text_hash=digest(text.encode()),
                )
            )
    jp = jp_refs[card_url("SYN-001")]
    physical = parsed_card(extract_card(RAW, number="SYN-001"))
    registry_inputs = RegistryInputs(
        jp={"SYN-001": physical}
        | (
            {"SYN-002": parsed_card(extract_card(RAW, number="SYN-002"))}
            if dual
            else {}
        ),
        en={},
        mapping=Mapping(targets={}, original_art=set(), reskins={}),
        decisions=InitDecisions(
            separate_groups={"jp:SYN-001": "synthetic-a", "jp:SYN-002": "synthetic-b"}
            if dual
            else {},
        ),
        as_of=date(2026, 9, 28),
        jp_hash=digest(b"synthetic cards"),
    )
    write_files(plan_files(root / "authored", build_registry(registry_inputs, {})))
    registry = load_registry(root / "authored")
    printing = next(
        r.data
        for r in registry.records.values()
        if isinstance(r.data, PrintingData) and r.data.card_no == "SYN-001"
    )
    card = next(
        r.data
        for r in registry.records.values()
        if isinstance(r.data, CardData) and r.data.id == printing.card_id
    )
    face = next(
        r.data
        for r in registry.records.values()
        if isinstance(r.data, FaceData) and r.data.card_id == card.id
    )
    others = []
    if dual:
        printing2 = next(
            r.data
            for r in registry.records.values()
            if isinstance(r.data, PrintingData) and r.data.card_no == "SYN-002"
        )
        card2 = next(
            r.data
            for r in registry.records.values()
            if isinstance(r.data, CardData) and r.data.id == printing2.card_id
        )
        face2 = next(
            r.data
            for r in registry.records.values()
            if isinstance(r.data, FaceData) and r.data.card_id == card2.id
        )
        others.append((card2, face2, printing2, jp_refs[card_url("SYN-002")]))
    program = commit(root)
    config: dict[str, JsonValue] = {
        "catalog_registry": {
            "authored_revision": program,
            "index_path": "authored/ids/index.yaml",
            "index_hash": digest(
                canonical(read_yaml(root / "authored/ids/index.yaml"))
            ),
        },
        "digital_link_sources": [{"batch_id": sealed.batch_id}],
        "translation_recipes": {},
    }
    recipes = object_value(config["translation_recipes"])
    for provider in ("jp", "svwb"):
        parser = "translation-" + provider + "-v1"
        recipes[parser] = {"version": parser, "config": {"provider": provider}}
    refs = sorted(
        refs, key=lambda r: canonical(r.model_dump(mode="json", round_trip=True))
    )
    config.update(
        configuration(
            tuple(refs),
            (("svwb", "22345678"),) + ((("svwb", "22345679"),) if dual else ()),
        )
    )
    r = record()
    subject = object_value(r["subject"])
    value = object_value(r["value"])
    subject["card_id"] = card.id
    subject["face_id"] = face.id
    value["sve_names"] = [
        {
            "printing_id": printing.id,
            "face_id": face.id,
            "name_ref": jp.model_dump(mode="json", round_trip=True),
        }
    ]
    value["digital_names"] = sorted(
        [
            {
                "phase": "normal",
                "lang": "ja"
                if "lang=ja" in frozen.descriptor(ref.source_version_id).url
                else "zh-Hant",
                "name_ref": ref.model_dump(mode="json", round_trip=True),
            }
            for ref in refs
        ],
        key=lambda n: (str(object_value(n)["phase"]), str(object_value(n)["lang"])),
    )
    records = [r]
    for other_card, other_face, other_printing, other_ref in others:
        extra = object_value(parse(canonical(r)))
        object_value(extra["subject"]).update(
            card_id=other_card.id, face_id=other_face.id, official_id="22345679"
        )
        other_value = object_value(extra["value"])
        other_value["sve_names"] = [
            {
                "printing_id": other_printing.id,
                "face_id": other_face.id,
                "name_ref": other_ref.model_dump(mode="json", round_trip=True),
            }
        ]
        for digital_name in array(other_value["digital_names"]):
            item = object_value(digital_name)
            ref = object_value(item["name_ref"])
            ref["locator"] = "/data/card_details/22345679/common/name"
            if item["lang"] == "zh-Hant":
                ref["text_hash"] = digest("第二個測試譯名".encode())
        records.append(extra)
    shard = envelope(records)
    write(root / "authored", {"digital-links/links/synthetic/001.yaml": shard})
    authored = commit(root)
    config.update(Inputs(root / "authored", root, authored).configuration())
    build = BuildContext.from_inputs(program, config)
    return Fixture(
        root,
        store.root,
        program,
        authored,
        build,
        canonical(r),
        canonical(shard),
        card,
        face,
        printing,
        tuple(refs),
        jp,
        tuple(others),
    )


def copied(fixture: Fixture, root: Path) -> Fixture:
    shutil.copytree(fixture.root, root)
    return replace(
        fixture, root=root, store=root / fixture.store.relative_to(fixture.root)
    )


def with_records(fixture: Fixture, records: list[dict[str, JsonValue]]) -> Fixture:
    shard = envelope(records)
    write(fixture.root / "authored", {"digital-links/links/synthetic/001.yaml": shard})
    authored = commit(fixture.root)
    inputs = Inputs(fixture.root / "authored", fixture.root, authored)
    config = object_value(parse(fixture.build.configuration.encode()))
    config.update(inputs.configuration())
    return replace(fixture, authored=authored, build=fixture.changed(config))


def current_api(  # ruff: ignore[too-many-locals] -- synthetic resealing preserves both old and current input closures
    fixture: Fixture, transform: Callable[[dict[str, JsonValue], str], None]
) -> Fixture:
    """Seal changed synthetic APIs while the authored link keeps its old references."""
    store = _store(fixture.root / "current-store")
    shutil.copytree(fixture.store, store.root, dirs_exist_ok=True)
    _put(store, _resource(card_url("SYN-001"), "raw/jp.html", RAW, Kind.CARD), RAW)
    sources = fixture.sources()
    for i, ref in enumerate(fixture.refs):
        lang, document, source = sources.document(ref)
        data = object_value(object_value(document)["data"])
        transform(data, lang)
        raw = canonical(document)
        resource = replace(
            _resource(str(source.url), f"raw/api-{i}.json", raw, Kind.API),
            content_type="application/json",
            region=SourceRegion.SVWB,
        )
        _put(store, resource, raw)
    batch = seal_batch(store)
    config = object_value(parse(fixture.build.configuration.encode()))
    config["digital_link_sources"] = [{"batch_id": batch.batch_id}]
    frozen = FrozenSources(store.root, "test-store", batch.batch_id)
    refs = []
    for current in batch.inventory.current:
        descriptor = frozen.descriptor(current.source_version_id)
        if descriptor.provider != "svwb":
            continue
        _, raw, _ = frozen.read(
            current.source_version_id, parser_version="translation-svwb-v1"
        )
        details = object_value(
            object_value(object_value(parse(raw))["data"])["card_details"]
        )
        for identifier, item in details.items():
            text = object_value(object_value(item)["common"]).get("name")
            if isinstance(text, str) and text:
                refs.append(
                    SourceRef(
                        batch_id=batch.batch_id,
                        source_version_id=current.source_version_id,
                        parser="translation-svwb-v1",
                        locator=f"/data/card_details/{identifier}/common/name",
                        text_hash=digest(text.encode()),
                    )
                )
                break
    refs = sorted(
        refs, key=lambda r: canonical(r.model_dump(mode="json", round_trip=True))
    )
    config.update(configuration(tuple(refs), (("svwb", "22345678"),)))
    return replace(
        fixture, store=store.root, refs=tuple(refs), build=fixture.changed(config)
    )


def catalogue_fixture(
    fixture: Fixture,
    *,
    game: str = "svwb",
    query: str = "",
    languages: tuple[str, ...] = ("ja", "cht"),
    transform: Callable[[dict[str, JsonValue], str], None] | None = None,
) -> Fixture:
    """Reseal complete synthetic API pages, preserving the linked old batch."""
    store = _store(fixture.root / "catalogue-store")
    shutil.copytree(fixture.store, store.root, dirs_exist_ok=True)
    _put(store, _resource(card_url("SYN-001"), "raw/jp.html", RAW, Kind.CARD), RAW)
    for index, language in enumerate(languages):
        name = "Synthetic card" if language == "ja" else "合成測試名"
        if game == "sv1":
            data: dict[str, JsonValue] = {
                "errors": [],
                "cards": [
                    {
                        "card_id": 123456789,
                        "card_name": name,
                        "char_type": 1,
                        "clan": 1,
                    },
                ],
            }
            url = (
                "https://shadowverse-portal.com/api/v1/cards?format=json&lang="
                + language
            )
            region = SourceRegion.SV1
            document: dict[str, JsonValue] = {"data": data}
        else:
            data = {
                "count": 1,
                "card_details": {
                    "22345678": {
                        "common": {
                            "card_id": 22345678,
                            "name": name,
                            "class": 1,
                            "type": 1,
                        },
                        "evo": {},
                    }
                },
            }
            url = (
                "https://shadowverse-wb.com/web/CardList/cardList?"
                "include_token=1&offset=0&lang=" + language
            )
            region = SourceRegion.SVWB
            document = {"data_headers": {"result_code": 1}, "data": data}
        if transform is not None:
            transform(data, language)
        raw = canonical(document)
        resource = replace(
            _resource(url + query, f"raw/catalogue-{index}.json", raw, Kind.API),
            region=region,
            content_type="application/json",
        )
        _put(store, resource, raw)
    batch = seal_batch(store)
    config = object_value(parse(fixture.build.configuration.encode()))
    config["digital_link_sources"] = [{"batch_id": batch.batch_id}]
    recipes = object_value(config["translation_recipes"])
    recipe = object_value(parse(canonical(recipes["translation-svwb-v1"])))
    recipe.update(
        version="translation-" + game + "-v1",
        config={"provider": game},
    )
    recipes["translation-" + game + "-v1"] = recipe
    return replace(fixture, store=store.root, build=fixture.changed(config))
