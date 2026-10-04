"""Independently signed synthetic product catalogs and isolated edit helpers."""

import hashlib
import io
import json
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.products import Language
from sve_carddb.registry.storage import read_yaml, yaml_parser

from .fixture_files import FrozenFiles, freeze_files, restore_files

if TYPE_CHECKING:
    from pathlib import Path

Object = dict[str, JsonValue]
LANGUAGES = (Language(code="ja", fallback_order=(), display_name="Japanese"),)


def checksum(value: JsonValue) -> str:
    return (
        "sha256:"
        + hashlib.sha256(
            json.dumps(
                value, sort_keys=True, ensure_ascii=False, separators=(",", ":")
            ).encode()
        ).hexdigest()
    )


def obj(value: JsonValue) -> Object:
    assert isinstance(value, dict)
    return value


def items(value: JsonValue) -> list[JsonValue]:
    assert isinstance(value, list)
    return value


def first_record(shard: Object) -> Object:
    return obj(items(shard["records"])[0])


def decision(shard: Object) -> Object:
    return obj(items(shard["decisions"])[0])


def family(
    identifier: str, *, code: str | None = None, text: str = "Synthetic family"
) -> Object:
    return {
        "kind": "product_family",
        "filing_key": identifier,
        "record_key": json.dumps(["product_family", identifier], separators=(",", ":")),
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
        "record_key": '["product","example"]',
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
        "record_key": json.dumps(
            ["printing_product", printing_id, "example"], separators=(",", ":")
        ),
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
    shard: Object = {
        "product_authored_format": 1,
        "kind": "product_shard",
        "default_decision_id": "",
        "records": list[JsonValue](records),
        "decisions": [
            {
                "id": "",
                "state": "confirmed",
                "scope": "batch",
                "category": "product_catalog",
                "policy_id": "product-authored-v1",
                "membership_hash": "",
                "members": [],
                "sample_ids": [],
                "authored_by": "synthetic-author",
                "authored_at": "2026-09-30T12:34:56Z",
                "reviewed_by": "synthetic-reviewer",
                "reviewed_at": "2026-09-30T00:00:00Z",
                "reviewed_precision": "day",
                "note": "Synthetic review note",
            }
        ],
    }
    sign(shard)
    return shard


def sign(shard: Object) -> None:
    review = decision(shard)
    members: list[JsonValue] = sorted(
        [
            [obj(record)["record_key"], checksum(record)]
            for record in items(shard["records"])
        ],
        key=lambda item: str(items(item)[0]),
    )
    review["members"] = members
    review["membership_hash"] = checksum(members)
    review["id"] = "d:" + str(review["membership_hash"]).removeprefix("sha256:")
    review["sample_ids"] = (
        [items(item)[0] for item in members] if review["state"] == "confirmed" else []
    )
    shard["default_decision_id"] = review["id"]


def write_yaml(path: Path, value: JsonValue) -> None:
    stream = io.StringIO()
    yaml_parser().dump(value, stream)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(stream.getvalue(), encoding="utf-8")


def install(root: Path, name: str, shard: Object, *, resign: bool = False) -> None:
    if resign:
        sign(shard)
    write_yaml(root / name, shard)
    index_path = root / "products/index.yaml"
    index: Object = (
        obj(read_yaml(index_path))
        if index_path.exists()
        else {
            "product_authored_format": 1,
            "kind": "product_index",
            "includes": {},
        }
    )
    obj(index["includes"])[name] = checksum(shard)
    write_yaml(index_path, index)


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
