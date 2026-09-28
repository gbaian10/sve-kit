"""Measure synthetic text_unit storage; run manually, never as a timing assertion."""

import sqlite3
import statistics
import sys
import time
from dataclasses import replace
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import TYPE_CHECKING

from sve_carddb.build_db import Json, Value, compile_schema, create_database
from sve_carddb.build_db.t0 import REGISTRY
from sve_carddb.build_db.t0_json import schemas
from sve_carddb.snapshot.values import digest

if TYPE_CHECKING:
    from sve_carddb.build_db import CompiledSchema


def _rows(count: int, text_bytes: int) -> list[dict[str, Value]]:
    result: list[dict[str, Value]] = []
    for index in range(count):
        text = f"Synthetic {index:08d} ".ljust(text_bytes, "x")
        content_hash = digest(text.encode())
        result.append(
            {
                "id": "t:ja:" + content_hash[7:23],
                "lang": "ja",
                "text": text,
                "content_hash": content_hash,
            }
        )
    return result


def _run(schema: CompiledSchema, rows: list[dict[str, Value]], path: Path) -> float:
    with create_database(schema, path) as db:
        start = time.perf_counter()
        with db.transaction():
            db.insert(
                "language",
                {"code": "ja", "fallback_order": Json([]), "display_name": "Synthetic"},
            )
            for row in rows:
                db.insert("text_unit", row)
        return time.perf_counter() - start


def measure(count: int = 10000, repeats: int = 3) -> None:
    sys.stdout.write(
        f"SQLite {sqlite3.sqlite_version}; rows={count}; repetitions={repeats}\n"
    )
    sys.stdout.write("text_bytes,without_rowid,median_seconds,database_bytes\n")
    with TemporaryDirectory(prefix="sve-build-db-benchmark-") as directory:
        for size in (128, 1024, 4096, 16384):
            rows = _rows(count, size)
            for without_rowid in (True, False):
                registry = replace(
                    REGISTRY,
                    tables=tuple(
                        replace(table, without_rowid=without_rowid)
                        if table.name == "text_unit"
                        else table
                        for table in REGISTRY.tables
                    ),
                )
                schema = compile_schema(registry, ("t0",), schemas())
                elapsed: list[float] = []
                database_bytes = 0
                for repetition in range(repeats):
                    path = (
                        Path(directory) / f"{size}-{without_rowid}-{repetition}.sqlite"
                    )
                    elapsed.append(_run(schema, rows, path))
                    database_bytes = path.stat().st_size
                    path.unlink()
                sys.stdout.write(
                    f"{size},{without_rowid},{statistics.median(elapsed):.4f},{database_bytes}\n"
                )


if __name__ == "__main__":
    measure()
