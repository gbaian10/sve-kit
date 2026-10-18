"""Official family labels replay frozen English product names before writing selections."""

from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build import Json, create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import digest, object_value
from sve_carddb.core.provenance import BuildContext
from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.translations.four_layer_authored import read_inputs
from sve_carddb.domains.translations.labels import apply
from sve_carddb.domains.translations.sources import Sources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import ArchiveError, seal_batch

from ...ingest.test_source_archive import _put, _resource, _store
from ...support.build_db_fixtures import seed
from .test_translation_labels import authored

if TYPE_CHECKING:
    from sve_carddb.build import Database


@pytest.fixture
def evidence(tmp_path: Path) -> tuple[Sources, dict[str, JsonValue]]:
    raw = (
        (Path(__file__).parents[2] / "fixtures/synthetic_en/F01-card.html")
        .read_bytes()
        .replace(
            b"</body>",
            b'<div class="cardlist-Detail_Products_Inner"><p class="ttl">Synthetic collection</p></div></body>',
        )
    )
    store = _store(tmp_path / "archive")
    resource = replace(
        _resource(
            "https://en.shadowverse-evolve.com/cards/?cardno=SYN01-001EN",
            "raw/en.html",
            raw,
            Kind.CARD,
        ),
        region=SourceRegion.EN,
    )
    _put(store, resource, raw)
    batch = seal_batch(store)
    sources = Sources(
        {"test-store": store.root},
        tmp_path,
        BuildContext.from_inputs("a" * 40, {}),
    )
    record: dict[str, JsonValue] = {
        "kind": "product_family_label",
        "origin": "official",
        "data": {"public_code": "TEST", "lang": "en", "text": "Synthetic collection"},
        "source_ref": {
            "batch_id": batch.batch_id,
            "source_version_id": batch.inventory.current[0].source_version_id,
            "parser": "translation-en-v1",
            "locator": "/products/0/name",
            "text_hash": digest(b"Synthetic collection"),
        },
    }
    return sources, record


def populate(db: Database) -> None:
    seed(db)
    with db.transaction():
        db.insert(
            "language",
            {"code": "en", "display_name": "English", "fallback_order": Json([])},
        )


def test_frozen_product_label_keeps_official_provenance(
    tmp_path: Path, evidence: tuple[Sources, dict[str, JsonValue]]
) -> None:
    sources, record = evidence
    authored(tmp_path, [record])
    with create_database(
        compile_build(("t0", "translation_evidence", "translation_names"))
    ) as db:
        populate(db)
        with db.transaction():
            apply(db, read_inputs(tmp_path), sources=sources)
        translation = db.rows("translation")[0].values
        assert (
            translation["text"],
            translation["origin"],
            translation["authority"],
        ) == ("Synthetic collection", "official", "sve_official")
        assert translation["source_id"] in {use.source.id for use in sources.uses}
        assert (
            db.rows("translation_selection")[0].values["translation_id"]
            == translation["id"]
        )


@pytest.mark.parametrize(
    "mutation",
    ["text", "lang", "hash", "version", "locator", "parser", "unofficial", "missing"],
)
def test_official_label_rejects_forged_or_missing_evidence_before_writing(
    tmp_path: Path, evidence: tuple[Sources, dict[str, JsonValue]], mutation: str
) -> None:
    sources, record = evidence
    data = object_value(record["data"])
    ref = object_value(record["source_ref"])
    if mutation == "text":
        data["text"] = "Invented collection"
    elif mutation == "lang":
        data["lang"] = "ja"
    elif mutation == "hash":
        ref["text_hash"] = "sha256:" + "f" * 64
    elif mutation == "version":
        ref["source_version_id"] = "src:v1:" + "f" * 64
    elif mutation == "locator":
        ref["locator"] = "/faces/0/name"
    elif mutation == "parser":
        ref["parser"] = "translation-jp-v1"
    elif mutation == "unofficial":
        record["origin"] = "project"
    else:
        del record["source_ref"]
    authored(tmp_path, [record])
    with create_database(
        compile_build(("t0", "translation_evidence", "translation_names"))
    ) as db:
        populate(db)
        with db.transaction():
            with pytest.raises(
                (ValueError, ArchiveError),
                match=r"Invalid four-layer|exact source|hash-verified|absent from pinned batch",
            ):
                apply(db, read_inputs(tmp_path), sources=sources)
        assert db.rows("translation_use") == ()
        assert db.rows("translation_selection") == ()
