"""Current composer checks real extractors over invented sealed JP/EN pages."""

import copy
import shutil
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import array, canonical, digest, object_value
from sve_carddb.core.provenance import BuildContext
from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.registry.storage import read_yaml
from sve_carddb.domains.translations.sources import Sources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import seal_batch
from sve_carddb.parse.pages import official_en, official_jp
from sve_carddb.workflows.offline import _populate_adoptions, _prepare_catalog

from ...ingest.test_source_archive import _put, _resource, _store
from ...support.adoption_fixtures import REPO, Case, commit, make_case, write
from ...support.en_extract_fixtures import page as english_page
from ..registry.test_registry_preview_archive import RAW as JAPANESE_PAGE

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import CompiledSchema, Database
    from sve_carddb.core.provenance import InputRecord

PARSERS = {
    "jp": "carddb/src/sve_carddb/parse/pages/extract_jp.py",
    "en": "carddb/src/sve_carddb/parse/pages/extract_en.py",
}
RUNTIME = (
    "carddb/uv.lock",
    "carddb/pyproject.toml",
    "carddb/src/sve_carddb/domains/catalog/adoption_sources.py",
    "carddb/src/sve_carddb/parse/html.py",
    "carddb/src/sve_carddb/ingest/http/validate.py",
    "carddb/src/sve_carddb/parse/pages/official_jp.py",
    "carddb/src/sve_carddb/parse/pages/official_en.py",
    "carddb/src/sve_carddb/parse/pages/extract_jp.py",
    "carddb/src/sve_carddb/parse/pages/extract_en.py",
    "carddb/src/sve_carddb/domains/registry/projection.py",
    "carddb/src/sve_carddb/domains/registry/inputs.py",
    "carddb/src/sve_carddb/domains/registry/review.py",
    "carddb/src/sve_carddb/core/json.py",
)
VOCABULARY = "catalog/adoptions/vocabulary/001.yaml"
IMAGE_RAW = b"Invented image evidence bytes"
IMAGE_URL = "https://shadowverse-evolve.com/synthetic/image.png"


@dataclass(frozen=True)
class PageCase:
    case: Case
    archive: Path
    references: dict[str, dict[str, JsonValue]]
    image: dict[str, JsonValue]

    def build(self) -> BuildContext:
        config = self.case.inputs().configuration()
        config["catalog_source_recipes"] = {
            "official-" + region + "-exact-v1": {
                "version": "official-" + region + "-exact-v1",
                "config": {},
            }
            for region in PARSERS
        }
        return BuildContext.from_inputs(
            self.case.revision,
            config,
        )


def save(case: PageCase, path: str, payload: JsonValue) -> PageCase:
    write(case.case.root, path, payload)
    return replace(case, case=replace(case.case, revision=commit(case.case.repository)))


def vocabulary(case: PageCase, region: str) -> dict[str, JsonValue]:
    payload = object_value(read_yaml(case.case.root / VOCABULARY))
    return next(
        object_value(item)
        for item in array(payload["records"])
        if object_value(object_value(object_value(item)["data"])["subject"])["code"]
        == "synthetic_" + region
    )


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> PageCase:
    root = tmp_path_factory.mktemp("current-html-recipes")
    store = _store(root / "sources")
    urls: dict[str, str] = {}
    names = {"jp": "Synthetic card", "en": "Synthetic front"}
    for region, provider in (("jp", official_jp), ("en", official_en)):
        raw = JAPANESE_PAGE if region == "jp" else english_page("SYN-EN")
        url = provider.card_url("SYN-JP" if region == "jp" else "SYN-EN")
        for variant, suffix in (
            ("known", url),
            ("missing", url.split("?")[0]),
            ("duplicate", url + "&cardno=SECOND"),
        ):
            urls[region + ":" + variant] = suffix
            resource = replace(
                _resource(suffix, f"raw/{region}-{variant}.html", raw, Kind.CARD),
                region=SourceRegion.JP if region == "jp" else SourceRegion.EN,
            )
            _put(store, resource, raw)
    _put(store, _resource(IMAGE_URL, "raw/image.png", IMAGE_RAW), IMAGE_RAW)
    sealed = seal_batch(store)
    versions = {item.url: item.source_version_id for item in sealed.inventory.current}
    references: dict[str, dict[str, JsonValue]] = {
        key: {
            "batch_id": sealed.batch_id,
            "source_version_id": versions[url],
            "parser": "official-" + key.split(":")[0] + "-exact-v1",
            "locator": "/faces/0/name",
            "text_hash": digest(names[key.split(":")[0]].encode()),
        }
        for key, url in urls.items()
    }
    case = make_case(root / "repository")
    for path in RUNTIME:
        target = case.repository / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((REPO / path).read_bytes())
    current = PageCase(
        case,
        store.root,
        references,
        {
            "batch_id": sealed.batch_id,
            "source_version_id": versions[IMAGE_URL],
            "raw_hash": digest(IMAGE_RAW),
            "printing_id": "synthetic-printing",
            "face_id": "synthetic-face",
        },
    )
    rows: list[JsonValue] = [
        {
            "kind": "vocabulary_adoption",
            "data": {
                "subject": {"kind": "type", "code": "synthetic_" + region},
                "value": {
                    "label": {
                        "kind": "source",
                        "source_ref": references[region + ":known"],
                    },
                    "raw_mappings": [],
                    "active": True,
                },
                "evidence": [
                    {
                        "source_ref": references[region + ":known"],
                        "role": "Synthetic page label",
                    }
                ],
            },
            "origin": "project",
            "low_confidence": False,
            "note": "Invented current page recipe",
        }
        for region in ("en", "jp")
    ]
    return save(
        current,
        VOCABULARY,
        {
            "format": 2,
            "kind": "catalog_adoption_shard",
            "records": rows,
        },
    )


@pytest.fixture
def case(tmp_path: Path, baseline: PageCase) -> PageCase:
    repository = tmp_path / "repository"
    shutil.copytree(baseline.case.repository, repository)
    return replace(
        baseline,
        case=replace(
            baseline.case, repository=repository, root=repository / "authored"
        ),
    )


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_build()


def populate(
    db: Database, case: PageCase, build: BuildContext | None = None
) -> InputRecord:
    with db.transaction():
        return _populate_adoptions(
            db,
            case.case.inputs(),
            build=case.build() if build is None else build,
            stores={"test-store": case.archive},
            prepared=_prepare_catalog(
                case.case.inputs(),
                case.build() if build is None else build,
                {"test-store": case.archive},
            ),
            sources=Sources(
                {"test-store": case.archive},
                case.case.inputs().repository,
                case.build() if build is None else build,
            ),
        )


def test_current_composer_replays_both_page_recipes(
    baseline: PageCase, schema: CompiledSchema
) -> None:
    with create_database(schema) as db:
        result = populate(db, baseline)
        assert {row.values["text"] for row in db.rows("text_unit")} == {
            "Synthetic card",
            "Synthetic front",
        }
        assert {(use.source.url, use.source.parser_version) for use in result.uses} == {
            (official_jp.card_url("SYN-JP"), "official-jp-exact-v1"),
            (official_en.card_url("SYN-EN"), "official-en-exact-v1"),
        }
        assert len(db.rows("vocabulary")) == 2
        assert not db.rows("decision")


@pytest.mark.parametrize("region", ["jp", "en"])
@pytest.mark.parametrize("variant", ["missing", "duplicate"])
def test_page_recipe_needs_one_exact_card_number(
    case: PageCase, schema: CompiledSchema, region: str, variant: str
) -> None:
    payload = object_value(read_yaml(case.case.root / VOCABULARY))
    row = vocabulary(case, region)
    data = object_value(row["data"])
    ref = case.references[region + ":" + variant]
    object_value(object_value(data["value"])["label"])["source_ref"] = ref
    data["evidence"] = [{"source_ref": ref, "role": "Synthetic label"}]
    payload["records"] = [row]
    case = save(case, VOCABULARY, payload)
    with create_database(schema) as db:
        with pytest.raises(
            ValueError, match=r"^Card source lacks exact official number$"
        ):
            populate(db, case)
        assert not db.rows("source_record")


@pytest.mark.parametrize("mutation", ["none", "raw_hash", "kind"])
def test_current_image_evidence_is_checked(
    case: PageCase, schema: CompiledSchema, mutation: str
) -> None:
    payload = object_value(read_yaml(case.case.root / VOCABULARY))
    row = vocabulary(case, "en")
    ref = copy.deepcopy(case.image)
    if mutation == "raw_hash":
        ref["raw_hash"] = "sha256:" + "e" * 64
    elif mutation == "kind":
        ref["source_version_id"] = case.references["en:known"]["source_version_id"]
        ref["raw_hash"] = digest(english_page("SYN-EN"))
    data = object_value(row["data"])
    data["evidence"] = sorted(
        [*array(data["evidence"]), {"image_ref": ref, "role": "Synthetic image"}],
        key=canonical,
    )
    payload["records"] = [row]
    case = save(case, VOCABULARY, payload)
    with create_database(schema) as db:
        if mutation != "none":
            with pytest.raises(
                ValueError, match=r"^Adoption image descriptor/raw hash mismatch$"
            ):
                populate(db, case)
            assert not db.rows("source_record")
            return
        result = populate(db, case)
        images = [use for use in result.uses if use.usage == "catalog_image_evidence"]
        assert len(images) == 1
        assert images[0].source.kind == "image"
        assert images[0].source.sha256 == case.image["raw_hash"]
        assert (
            len([r for r in db.rows("source_record") if r.values["kind"] != "authored"])
            == 2
        )
