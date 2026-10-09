"""Synthetic sealed official HTML and Git inputs."""

import os
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- isolated test Git repository for revision pin verification
from dataclasses import dataclass, replace
from datetime import date
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.core.provenance import BuildContext
from sve_carddb.domains.products import Language, load_products
from sve_carddb.domains.products.identities import load_product_identities
from sve_carddb.domains.products.official import PARSER, parse_products
from sve_carddb.domains.products.plan import plan_official_products
from sve_carddb.domains.registry.build import build
from sve_carddb.domains.registry.inputs import Mapping as CardMapping
from sve_carddb.domains.registry.preview import plan_preview
from sve_carddb.domains.registry.preview.evidence import CardEvidence, FaceEvidence
from sve_carddb.domains.registry.review import InitDecisions, Inputs
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.registry.storage import plan_files, write_files
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import seal_batch
from sve_carddb.parse.pages.extract_jp import extract_card
from sve_carddb.parse.pages.official_jp import card_url

from .identity_evidence_fixtures import MemoryEvidence
from .product_fixtures import envelope, family, install, obj, write_yaml
from .registry_observation_fixtures import parsed_card
from .test_registry_preview_archive import RAW
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.products.identities import ProductIdentities
    from sve_carddb.domains.products.loader import ProductSnapshot
    from sve_carddb.domains.products.official import ProductPage
    from sve_carddb.domains.products.plan import OfficialProducts
    from sve_carddb.domains.registry.preview import PreviewPlan

NAME = "products/identities/jp/001.yaml"
LANGUAGES = (
    Language(code="ja", fallback_order=(), display_name="Japanese"),
    Language(code="en", fallback_order=(), display_name="English"),
)


def html(
    *,
    number: str = "TEST-001",
    name: str = "Booster Pack Synthetic",
    date: str | None = "2026-09-30",
    links: tuple[str, ...] = (
        "/products/synthetic/",
        "/cardlist/cardsearch?expansion=Test-A",
    ),
    extra: str = "",
) -> bytes:
    body = RAW.decode().replace(
        '<div class="detail">',
        f'<div class="illustrator"><span class="name">{number}</span></div><div class="detail">',
    )
    block = '<div class="cardlist-Under"><div class="cardlist-Detail_Products"><div class="cardlist-Detail_Products_Inner">'
    block += f'<p class="ttl">{name}</p>'
    if date is not None:
        block += f'<p class="date">{date}</p>'
    block += "".join(f'<a href="{link}">Synthetic link</a>' for link in links)
    block += "</div>" + extra + "</div></div>"
    return body.replace("</body>", block + "</body>").encode()


def commit(root: Path) -> str:
    executable = shutil.which("git")
    assert executable is not None
    # Synthetic repositories must not inherit a developer's hooks or signing policy.
    environment = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("GIT_CONFIG")
    }
    environment.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")
    for args in (
        ("init", "-q"),
        ("add", "authored"),
        (
            "-c",
            "user.name=Synthetic",
            "-c",
            "user.email=synthetic@example.invalid",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "Synthetic fixture",
        ),
    ):
        subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed arguments in an isolated temporary Git repository
            [executable, "-C", str(root.parent), *args],
            capture_output=True,
            check=True,
            env=environment,
        )
    return subprocess.check_output(  # ruff: ignore[subprocess-without-shell-equals-true] -- read the isolated fixture commit
        [executable, "-C", str(root.parent), "rev-parse", "HEAD"],
        text=True,
        env=environment,
    ).strip()


def identity_record(
    page: ProductPage,
    *,
    product_id: str = "permanent-example",
    match_index: int = 0,
    ordinal: int = 0,
) -> dict[str, JsonValue]:
    match = (
        page.blocks[ordinal]
        .matches[match_index]
        .model_dump(mode="json", round_trip=True)
    )
    return {
        "kind": "product_identity",
        "filing_key": page.region,
        "data": {"product_id": product_id, "region": page.region, "match": match},
        "evidence": [
            {
                "batch_id": page.source.archive.batch_id,
                "source_version_id": page.source.id,
                "locator": '{"product_block_ordinal":' + str(ordinal) + "}",
                "role": "product_identity_match",
            }
        ],
    }


def identity_envelope(records: list[dict[str, JsonValue]]) -> dict[str, JsonValue]:
    ordered = records
    return {
        "format": 1,
        "kind": "product_identity_shard",
        "records": list[JsonValue](ordered),
    }


def install_identity(
    root: Path, shard: dict[str, JsonValue], *, name: str = NAME
) -> None:
    write_yaml(root / name, shard)


@dataclass
class IdentityFixture:
    root: Path
    store: Path
    batch: str
    revision: str
    pages: tuple[ProductPage, ...]
    preview: PreviewPlan
    catalog: ProductSnapshot

    def load(self) -> ProductIdentities:
        return load_product_identities(
            self.root,
            authored_revision=self.revision,
            catalog=self.catalog,
            stores={"test-store": self.store},
        )

    def official(self) -> OfficialProducts:
        return plan_official_products(self.load(), self.pages, self.preview)

    def context(self, identities: ProductIdentities) -> BuildContext:
        return BuildContext.from_inputs(
            self.revision,
            {"product_identity": identities.configuration()},
        )


@dataclass(frozen=True)
class IdentityTemplate:
    root: Path
    store: Path
    batch: str
    revision: str
    pages: tuple[ProductPage, ...]
    preview: PreviewPlan
    catalog: ProductSnapshot

    def copy(self, destination: Path) -> IdentityFixture:
        root = destination / "checkout/authored"
        store = destination / "archive/archive"
        # Copies must not share inodes: tests corrupt both authored and sealed bytes.
        shutil.copytree(self.root.parent, root.parent)
        shutil.copytree(self.store.parent, store.parent)
        return IdentityFixture(
            root,
            store,
            self.batch,
            self.revision,
            self.pages,
            self.preview,
            self.catalog,
        )


@pytest.fixture
def identity_fixture(
    tmp_path: Path, identity_template: IdentityTemplate
) -> IdentityFixture:
    return identity_template.copy(tmp_path)


@pytest.fixture(scope="session")
def identity_template(tmp_path_factory: pytest.TempPathFactory) -> IdentityTemplate:
    tmp_path = tmp_path_factory.mktemp("identity-template")
    root = tmp_path / "checkout/authored"
    root.mkdir(parents=True)
    raw = html()
    store = _store(tmp_path / "archive")
    _put(store, _resource(card_url("TEST-001"), "raw/card.html", raw, Kind.CARD), raw)
    sealed = seal_batch(store)
    source, verified, _ = FrozenSources(
        store.root, store.store_id, sealed.batch_id
    ).read(sealed.inventory.current[0].source_version_id, parser_version=PARSER)
    page = parse_products(verified, source, "jp")
    card = parsed_card(extract_card(verified, number="TEST-001"))
    inputs = Inputs(
        jp={card.number: card},
        en={},
        mapping=CardMapping(targets={}, original_art=set(), reskins={}),
        decisions=InitDecisions(),
        as_of=date(2026, 9, 30),
        jp_hash="sha256:" + "1" * 64,
    )
    write_files(plan_files(root, build(inputs, {})))
    family_record = family("TEST")
    obj(family_record["data"])["public_code"] = "Test-A"
    install(root, "products/family/TEST/001.yaml", envelope([family_record]))
    observed = CardEvidence.from_card(
        source.model_copy(update={"parser_version": "synthetic-identity-parser-v1"}),
        "jp",
        card,
        (FaceEvidence("LG", None),),
    )
    preview = plan_preview(
        root, MemoryEvidence({("jp", card.number): observed}), regions=("jp",)
    )
    catalog = load_products(root, registry=load_registry(root))
    install_identity(root, identity_envelope([identity_record(page)]))
    return IdentityTemplate(
        root, store.root, sealed.batch_id, commit(root), (page,), preview, catalog
    )


def add_page(
    fixture: IdentityFixture,
    raw: bytes,
    *,
    region: str = "jp",
    number: str = "TEST-002",
) -> ProductPage:
    """Append a separately sealed synthetic version without touching old evidence."""
    from sve_carddb.core.regions import SourceRegion  # ruff: ignore[import-outside-top-level] -- fixture-only archive construction
    from sve_carddb.ingest.archive.source_archive import ArchiveStore  # ruff: ignore[import-outside-top-level] -- fixture-only archive construction
    from sve_carddb.parse.pages import official_en  # ruff: ignore[import-outside-top-level] -- fixture-only EN source

    data = fixture.store.parent / "data"
    store = ArchiveStore(
        data,
        data / "manifest/manifest.sqlite",
        data / "manifest/.lock",
        fixture.store,
        "test-store",
        (),
    )
    url = card_url(number) if region == "jp" else official_en.card_url(number)
    resource = replace(
        _resource(url, f"raw/{region}-{number}.html", raw, Kind.CARD),
        region=SourceRegion.JP if region == "jp" else SourceRegion.EN,
    )
    _put(store, resource, raw)
    sealed = seal_batch(store)
    version = next(
        item.source_version_id for item in sealed.inventory.current if item.url == url
    )
    source, content, _ = FrozenSources(store.root, "test-store", sealed.batch_id).read(
        version, parser_version=PARSER
    )
    return parse_products(content, source, "jp" if region == "jp" else "en")
