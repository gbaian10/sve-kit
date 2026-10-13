"""Completed stages remain readable before the export finishes."""

import csv
from typing import TYPE_CHECKING

from sve_carddb.core.stage_log import mark

if TYPE_CHECKING:
    from pathlib import Path

    import pytest


def test_stage_log_creates_header_and_appends_completed_stages(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    table = tmp_path / "stages.tsv"
    monkeypatch.setenv("SVE_STAGE_LOG", str(table))
    mark("verified inputs")
    with table.open(encoding="utf-8") as stream:
        first = list(csv.DictReader(stream, delimiter="\t"))
    assert [row["stage"] for row in first] == ["verified inputs"]

    mark("built text")
    with table.open(encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream, delimiter="\t"))
    assert rows[0] == first[0]
    assert [row["stage"] for row in rows] == ["verified inputs", "built text"]
    for row in rows:
        assert row["utc"].endswith("Z")
        assert float(row["wall_s"]) >= 0
        assert float(row["cpu_s"]) >= 0
        assert float(row["rss_gib"]) > 0
        assert float(row["peak_gib"]) > 0
    output = capsys.readouterr()
    assert "verified inputs" in output.err
    assert "built text" in output.err
    assert not output.out
