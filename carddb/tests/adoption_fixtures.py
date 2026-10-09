"""Current catalog fixtures containing exclusively synthetic values."""

# ruff: file-ignore[suspicious-subprocess-import,subprocess-without-shell-equals-true] -- synthetic Git histories use argument vectors and no shell

import os
import shutil
import subprocess
import unicodedata
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import canonical
from sve_carddb.core.provenance import BuildContext
from sve_carddb.domains.catalog.adoption_importer import AdoptionInputs
from sve_carddb.domains.catalog.adoption_loader import load_adoptions

if TYPE_CHECKING:
    from sve_carddb.domains.catalog.adoption_loader import Entry

REPO = Path(__file__).resolve().parents[2]
CODE = "carddb/src/sve_carddb/domains/routes/codec.py"


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


def record(
    kind: str, subject: dict[str, JsonValue], value: JsonValue
) -> dict[str, JsonValue]:
    return {
        "kind": kind,
        "data": {"subject": subject, "value": value},
        "origin": "project",
        "low_confidence": False,
    }


def envelope(
    records: list[JsonValue], *, entry: Entry = "catalog/adoptions"
) -> dict[str, JsonValue]:
    prefix = "catalog_adoption" if entry == "catalog/adoptions" else "display_override"
    return {
        "format": 2,
        "kind": prefix + "_shard",
        "records": records,
    }


def write(root: Path, path: str, value: JsonValue) -> None:
    file = root / path
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_bytes(canonical(value))


@dataclass(frozen=True)
class Case:
    repository: Path
    root: Path
    review: dict[str, JsonValue]
    normalizer: dict[str, JsonValue]
    revision: str

    def inputs(self) -> AdoptionInputs:
        return AdoptionInputs(
            self.root, self.repository, self.revision, ("catalog/adoptions",)
        )

    def build(self) -> BuildContext:
        return BuildContext.from_inputs(
            self.revision,
            source_configuration(self, self.inputs().configuration()),
        )


def source_configuration(
    _case: Case, config: dict[str, JsonValue]
) -> dict[str, JsonValue]:
    """Give the build its own current recipes without touching signed historical context."""
    return config


def make_case(root: Path) -> Case:
    root.mkdir()
    git(root, "init", "--quiet")
    (root / CODE).parent.mkdir(parents=True)
    (root / CODE).write_bytes((REPO / CODE).read_bytes())
    revision = commit(root)
    context = BuildContext.from_inputs(revision, {})
    review: dict[str, JsonValue] = {
        "context": context.model_dump(mode="json", round_trip=True),
        "source_batches": [],
    }
    config: dict[str, JsonValue] = {"unicode_version": unicodedata.unidata_version}
    normalizer: dict[str, JsonValue] = {"version": "nfkc-casefold-v1", "config": config}
    authored = root / "authored"
    languages: list[JsonValue] = [
        record(
            "language_adoption",
            {"code": lang},
            {"display_name": lang, "fallback_order": list[JsonValue](fallback)},
        )
        for lang, fallback in (
            ("ja", ("en",)),
            ("en", ("ja",)),
            ("zh-Hant", ("ja", "en")),
        )
    ]
    write(
        authored,
        "catalog/adoptions/languages/001.yaml",
        envelope(languages),
    )
    term = record(
        "vocabulary_adoption",
        {"kind": "type", "code": "follower"},
        {
            "label": {"kind": "authored", "lang": "ja", "text": "Synthetic follower"},
            "raw_mappings": [],
            "active": True,
        },
    )
    write(
        authored,
        "catalog/adoptions/vocabulary/001.yaml",
        envelope([term]),
    )
    for area in ("aliases", "symbols"):
        (authored / "catalog/adoptions" / area).mkdir()
    final = commit(root)
    load_adoptions(authored, entry="catalog/adoptions")
    return Case(root, authored, review, normalizer, final)
