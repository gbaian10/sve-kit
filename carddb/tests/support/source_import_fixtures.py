"""Tiny source sets and one immutable synthetic program repository per module."""

import hashlib
import json
import os
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- Only an isolated synthetic Git repository is initialized.
from dataclasses import dataclass
from pathlib import Path

from sve_carddb.core.json import digest
from sve_carddb.ingest.archive.source_import.importer import PROGRAM_FILES

REPO = Path(__file__).resolve().parents[3]
HTML = b"<!doctype html><html><head><title>Sample bulletin</title></head><body><p>Synthetic rules example.</p></body></html>"
PDF = b"%PDF-1.7\nSynthetic PDF container, not official text.\n%%EOF\n"
URLS = (
    "https://shadowverse-evolve.com/card_limit/",
    "https://shadowverse-evolve.com/news/post-900001/",
    "https://shadowverse-evolve.com/rules/synthetic.pdf",
)


@dataclass(frozen=True)
class Inputs:
    input_dir: Path
    selection: Path
    program: Path


def git(root: Path, *args: str) -> str:
    env = os.environ | {
        "GIT_AUTHOR_NAME": "Synthetic author",
        "GIT_AUTHOR_EMAIL": "author@example.invalid",
        "GIT_COMMITTER_NAME": "Synthetic author",
        "GIT_COMMITTER_EMAIL": "author@example.invalid",
    }
    return subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- Fixed local Git plumbing, with no shell.
        [shutil.which("git") or "/usr/bin/git", "-C", str(root), *args],
        check=True,
        capture_output=True,
        text=True,
        env=env,
    ).stdout.strip()


def create_seed(root: Path) -> Inputs:
    program = root / "program"
    program.mkdir()
    for name in PROGRAM_FILES:
        target = program / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((REPO / name).read_bytes())
    git(program, "init", "-q")
    git(program, "add", ".")
    git(program, "commit", "-qm", "Synthetic program")
    revision = git(program, "rev-parse", "HEAD")
    input_dir = root / "acquired"
    (input_dir / "raw").mkdir(parents=True)
    rows = []
    for url, raw in zip(URLS, (HTML, HTML, PDF), strict=True):
        content_hash = hashlib.sha256(raw).hexdigest()
        (input_dir / "raw" / content_hash).write_bytes(raw)
        rows.append(
            {
                "url": url,
                "final_url": url,
                "status": 200,
                "sha256": content_hash,
                "bytes": len(raw),
                "fetched_at": "2020-01-02T03:04:05+00:00",
                "content_type": "application/pdf"
                if raw == PDF
                else "text/html; charset=UTF-8",
                "etag": None,
                "last_modified": None,
                "chain": [{"url": url, "status": 200, "location": None}],
            }
        )
    (input_dir / "index.jsonl").write_text(
        "\n".join(json.dumps(row) for row in rows) + "\n"
    )
    selection = root / "selection.json"
    selection.write_text(
        json.dumps(
            {
                "source_import_format": 1,
                "sources": [
                    {"url": URLS[0], "provider": "jp", "kind": "limit"},
                    {"url": URLS[1], "provider": "jp", "kind": "news"},
                    {"url": URLS[2], "provider": "en", "kind": "rules"},
                ],
                "program_revision": revision,
                "dependencies": [
                    {"name": name, "sha256": digest((REPO / name).read_bytes())}
                    for name in sorted(PROGRAM_FILES)
                ],
            }
        )
    )
    return Inputs(input_dir, selection, program)


def copy_inputs(seed: Inputs, root: Path) -> Inputs:
    target = root / "acquired"
    shutil.copytree(seed.input_dir, target)
    selection = root / "selection.json"
    shutil.copyfile(seed.selection, selection)
    return Inputs(target, selection, seed.program)


def index_rows(inputs: Inputs) -> list[dict[str, object]]:
    return [
        json.loads(line)
        for line in (inputs.input_dir / "index.jsonl").read_text().splitlines()
    ]


def write_index(inputs: Inputs, rows: list[dict[str, object]]) -> None:
    (inputs.input_dir / "index.jsonl").write_text(
        "\n".join(json.dumps(row) for row in rows) + "\n"
    )
