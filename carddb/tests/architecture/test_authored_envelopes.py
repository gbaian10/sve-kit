"""Current document envelopes reject missing metadata and former wire formats."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from pydantic import BaseModel, JsonValue, ValidationError

from sve_carddb.core.json import canonical
from sve_carddb.domains.catalog.records import Shard as CatalogShard
from sve_carddb.domains.digital.name_policies.records import LinkPolicy, Policy
from sve_carddb.domains.products.identity_models import IdentityShard
from sve_carddb.domains.products.models import Shard as ProductShard
from sve_carddb.domains.registry.storage import read_base_files, read_yaml
from sve_carddb.domains.translations.flavor import Shard as FlavorShard
from sve_carddb.domains.translations.glossary.records import Shard as GlossaryShard
from sve_carddb.domains.translations.parameters.rules import Rules
from sve_carddb.domains.translations.templates.records import Shard as TemplateShard
from sve_carddb.images.crops import CropOverrides

if TYPE_CHECKING:
    from collections.abc import Iterator

AUTHORED = Path(__file__).resolve().parents[3] / "authored"
CASES = (
    (CatalogShard, "catalog/adoptions/languages/001.yaml", "catalog_adoption_format"),
    (Policy, "digital/policies/names.yaml", "digital_name_policy_format"),
    (LinkPolicy, "digital/policies/links.yaml", "digital_name_policy_format"),
    (IdentityShard, "products/identities/jp/001.yaml", "product_identity_format"),
    (ProductShard, "products/family/BP01/001.yaml", "product_authored_format"),
    (FlavorShard, "translations/flavor/0.yaml", None),
    (
        GlossaryShard,
        "translations/glossary/concepts/001.yaml",
        "translation_authored_format",
    ),
    (
        TemplateShard,
        "translations/templates/definitions/001.yaml",
        "translation_authored_format",
    ),
    (Rules, "translations/parameter-rules/current.yaml", "parameter_rule_format"),
    (CropOverrides, "images/crops.yaml", None),
)


@pytest.fixture(params=CASES, ids=[c[1] for c in CASES])
def document(
    request: pytest.FixtureRequest,
) -> tuple[type[BaseModel], dict[str, JsonValue], str | None]:
    model, path, former = request.param
    raw = read_yaml(AUTHORED / path)
    assert isinstance(raw, dict)
    model.model_validate_json(canonical(raw))
    return model, raw, former


def invalid_headers(
    raw: dict[str, JsonValue], former: str | None
) -> Iterator[dict[str, JsonValue]]:
    for value in (0, 999, True, "1", 1.0):
        yield raw | {"format": value}
    yield raw | {"kind": "unknown"}
    for field in ("kind", "format"):
        yield {k: v for k, v in raw.items() if k != field}
    yield raw | {"version": "former/1"}
    if former is not None:
        yield {k: v for k, v in raw.items() if k != "format"} | {former: raw["format"]}
        yield raw | {former: raw["format"]}


def test_current_envelope_is_exclusive(
    document: tuple[type[BaseModel], dict[str, JsonValue], str | None],
) -> None:
    model, raw, former = document
    for value in invalid_headers(raw, former):
        with pytest.raises((ValidationError, ValueError)):
            model.model_validate_json(json.dumps(value))


@pytest.mark.parametrize("shard", [False, True], ids=["index", "shard"])
def test_registry_wire_metadata_is_required(tmp_path: Path, *, shard: bool) -> None:
    index: dict[str, JsonValue] = {
        "format": 2,
        "kind": "registry_index",
        "allocation_policy": "region-ranges-2026-09-28-v1",
        "next_int_id": {"en": 60001, "jp": 20001},
    }
    path = tmp_path / "ids/index.yaml"
    path.parent.mkdir()
    path.write_bytes(canonical(index))
    if shard:
        path = tmp_path / "registry/card/test/001.yaml"
        path.parent.mkdir(parents=True)
        raw: dict[str, JsonValue] = {
            "format": 1,
            "kind": "registry_shard",
            "records": [],
        }
    else:
        raw = index
    path.write_bytes(canonical(raw))
    read_base_files(tmp_path)
    for value in invalid_headers(raw, "authored_format"):
        path.write_bytes(json.dumps(value).encode())
        with pytest.raises(ValueError, match=r"envelope|kind|Floating"):
            read_base_files(tmp_path)
