"""Inspect persisted request results independently of the writer's result objects."""

from sve_carddb.ingest.archive.manifest import Manifest, Outcome


def request_outcomes(manifest: Manifest, url: str) -> list[Outcome]:
    rows = manifest._conn.execute(
        "SELECT outcome FROM fetch_log WHERE url = ? ORDER BY id", (url,)
    ).fetchall()
    return [Outcome(row[0]) for row in rows]
