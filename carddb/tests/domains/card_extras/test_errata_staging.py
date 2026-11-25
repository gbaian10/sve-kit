"""Exercise original byte ranges and the offline staging entry point."""

import json
from dataclasses import replace
from typing import TYPE_CHECKING, cast

import httpx
import pytest
from typer.testing import CliRunner

from sve_carddb.core.json import digest
from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.card_extras.errata_parser import PARSER, parse_notice
from sve_carddb.domains.parsed_cards.commands import app
from sve_carddb.ingest.archive.manifest import Kind, Manifest
from sve_carddb.ingest.archive.source_archive import seal_batch

from ...ingest.test_source_archive import _put, _resource, _store
from ...support.card_extras_fixtures import source

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.provenance import Source
    from sve_carddb.core.regions import Region

_JP_PAIR = """<p>▼修正内容</p><p>（誤）<br />合成　旧値</p>
<div><p>↓ <br />（正）<br />合成　<strong>新値</strong>&#x21;<img alt="記号" /></p></div>
<p>&nbsp;</p><p>▼修正理由</p><p>合成の説明。</p>"""
_EN_PAIR = """<section class=inh1>Changes</section>
<p>(Incorrect)<br>Old synthetic <img src="/icon.png?v=1" alt="sign">&amp; text</p>
<p>↓<br>(Correct)<br>New synthetic text</p>"""


def _page(region: Region, content: str, date: str | None) -> tuple[bytes, Source]:
    prefix = "eratta" if region == "jp" else "errata"
    number, name = (
        ("カード番号", "カード名")
        if region == "jp"
        else ("Card Number(s)", "Card Name")
    )
    time = "" if date is None else f'<time class="time Sans">{date}</time>'
    raw = f"""<html><body><div class=st-Container><div class=st-Container_Inner><div class=sw-Lower>
    <div class=sw-Lower_Wrapper><div class=sw-Lower_Container><div class={prefix}-Detail>
    <div class={prefix}-Detail_Inner>
    <div class=heading><h1 class=ttl>Synthetic</h1><div class=heading-top>{time}</div></div>
    <div class=sw-Info><dl><dt>{number}</dt><dd>TEST-001、TEST-002</dd></dl>
    <dl><dt>{name}</dt><dd>Synthetic</dd></dl></div>
    <div class='contents sw-Txtarea'>{content}</div>
    </div></div></div></div></div></div></div></body></html>""".encode()
    host = "shadowverse-evolve.com" if region == "jp" else "en.shadowverse-evolve.com"
    return raw, source(raw=raw, region=region).model_copy(
        update={"url": f"https://{host}/errata/synthetic/", "parser_version": PARSER}
    )


def test_stage_sealed_notice_and_report_exact_utf8_ranges(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw, pin = _page("jp", _JP_PAIR, "2026.10.03")
    store = _store(tmp_path)
    _put(store, _resource(pin.url, "raw/errata.html", raw, Kind.ERRATA), raw)
    batch = seal_batch(store)
    version = batch.inventory.current[0].source_version_id

    def forbidden(*_args: object, **_kwargs: object) -> None:
        pytest.fail("Staging accessed a live input")

    monkeypatch.setattr(Manifest, "open", forbidden)
    monkeypatch.setattr(Manifest, "open_live", forbidden)
    monkeypatch.setattr(httpx.Client, "send", forbidden)
    output = tmp_path / "stage.json"
    args = [
        "stage-errata",
        str(store.root),
        store.store_id,
        batch.batch_id,
        version,
        "jp",
        str(output),
    ]
    runner = CliRunner()
    result = runner.invoke(app, args)
    assert result.exit_code == 0
    staged_bytes = output.read_bytes()
    report = json.loads(staged_bytes)
    assert report["source"]["id"] == version
    assert report["source"]["sha256"] == digest(raw)
    assert "store_id" not in report["source"]
    assert report["announced_on"] == "2026-10-03"
    assert report["listed_card_numbers"] == ["TEST-001", "TEST-002"]
    assert report["issues"] == []
    assert report["end_bases"] == ["nbsp_separator"]
    assert report["outside_text"][-2:] == ["▼修正理由", "合成の説明。"]
    (block,) = report["blocks"]
    assert block["context"] == ["▼修正内容"]
    assert (block["before"], block["after"]) == ("合成　旧値", "合成　新値!{記号}")
    for span, expected in (
        (block["raw_before"], "合成　旧値"),
        (block["raw_after"], '合成　<strong>新値</strong>&#x21;<img alt="記号" />'),
    ):
        assert raw[span["start"] : span["end"]] == expected.encode()
        assert span["sha256"] == digest(expected.encode())

    assert runner.invoke(app, args).exit_code == 1
    assert output.read_bytes() == staged_bytes
    result = runner.invoke(app, [*args[:-2], "en", str(tmp_path / "wrong-region.json")])
    assert result.exit_code == 1
    assert not (tmp_path / "wrong-region.json").exists()


def test_stage_rejects_matching_html_with_a_nonerrata_archive_kind(
    tmp_path: Path,
) -> None:
    raw, pin = _page("en", _EN_PAIR, "Oct. 03, 2026")
    store = _store(tmp_path)
    _put(
        store,
        replace(
            _resource(pin.url, "raw/notice.html", raw, Kind.QA),
            region=SourceRegion.EN,
        ),
        raw,
    )
    batch = seal_batch(store)
    output = tmp_path / "stage.json"
    result = CliRunner().invoke(
        app,
        [
            "stage-errata",
            str(store.root),
            store.store_id,
            batch.batch_id,
            batch.inventory.current[0].source_version_id,
            "en",
            str(output),
        ],
    )
    assert result.exit_code == 1
    assert result.output.strip() == "Frozen errata staging failed."
    assert not output.exists()


def test_transcribes_english_icons_and_reports_the_heading_boundary() -> None:
    raw, pin = _page(
        "en", _EN_PAIR + "<section class=inh1>Reason</section>", "Oct. 03, 2026"
    )
    notice = parse_notice(raw, pin, region="en")
    (block,) = notice.blocks
    assert notice.heading_date == "2026-10-03"
    assert block.evidence.before == "Old synthetic {icon|sign}& text"
    assert block.evidence.after == "New synthetic text"
    assert block.evidence.context == ("Changes",)
    assert block.end_basis == "heading"
    assert notice.outside_text == ("Changes", "Reason")


@pytest.mark.parametrize(
    ("body", "diagnostics"),
    [
        ("<p>(Incorrect)<br>Old.</p>", ("missing_correct_marker", "no_paired_changes")),
        ("<p>(Correct)<br>New.</p>", ("no_paired_changes", "orphan_correct_marker")),
        (
            "<p><script>(Incorrect)</script>↓</p>",
            ("no_paired_changes", "orphan_change_arrow"),
        ),
        (
            "<p>(Incorrect)<br>Old.</p>" + _EN_PAIR.split("</section>")[1],
            ("missing_correct_marker", "pair_truncated_by_incorrect_marker"),
        ),
    ],
)
def test_reports_uncertain_markup_for_review(
    body: str, diagnostics: tuple[str, ...]
) -> None:
    raw, pin = _page("en", body, None)
    notice = parse_notice(raw, pin, region="en")
    assert notice.issues == diagnostics
    assert notice.heading_date is notice.heading_date_raw is None


@pytest.mark.parametrize(
    ("before", "after", "message"),
    [
        (
            "<p>↓<br>",
            "<p>",
            "Errata pair requires exactly one change arrow",
        ),
        ("New synthetic text", "", "Errata pair has an empty side"),
        ('alt="sign"', "", "EN errata text icon lacks filename stem or alt"),
        ("TEST-002", "TEST-001", "Unrecognized errata card-number list"),
        ("Card Name", "Unknown", "Errata card-information labels are missing"),
        ("Card Name", "Card Number(s)", "Duplicate errata card-information label"),
        (
            "errata-Detail_Inner",
            "unrecognized",
            "Errata announcement container is missing or ambiguous",
        ),
    ],
)
def test_rejects_invalid_announcement(before: str, after: str, message: str) -> None:
    raw, pin = _page("en", _EN_PAIR, "Oct. 03, 2026")
    changed = raw.replace(before.encode(), after.encode())
    with pytest.raises(ValueError, match=message):
        parse_notice(
            changed, pin.model_copy(update={"sha256": digest(changed)}), region="en"
        )


def test_validates_source_pin_and_region() -> None:
    raw, pin = _page("en", _EN_PAIR, "Oct. 03, 2026")
    with pytest.raises(ValueError, match="hash/parser pin mismatch"):
        parse_notice(raw + b" ", pin, region="en")
    with pytest.raises(ValueError, match="URL/region/media mismatch"):
        parse_notice(raw, pin, region="jp")
    with pytest.raises(ValueError, match="requires an explicit JP or EN region"):
        parse_notice(raw, pin, region=cast("Region", "invalid"))
