"""Independent unknown/partial counterexamples using invented source wording."""

from dataclasses import replace
from typing import TYPE_CHECKING, Literal

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.card_extras import plan_card_extras, populate_card_extras
from sve_carddb.card_extras.changes import DownstreamUse, changes, conflicts
from sve_carddb.card_extras.generation import Observation, closure
from sve_carddb.card_extras.models import RelatedLink
from sve_carddb.core.json import digest
from sve_carddb.sources import official_en, official_jp
from sve_carddb.sources.official_qa import PARSER, materialize, parse_qa

from .card_extras_fixtures import context, seed, source
from .official_qa_fixtures import DETAIL, ROOT, SECOND, block, bodies, listing

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build_db import CompiledSchema, Database
    from sve_carddb.card_extras.models import QAPage
    from sve_carddb.registry.records import Region


@pytest.fixture(scope="module")
def observations() -> dict[str, Observation]:
    return {
        url: Observation(
            parse_qa(
                raw, url=url, region="jp", kind="detail" if url == DETAIL else "index"
            ),
            digest(raw)[7:],
        )
        for url, raw in bodies().items()
    }


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_build(("qa", "related", "errata"))


@pytest.fixture(scope="module")
def qa_pages() -> tuple[QAPage, ...]:
    result = []
    for hour, (url, raw) in enumerate(bodies().items()):
        pin = source(raw=raw, hour=hour).model_copy(
            update={"url": url, "parser_version": PARSER}
        )
        result.append(
            materialize(
                raw, pin, region="jp", kind="detail" if url == DETAIL else "index"
            )
        )
    return tuple(result)


@pytest.fixture(scope="module")
def imported(
    schema: CompiledSchema, qa_pages: tuple[QAPage, ...]
) -> Iterator[Database]:
    with create_database(schema) as db:
        seed(db)
        plan = plan_card_extras(db, (), qa_pages=qa_pages)
        with db.transaction():
            populate_card_extras(db, plan, build=context(plan))
        yield db


class TestObservations:
    def test_closed(self, observations: dict[str, Observation]) -> None:
        assert closure(ROOT, observations).complete

    def test_same_number_cross_page_conflict(
        self, qa_pages: tuple[QAPage, ...], imported: Database
    ) -> None:
        report = conflicts(qa_pages)
        assert len(report) == 1
        items = report[0]["observations"]
        assert isinstance(items, list)
        assert len(items) == 2
        assert "Synthetic answer" not in str(report)
        numbered = next(
            row.values["id"]
            for row in imported.rows("qa")
            if row.values["official_number"] == "Q900000"
        )
        assert (
            len(
                [
                    row
                    for row in imported.rows("qa_version")
                    if row.values["qa_id"] == numbered
                ]
            )
            == 2
        )

    def test_unnumbered_stays_unknown(self, imported: Database) -> None:
        unnumbered = [
            row for row in imported.rows("qa") if row.values["official_number"] is None
        ]
        assert len(unnumbered) == 1
        assert unnumbered[0].values["stable_source_key"] == ROOT + "#unnumbered"
        version = next(
            row
            for row in imported.rows("qa_version")
            if row.values["qa_id"] == unnumbered[0].values["id"]
        )
        assert version.values["published_on"] is None
        assert "unknown" in str(version.values["date_raw"])

    def test_repeated_multi_card_and_missing_target(self, imported: Database) -> None:
        owner = next(
            row.values["id"]
            for row in imported.rows("qa")
            if row.values["official_number"] == "Q900002"
        )
        version = next(
            row.values["id"]
            for row in imported.rows("qa_version")
            if row.values["qa_id"] == owner
        )
        assert {
            row.values["card_id"]
            for row in imported.rows("qa_card")
            if row.values["qa_version_id"] == version
        } == {"card", "card1"}
        assert any(
            row.values["category"] == "card_extras:qa_target_missing"
            for row in imported.rows("build_issue")
        )
        assert len(imported.rows("printing")) == 3

    def test_generation_is_not_temporal_source_window(self, imported: Database) -> None:
        assert not imported.rows("source_coverage")


@pytest.mark.parametrize("field", ["page", "max_page", "total"])
def test_unknown_pagination_is_not_complete(
    observations: dict[str, Observation], field: str
) -> None:
    changed = dict(observations)
    first = changed[ROOT]
    assert first.parsed.pagination is not None
    pagination = first.parsed.pagination
    mutations = {
        "page": replace(pagination, page=None),
        "max_page": replace(pagination, max_page=None),
        "total": replace(pagination, total=None),
    }
    changed[ROOT] = replace(
        first, parsed=replace(first.parsed, pagination=mutations[field])
    )
    assert closure(ROOT, changed).issues == ("pagination_unknown",)


@pytest.mark.parametrize(
    ("fault", "expected"),
    [
        ("root", "root_missing"),
        ("page", "index_unfetched"),
        ("detail", "detail_unfetched"),
        ("gap", "pagination_gap"),
        ("total", "total_disagreement"),
        ("page_number", "pagination_disagreement"),
        ("page_maximum", "pagination_disagreement"),
        ("page_total", "pagination_disagreement"),
        ("target", "pagination_target_disagreement"),
        ("empty", "empty_layout_unknown"),
        ("extra", "generation_membership_disagreement"),
        ("invalid_detail", "detail_invalid"),
    ],
)
def test_partial_cannot_promote(
    observations: dict[str, Observation], fault: str, expected: str
) -> None:
    changed = dict(observations)
    if fault in {"root", "page", "detail"}:
        del changed[{"root": ROOT, "page": SECOND, "detail": DETAIL}[fault]]
        if fault == "page":
            observed = changed[ROOT]
            assert observed.parsed.pagination is not None
            changed[ROOT] = replace(
                observed,
                parsed=replace(
                    observed.parsed,
                    pagination=replace(observed.parsed.pagination, total=3),
                ),
            )
    elif fault == "extra":
        changed[ROOT + "unexpected"] = changed[DETAIL]
    elif fault == "invalid_detail":
        changed[DETAIL] = replace(
            changed[DETAIL], parsed=replace(changed[DETAIL].parsed, blocks=())
        )
    elif fault == "empty":
        changed[SECOND] = replace(
            changed[SECOND],
            parsed=replace(changed[SECOND].parsed, issues=("unknown_empty_layout",)),
        )
    else:
        url = ROOT if fault in {"gap", "total"} else SECOND
        observed = changed[url]
        pagination = observed.parsed.pagination
        assert pagination is not None
        mutations = {
            "gap": replace(pagination, urls=((1, ROOT),), total=3),
            "total": replace(pagination, total=6),
            "page_number": replace(pagination, page=1),
            "page_maximum": replace(pagination, max_page=3),
            "page_total": replace(pagination, total=6),
            "target": replace(pagination, urls=((1, ROOT), (2, ROOT + "wrong"))),
        }
        changed[url] = replace(
            observed, parsed=replace(observed.parsed, pagination=mutations[fault])
        )
        if fault == "gap":
            del changed[SECOND]
        if fault == "total":
            observed = changed[SECOND]
            assert observed.parsed.pagination is not None
            changed[SECOND] = replace(
                observed,
                parsed=replace(
                    observed.parsed,
                    pagination=replace(observed.parsed.pagination, total=6),
                ),
            )
    checked = closure(ROOT, changed)
    assert not checked.complete
    assert expected in checked.issues


@pytest.mark.parametrize("date", ["2026/2/30", "unknown", "10/1"])
def test_unknown_date_not_invented(date: str) -> None:
    parsed = parse_qa(listing(block(date=date)), url=ROOT, region="jp", kind="index")
    assert parsed.blocks[0].entry.published_on is None
    assert date in str(parsed.blocks[0].entry.date_raw)


def test_explicit_update_and_withdrawal(
    schema: CompiledSchema, qa_pages: tuple[QAPage, ...]
) -> None:
    original = qa_pages[0]
    entry = original.blocks[0].entry
    updated = original.model_copy(
        update={
            "source": source(hour=4).model_copy(
                update={"url": ROOT, "parser_version": PARSER}
            ),
            "blocks": (
                original.blocks[0].model_copy(
                    update={
                        "entry": entry.model_copy(
                            update={
                                "updated_on": "2026-10-01",
                                "answer": "Synthetic same-day change.",
                            }
                        )
                    }
                ),
            ),
        }
    )
    withdrawn = updated.model_copy(
        update={
            "source": source(hour=5).model_copy(
                update={"url": ROOT, "parser_version": PARSER}
            ),
            "blocks": (
                updated.blocks[0].model_copy(
                    update={
                        "entry": updated.blocks[0].entry.model_copy(
                            update={"state": "withdrawn"}
                        )
                    }
                ),
            ),
        }
    )
    with create_database(schema) as db:
        seed(db)
        plan = plan_card_extras(db, (), qa_pages=(original, updated, withdrawn))
        versions = [
            version
            for version in plan.questions
            if version.qa_id == entry.identity("jp")
        ]
        assert len({version.id for version in versions}) == 3
        assert [version.entry.state for version in versions] == [
            "active",
            "active",
            "withdrawn",
        ]


def test_change_report_is_identifiers_and_unknown_inventory(
    imported: Database, qa_pages: tuple[QAPage, ...]
) -> None:
    original = qa_pages[0]
    changed = original.model_copy(
        update={
            "source": source(hour=6).model_copy(
                update={"url": ROOT, "parser_version": PARSER}
            ),
            "blocks": (
                original.blocks[0].model_copy(
                    update={
                        "entry": original.blocks[0].entry.model_copy(
                            update={"answer": "Synthetic new wording."}
                        )
                    }
                ),
            ),
        }
    )
    plan = plan_card_extras(imported, (), qa_pages=(*qa_pages, changed))
    report = changes(imported, plan)
    versions = report["new_qa_versions"]
    assert isinstance(versions, list)
    assert len(versions) == 1
    assert report["affected_downstream"] == {
        "ruling": None,
        "dsl": None,
        "translation": None,
        "card_ids": ["card", "card1"],
    }
    old = next(
        row.values["id"]
        for row in imported.rows("qa_version")
        if row.values["qa_id"] == original.blocks[0].entry.identity("jp")
    )
    known = changes(
        imported,
        plan,
        references=(
            DownstreamUse(str(old), "ruling", "synthetic-ruling"),
            DownstreamUse(str(old), "dsl", "synthetic-dsl"),
            DownstreamUse(str(old), "translation", "synthetic-translation"),
        ),
    )
    assert known["affected_downstream"] == {
        "ruling": ["synthetic-ruling"],
        "dsl": ["synthetic-dsl"],
        "translation": ["synthetic-translation"],
        "card_ids": ["card", "card1"],
    }
    assert "Synthetic new wording" not in str(known)
    with pytest.raises(ValueError, match="unknown previous"):
        changes(imported, plan, references=(DownstreamUse("absent", "dsl", "test"),))


def test_en_contract_and_explicit_unanchored_identity() -> None:
    url = f"https://{official_en.HOST}/qa/synthetic/"
    parsed = parse_qa(
        listing(block("", date="unknown"))
        .replace(ROOT.encode(), url.encode())
        .replace(SECOND.encode(), (url + "?page=2").encode()),
        url=url,
        region="en",
        kind="index",
    )
    assert parsed.blocks[0].entry.official_number is None
    assert "unnumbered_identity_reconciliation" in parsed.issues


@pytest.mark.parametrize(
    "fault",
    ["region", "url", "state", "wording", "detail_empty", "duplicate_page", "url_page"],
)
def test_source_contract_rejects_guessing(fault: str) -> None:
    raw = listing(block())
    url = ROOT
    region: Region = "jp"
    kind: Literal["index", "detail"] = "index"
    if fault == "region":
        region = "en"
    elif fault == "url":
        url = "https://example.invalid/qa/synthetic/"
    elif fault == "state":
        raw = raw.replace(b'data-state="active"', b'data-state="missing"')
    elif fault == "wording":
        raw = raw.replace(b"qa-List_Txt-A", b"unknown-answer")
    elif fault == "detail_empty":
        raw, kind = b'<div class="qa-List"></div>', "detail"
    elif fault == "duplicate_page":
        raw = raw.replace(SECOND.encode(), (SECOND + "&page=2").encode())
    else:
        raw = raw.replace(b'data-page="2" href=', b'data-page="3" href=')
    with pytest.raises(
        ValueError, match=r"regional|withdrawal|wording|observed|URL/number"
    ):
        parse_qa(raw, url=url, region=region, kind=kind)


@pytest.mark.parametrize("fault", ["hash", "parser", "region", "media"])
def test_frozen_qa_pin_cannot_be_guessed(fault: str) -> None:
    raw = bodies()[ROOT]
    pin = source(raw=raw).model_copy(update={"url": ROOT, "parser_version": PARSER})
    changes = {
        "hash": {"sha256": "sha256:" + "0" * 64},
        "parser": {"parser_version": "unknown"},
        "media": {"kind": "official_pdf"},
    }
    if fault != "region":
        pin = pin.model_copy(update=changes[fault])
    with pytest.raises(ValueError, match=r"hash/parser|regional|URL/region/media"):
        materialize(raw, pin, region="en" if fault == "region" else "jp", kind="index")


def test_unknown_empty_listing_is_not_absence() -> None:
    raw = b'<div class="qa-List" data-page="1" data-max-page="1" data-total="0"></div>'
    parsed = parse_qa(raw, url=ROOT, region="jp", kind="index")
    assert not closure(ROOT, {ROOT: Observation(parsed, digest(raw)[7:])}).complete
    explicit = raw.replace(b"</div>", b'<div class="qa-Empty"></div></div>')
    parsed = parse_qa(explicit, url=ROOT, region="jp", kind="index")
    assert closure(ROOT, {ROOT: Observation(parsed, digest(explicit)[7:])}).complete


def test_association_change_reports_downstream_without_inventing_version(
    imported: Database, qa_pages: tuple[QAPage, ...]
) -> None:
    original = qa_pages[0]
    unnumbered = original.blocks[1]
    linked = original.model_copy(
        update={
            "source": source(hour=7).model_copy(
                update={"url": ROOT, "parser_version": PARSER}
            ),
            "blocks": (
                unnumbered.model_copy(
                    update={
                        "card_links": (
                            RelatedLink(
                                locator="qa-block:1/card-link:0",
                                href_raw=official_jp.card_url("TEST-002"),
                            ),
                        )
                    }
                ),
            ),
        }
    )
    plan = plan_card_extras(imported, (), qa_pages=(*qa_pages, linked))
    report = changes(imported, plan)
    assert report["new_qa_versions"] == []
    assert report["affected_downstream"] == {
        "ruling": None,
        "dsl": None,
        "translation": None,
        "card_ids": ["card1"],
    }


def test_qa_block_outside_expected_container_is_rejected() -> None:
    raw = bodies()[ROOT] + block("Q999999").encode()
    with pytest.raises(ValueError, match="Unrecognized Q&A block layout"):
        parse_qa(raw, url=ROOT, region="jp", kind="index")
