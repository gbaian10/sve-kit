"""Synthetic product catalogs and isolated edit helpers."""

import io
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.domains.products import Language

from .fixture_files import FrozenFiles, freeze_files, restore_files
from .yaml_fixtures import yaml_emitter

if TYPE_CHECKING:
    from pathlib import Path

Object = dict[str, JsonValue]
LANGUAGES = (Language(code="ja", fallback_order=(), display_name="Japanese"),)


def obj(value: JsonValue) -> Object:
    assert isinstance(value, dict)
    return value


def items(value: JsonValue) -> list[JsonValue]:
    assert isinstance(value, list)
    return value


def first_record(shard: Object) -> Object:
    return obj(items(shard["records"])[0])


def family(
    identifier: str, *, code: str | None = None, text: str = "Synthetic family"
) -> Object:
    return {
        "kind": "product_family",
        "filing_key": identifier,
        "state": "confirmed",
        "data": {
            "id": identifier,
            "code": identifier.lower() if code is None else code,
            "public_code": identifier,
            "kind": "booster",
            "name": {"lang": "ja", "text": text},
        },
        "evidence": [],
    }


def reference() -> Object:
    return {
        "batch_id": "sha256:" + "1" * 64,
        "source_version_id": "src:v1:" + "2" * 64,
        "locator": "product block 0",
        "role": "catalog",
    }


def product(*, region: str = "jp") -> Object:
    return {
        "kind": "product",
        "filing_key": "unassigned",
        "state": "confirmed",
        "data": {
            "id": "example",
            "region": region,
            "family_id": "BP02",
            "product_code": None,
            "name": {"lang": "ja", "text": "Synthetic product"},
            "product_type": "pack",
            "released_on": None,
            "date_precision": "unknown",
            "date_raw": None,
        },
        "evidence": [reference()],
    }


def inclusion(printing_id: str) -> Object:
    return {
        "kind": "printing_product",
        "filing_key": "unassigned",
        "state": "confirmed",
        "data": {
            "printing_id": printing_id,
            "product_id": "example",
            "first_available_on": None,
            "first_available_precision": None,
            "first_available_raw": None,
            "inclusion_kind": "pack",
            "note": None,
        },
        "evidence": [reference()],
    }


def envelope(records: list[Object]) -> Object:
    return {
        "product_authored_format": 1,
        "kind": "product_shard",
        "records": list[JsonValue](records),
    }


def write_yaml(path: Path, value: JsonValue) -> None:
    stream = io.StringIO()
    yaml_emitter().dump(value, stream)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(stream.getvalue(), encoding="utf-8")


def install(root: Path, name: str, shard: Object) -> None:
    write_yaml(root / name, shard)


@pytest.fixture(scope="session")
def product_files(tmp_path_factory: pytest.TempPathFactory) -> FrozenFiles:
    registry_root = tmp_path_factory.mktemp("product-template")
    for identifier in ("BP02", "PR", "GF01"):
        install(
            registry_root,
            f"products/family/{identifier}/001.yaml",
            envelope([family(identifier)]),
        )
    return freeze_files(registry_root)


@pytest.fixture
def product_root(registry_root: Path, product_files: FrozenFiles) -> Path:
    restore_files(product_files, registry_root)
    return registry_root
