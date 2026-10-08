"""One immutable bilingual synthetic archive; each test edits only copied receipts."""

import shutil
from dataclasses import dataclass, replace

import pytest
from pydantic import JsonValue

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.provenance import BuildContext
from sve_carddb.core.regions import SourceRegion
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import seal_batch

from .adoption_fixtures import REPO, commit, index, make_case
from .catalog_vocabulary_fixtures import RUNTIME, VocabularyCase
from .test_source_archive import _put, _resource, _store


@dataclass(frozen=True)
class TraitCase:
    vocabulary: VocabularyCase
    versions: dict[str, str]
    batch_id: str
    faces: dict[str, list[dict[str, JsonValue]]]

    def mapping(
        self, region: str = "jp", face: int = 0, component: int = 1
    ) -> dict[str, JsonValue]:
        traits = self.faces[region][face]["traits"]
        assert isinstance(traits, list)
        raw = traits[component]
        assert isinstance(raw, str)
        return {
            "region": region,
            "lang": "ja" if region == "jp" else "en",
            "raw": raw,
            "source_ref": {
                "batch_id": self.batch_id,
                "source_version_id": self.versions[region],
                "parser": "exact-json-v1",
                "locator": f"/faces/{face}/traits/{component}",
                "text_hash": digest(raw.encode()),
            },
            "special_kinds": [],
        }


def make_face(raw: JsonValue, traits: list[JsonValue]) -> dict[str, JsonValue]:
    return {
        "trait_raw": raw,
        "traits": traits,
        "title": "Synthetic title",
        "card_class": "Synthetic class",
        "info": {"Universe": "Synthetic title", "Class": "Synthetic class"},
    }


@pytest.fixture(scope="module")
def trait_baseline(tmp_path_factory: pytest.TempPathFactory) -> TraitCase:
    root = tmp_path_factory.mktemp("trait-adoption")
    store = _store(root / "archive")
    faces = {
        "jp": [
            make_face(
                "Synthetic・〈Synthetic・Compound〉",
                ["Synthetic", "〈Synthetic・Compound〉"],
            ),
            make_face("Synthetic・Other", ["Synthetic", "Wrong"]),
            make_face(
                "Synthetic・〈Synthetic・Compound",
                ["Synthetic", "〈Synthetic", "Compound"],
            ),
            make_face("Synthetic・Compound〉", ["Synthetic", "Compound〉"]),
            make_face("Prefix〈Synthetic〉", ["Prefix〈Synthetic〉"]),
            make_face("〈Synthetic〉Suffix", ["〈Synthetic〉Suffix"]),
            make_face("〈〈Synthetic〉〉", ["〈〈Synthetic〉〉"]),
            make_face("〈Synthetic〉〈Other〉", ["〈Synthetic〉〈Other〉"]),
            make_face("〉Synthetic〈", ["〉Synthetic〈"]),
            make_face("Synthetic・Other", ["Synthetic・Other"]),
            make_face("Synthetic・・Other", ["Synthetic", "", "Other"]),
            make_face("Synthetic・7", ["Synthetic", 7]),
            make_face(None, ["Synthetic"]),
            make_face("〈Synthetic〈〉", ["〈Synthetic〈〉"]),
            make_face("〈Synthetic〉〉", ["〈Synthetic〉〉"]),
            make_face("Synthetic・Other", ["Synthetic", "", "Other"]),
        ],
        "en": [
            make_face("Synthetic's Nest / Other", ["Synthetic's Nest", "Other"]),
            make_face("Synthetic’s Nest / Other", ["Synthetic’s Nest", "Other"]),
            make_face("Synthetic A / Synthetic B", ["Synthetic A / Synthetic B"]),
            make_face("Synthetic・Other", ["Synthetic", "Other"]),
            make_face("〈Synthetic / Other〉", ["〈Synthetic / Other〉"]),
        ],
    }
    for region, parts in faces.items():
        raw = canonical({"faces": list[JsonValue](parts)})
        resource = replace(
            _resource(
                "https://example.invalid/" + region,
                "raw/" + region + ".json",
                raw,
                Kind.CARD,
            ),
            region=SourceRegion(region),
            content_type="application/json",
        )
        _put(store, resource, raw)
    batch = seal_batch(store)
    versions = {
        entry.url.rsplit("/", 1)[1]: entry.source_version_id
        for entry in batch.inventory.current
    }
    case = make_case(root / "repository")
    for name in RUNTIME:
        target = case.repository / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((REPO / name).read_bytes())
    revision = commit(case.repository)
    context = BuildContext.from_inputs(
        revision,
        {
            "catalog_source_recipes": {
                "exact-json-v1": {"version": "exact-json-v1", "config": {}}
            }
        },
    )
    review: dict[str, JsonValue] = {
        "context": context.model_dump(mode="json"),
        "source_batches": [{"batch_id": batch.batch_id}],
    }
    shutil.rmtree(case.root / "catalog-adoptions/aliases")
    shutil.rmtree(case.root / "catalog-adoptions/symbols")
    index(case.root)
    return TraitCase(
        VocabularyCase(replace(case, review=review), store.root, {}),
        versions,
        batch.batch_id,
        faces,
    )
