"""First availability is proven from every confirmed inclusion, never crawl dates."""

import dataclasses
import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.products.loader import LoadedShard, ProductSnapshot
from sve_carddb.products.models import InclusionRecord, ProductRecord, Shard
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.wording_adoptions.replay import _availability, _official_order

from .product_fixtures import Object, decision, envelope, inclusion, obj, product
from .test_wording_adoption_integration import adoption_case as adoption_case  # ruff: ignore[useless-import-alias] -- shared immutable synthetic replay fixture

if TYPE_CHECKING:
    from sve_carddb.wording_adoptions.reconstruction import ReconstructedScope

    from .wording_adoption_fixtures import AdoptionCase


def with_products(case: AdoptionCase, change: str) -> ReconstructedScope:
    items = sorted(case.scope.contents.values(), key=lambda i: i.printing_id)
    printings = sorted({i.printing_id for i in items})
    records: list[Object] = []
    for n, printing in enumerate(printings):
        for m, day in enumerate(("2020-01-01", "2021-01-01")):
            identifier = f"synthetic-{n}-{m}"
            p = product(region="en" if change == "region" else "jp")
            p["record_key"] = canonical(["product", identifier]).decode()
            obj(p["data"]).update(
                id=identifier,
                released_on=day.replace("2020", str(2020 + n)).replace(
                    "2021", str(2021 + n)
                ),
                date_precision="day",
            )
            inc = inclusion(printing)
            inc["record_key"] = canonical(
                ["printing_product", printing, identifier]
            ).decode()
            obj(inc["data"])["product_id"] = identifier
            evidence = case.replayed[0].record.evidence[n].model_dump(mode="json")
            p["evidence"] = [evidence]
            inc["evidence"] = [evidence]
            if m == 1 and change in {"unknown", "month", "year"}:
                precision = change
                obj(p["data"]).update(
                    released_on=None,
                    date_precision=precision,
                    date_raw=None
                    if precision == "unknown"
                    else "2021-01"
                    if precision == "month"
                    else "2021",
                )
            if change == "override":
                obj(inc["data"]).update(
                    first_available_on="2019-01-01",
                    first_available_precision="day",
                    first_available_raw=None,
                )
            if change == "override-month":
                obj(inc["data"]).update(
                    first_available_on=None,
                    first_available_precision="month",
                    first_available_raw="2019-01",
                )
            records.extend((p, inc))
    raw = envelope(records)
    if change == "unconfirmed":
        decision(raw).update(
            state="proposed",
            sample_ids=[],
        )
    parsed = Shard.model_validate_json(canonical(raw))
    data = {
        r.record_key: r
        for r in parsed.records
        if isinstance(r, (ProductRecord, InclusionRecord))
    }
    if change == "missing":
        data = {k: r for k, r in data.items() if not isinstance(r, InclusionRecord)}
    catalog = ProductSnapshot(
        b"{}",
        b"{}",
        (LoadedShard("synthetic", digest(canonical(raw)), canonical(raw), parsed),),
        data,
        {d.id: d for d in parsed.decisions},
    )
    return dataclasses.replace(case.scope, products=catalog)


@pytest.mark.parametrize(
    "change",
    [
        "none",
        "override",
        "unknown",
        "month",
        "year",
        "override-month",
        "unconfirmed",
        "region",
        "missing",
        "no-catalog",
    ],
)
def test_first_day_requires_all_inclusions_and_explicit_precision(
    adoption_case: AdoptionCase, change: str
) -> None:
    scope = (
        dataclasses.replace(adoption_case.scope, products=None)
        if change == "no-catalog"
        else with_products(adoption_case, change)
    )
    item = min(scope.contents.values(), key=lambda i: i.printing_id)
    if change in {"none", "override"}:
        day, sources = _availability(scope, item)
        assert day == ("2019-01-01" if change == "override" else "2020-01-01")
        assert sources
    else:
        message = {
            "unknown": "Unknown, month or year precision cannot establish first availability",
            "month": "Unknown, month or year precision cannot establish first availability",
            "year": "Unknown, month or year precision cannot establish first availability",
            "override-month": "Unknown, month or year precision cannot establish first availability",
            "unconfirmed": "First availability has missing or unconfirmed inclusions",
            "region": "Printing order product is unconfirmed or in another region",
            "missing": "First availability has missing or unconfirmed inclusions",
            "no-catalog": "Printing order requires a pinned formal product catalog",
        }[change]
        with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
            _availability(scope, item)


@pytest.mark.parametrize(
    "change",
    ["none", "reverse", "same-day", "missing-proof", "extra-proof", "source-update"],
)
def test_official_order_requires_strict_dates_and_the_exact_supporting_sources(
    adoption_case: AdoptionCase, change: str
) -> None:
    case = adoption_case
    scope = with_products(case, "override" if change == "same-day" else "none")
    items = {i.printing_id: i for i in scope.contents.values()}
    first, second = (items[k] for k in sorted(items))
    record = case.replayed[0].record
    sources = _availability(scope, first)[1] | _availability(scope, second)[1]
    indexes = tuple(i for i, e in enumerate(record.evidence) if e in sources)
    if change == "reverse":
        first, second = second, first
    elif change == "missing-proof":
        indexes = indexes[:-1]
    elif change == "extra-proof":
        indexes = tuple(range(len(record.evidence)))
    basis = "source_update" if change == "source-update" else "printing_availability"
    if change == "none":
        _official_order(record, scope, (first,), (second,), basis, indexes)
    else:
        message = {
            "reverse": "Official evidence does not prove strict adjacent wording order",
            "same-day": "Official evidence does not prove strict adjacent wording order",
            "missing-proof": "Order evidence must cover exactly every first-availability source",
            "extra-proof": "Order evidence must cover exactly every first-availability source",
            "source-update": "Official source-update recipe is explicitly unimplemented",
        }[change]
        with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
            _official_order(record, scope, (first,), (second,), basis, indexes)
