"""Small versioned, exclusively synthetic adoption receipts."""

# ruff: file-ignore[suspicious-subprocess-import,subprocess-without-shell-equals-true] -- synthetic Git histories use argument vectors and no shell

import os
import shutil
import subprocess
import unicodedata
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import BuildContext
from sve_carddb.catalog.adoption_importer import AdoptionInputs
from sve_carddb.catalog.adoption_loader import load_adoptions
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_loader import Entry

REPO = Path(__file__).resolve().parents[2]
CODE = "carddb/src/sve_carddb/routes/codec.py"


def git(root: Path, *args: str) -> str:
    executable = shutil.which("git")
    assert executable is not None
    # User settings stay outside fixtures; background repacking would race snapshot copies.
    environment = {
        key: value for key, value in os.environ.items() if not key.startswith("GIT_")
    }
    environment.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")
    return subprocess.run(
        [
            executable,
            "-c",
            "maintenance.auto=false",
            "-c",
            "gc.auto=0",
            "-C",
            str(root),
            *args,
        ],
        check=True,
        capture_output=True,
        text=True,
        env=environment,
    ).stdout.strip()


def commit(root: Path) -> str:
    git(root, "add", ".")
    if not git(root, "status", "--porcelain"):
        return git(root, "rev-parse", "HEAD")
    git(
        root,
        "-c",
        "user.name=Synthetic Reviewer",
        "-c",
        "user.email=synthetic@example.invalid",
        "commit",
        "-m",
        "synthetic fixture",
    )
    return git(root, "rev-parse", "HEAD")


def dependency(table: str, **key: str) -> dict[str, JsonValue]:
    return {"table": table, "key": dict(key)}


def record(  # ruff: ignore[too-many-arguments] -- synthetic data explicitly supplies revision and predecessor
    kind: str,
    subject: dict[str, JsonValue],
    value: JsonValue,
    review: dict[str, JsonValue],
    dependencies: list[JsonValue] | None = None,
    *,
    number: int = 1,
    previous: JsonValue = None,
) -> dict[str, JsonValue]:
    return {
        "record_key": canonical([kind, subject, number]).decode(),
        "kind": kind,
        "filing_key": "shared",
        "data": {
            "subject": subject,
            "value": value,
            "adoption_no": number,
            "predecessor": previous,
            "review_context_hash": digest(canonical(review)),
            "dependencies": sorted(dependencies or [], key=canonical),
            "reason": "Synthetic human selection.",
        },
        "evidence": [],
    }


def envelope(
    records: list[JsonValue],
    review: dict[str, JsonValue],
    *,
    entry: Entry = "catalog-adoptions",
) -> dict[str, JsonValue]:
    records = sorted(records, key=lambda r: str(object_value(r)["record_key"]))
    members: list[JsonValue] = [
        [object_value(r)["record_key"], digest(canonical(r))] for r in records
    ]
    checksum = digest(canonical(members))
    category = str(object_value(records[0])["kind"])
    policy = {
        "vocabulary_adoption": "catalog-vocabulary-v1",
        "language_adoption": "catalog-language-v1",
        "search_alias_adoption": "catalog-alias-v1",
        "text_symbol_adoption": "catalog-symbol-v1",
        "rules_name_adoption": "catalog-rules-name-v1",
        "route_override_adoption": "display-route-v1",
        "default_printing_adoption": "display-default-v1",
    }[category]
    decision: dict[str, JsonValue] = {
        "id": "d:" + checksum.removeprefix("sha256:"),
        "state": "confirmed",
        "scope": "batch",
        "category": category,
        "policy_id": policy,
        "membership_hash": checksum,
        "members": members,
        "sample_ids": [object_value(r)["record_key"] for r in records],
        "authored_by": "Synthetic author",
        "authored_at": "2026-10-01T00:00:00Z",
        "reviewed_by": "gbaian10",
        "reviewed_at": "2026-10-01T00:00:00Z",
        "reviewed_precision": "day",
        "note": "Synthetic data only.",
    }
    prefix = "catalog_adoption" if entry == "catalog-adoptions" else "display_override"
    return {
        prefix + "_format": 1,
        "kind": prefix + "_shard",
        "review_context": review,
        "default_decision_id": decision["id"],
        "records": records,
        "decisions": [decision],
    }


def write(root: Path, path: str, value: JsonValue) -> None:
    file = root / path
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_bytes(canonical(value))


def index(root: Path, *, entry: Entry = "catalog-adoptions") -> None:
    includes: dict[str, JsonValue] = {}
    for file in sorted((root / entry).rglob("*.yaml")):
        if file.name != "index.yaml":
            includes[file.relative_to(root).as_posix()] = digest(
                canonical(read_yaml(file))
            )
    prefix = "catalog_adoption" if entry == "catalog-adoptions" else "display_override"
    write(
        root,
        entry + "/index.yaml",
        {prefix + "_format": 1, "kind": prefix + "_index", "includes": includes},
    )


@dataclass(frozen=True)
class Case:
    repository: Path
    root: Path
    review: dict[str, JsonValue]
    normalizer: dict[str, JsonValue]
    revision: str

    def inputs(self) -> AdoptionInputs:
        return AdoptionInputs(
            self.root, self.repository, self.revision, ("catalog-adoptions",)
        )

    def build(self) -> BuildContext:
        return BuildContext.from_inputs(
            self.revision,
            {CODE: (self.repository / CODE).read_bytes()},
            self.inputs().configuration(),
        )


def make_case(root: Path) -> Case:
    root.mkdir()
    git(root, "init", "--quiet")
    (root / CODE).parent.mkdir(parents=True)
    (root / CODE).write_bytes((REPO / CODE).read_bytes())
    revision = commit(root)
    context = BuildContext.from_inputs(revision, {CODE: (root / CODE).read_bytes()}, {})
    review: dict[str, JsonValue] = {
        "context": context.model_dump(mode="json"),
        "source_batches": [],
    }
    config: dict[str, JsonValue] = {"unicode_version": unicodedata.unidata_version}
    normalizer: dict[str, JsonValue] = {
        "version": "nfkc-casefold-v1",
        "program_revision": revision,
        "code_path": CODE,
        "code_hash": digest((root / CODE).read_bytes()),
        "config": config,
        "config_hash": digest(canonical(config)),
    }
    authored = root / "authored"
    languages: list[JsonValue] = [
        record(
            "language_adoption",
            {"code": lang},
            {"display_name": lang, "fallback_order": list[JsonValue](fallback)},
            review,
            [dependency("language", code=target) for target in fallback],
        )
        for lang, fallback in (
            ("ja", ("en",)),
            ("en", ("ja",)),
            ("zh-Hant", ("ja", "en")),
        )
    ]
    write(
        authored,
        "catalog-adoptions/languages/shared/001.yaml",
        envelope(languages, review),
    )
    term = record(
        "vocabulary_adoption",
        {"kind": "type", "code": "follower"},
        {
            "label": {"kind": "authored", "lang": "ja", "text": "Synthetic follower"},
            "raw_mappings": [],
            "active": True,
        },
        review,
        [dependency("language", code="ja")],
    )
    write(
        authored,
        "catalog-adoptions/vocabulary/shared/001.yaml",
        envelope([term], review),
    )
    item_alias = record(
        "search_alias_adoption",
        {
            "kind": "type",
            "code": "follower",
            "lang": "ja",
            "text": "ＳＹＮＴＨＥＴＩＣ ",
        },
        {"normalized": "synthetic ", "normalizer": normalizer},
        review,
        [
            dependency("vocabulary", kind="type", code="follower"),
            dependency("language", code="ja"),
        ],
    )
    write(
        authored,
        "catalog-adoptions/aliases/shared/001.yaml",
        envelope([item_alias], review),
    )
    item_symbol = record(
        "text_symbol_adoption",
        {"id": "symbol:synthetic"},
        {
            "code": "synthetic",
            "parameter_schema": {"parameters": []},
            "keyword_id": None,
            "spellings": [
                {
                    "lang": "ja",
                    "literal_prefix": "Q",
                    "literal_suffix": "",
                    "parameter_name": None,
                    "parse_kind": "literal",
                }
            ],
            "source_localization": {
                "lang": "ja",
                "name": {"kind": "authored", "lang": "ja", "text": "Synthetic Q"},
                "tooltip": {"kind": "authored", "lang": "ja", "text": "Synthetic tip"},
                "copy_pattern": {"kind": "authored", "lang": "ja", "text": "Q"},
            },
        },
        review,
        [dependency("language", code="ja")],
    )
    write(
        authored,
        "catalog-adoptions/symbols/shared/001.yaml",
        envelope([item_symbol], review),
    )
    index(authored)
    final = commit(root)
    load_adoptions(authored, entry="catalog-adoptions")
    return Case(root, authored, review, normalizer, final)


def fields(
    shard: dict[str, JsonValue],
) -> tuple[dict[str, JsonValue], dict[str, JsonValue], dict[str, JsonValue]]:
    member = object_value(array(shard["records"])[0])
    return (
        member,
        object_value(member["data"]),
        object_value(array(shard["decisions"])[0]),
    )


def successor(
    root: Path, area: str, value: JsonValue, *, entry: Entry = "catalog-adoptions"
) -> None:
    old = object_value(read_yaml(root / f"{entry}/{area}/shared/001.yaml"))
    member, data, _ = fields(old)
    review = object_value(old["review_context"])
    previous = {
        "record_key": member["record_key"],
        "record_hash": digest(canonical(member)),
        "decision_id": old["default_decision_id"],
    }
    revised = record(
        str(member["kind"]),
        object_value(data["subject"]),
        value,
        review,
        [] if value is None else array(data["dependencies"]),
        number=2,
        previous=previous,
    )
    revised["evidence"] = [] if value is None else member["evidence"]
    write(
        root,
        f"{entry}/{area}/shared/002.yaml",
        envelope([revised], review, entry=entry),
    )
    index(root, entry=entry)
