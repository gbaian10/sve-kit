"""Minimal signed counterexamples for every new glossary adoption guard."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.snapshot.values import canonical, object_value
from sve_carddb.translations.current_models import EmphasisData, TermData
from sve_carddb.translations.models import SourceClaim

from .translation_fixtures import term

if TYPE_CHECKING:
    from sve_carddb.registry.records import RecordData

NOTE = "維護者委託；協調者決定；不是維護者親自核可"


def authored(
    identifier: str = "rule.test", *, category: str = "rule_term"
) -> dict[str, JsonValue]:
    record = term(identifier, category=category)
    object_value(record["data"]).update(
        {
            "source_ref": None,
            "authored_source_ja": "合成名",
            "missing_source_reason": "合成專案概念，沒有凍結來源。",
        }
    )
    return record


def emphasis(*, value: JsonValue = True) -> dict[str, JsonValue]:
    return {
        "record_key": canonical(
            ["glossary_emphasis_choice", "term:rule.test"]
        ).decode(),
        "kind": "glossary_emphasis_choice",
        "origin": "project",
        "low_confidence": False,
        "note": "",
        "data": {"term_id": "term:rule.test", "value": value},
    }


@pytest.mark.parametrize("field", ["authored_source_ja", "missing_source_reason"])
def test_frozen_mode_excludes_authored_metadata(field: str) -> None:
    data = object_value(term()["data"])
    data[field] = "合成"
    with pytest.raises(
        ValueError, match=r"^Frozen glossary source forbids authored source metadata$"
    ):
        checked(TermData, data)


@pytest.mark.parametrize(
    "field", ["authored_source_ja", "missing_source_reason", "source_span"]
)
def test_authored_mode_requires_complete_metadata(field: str) -> None:
    data = object_value(authored()["data"])
    data[field] = {"start": 0, "end": 1} if field == "source_span" else None
    with pytest.raises(
        ValueError,
        match=r"^Authored glossary source requires name and reason without span$",
    ):
        checked(TermData, data)


@pytest.mark.parametrize("field", ["authored_source_ja", "missing_source_reason"])
def test_authored_metadata_nonblank(field: str) -> None:
    data = object_value(authored()["data"])
    data[field] = "\t"
    with pytest.raises(
        ValueError, match=r"^Authored glossary source name and reason must be nonblank$"
    ):
        checked(TermData, data)


def claim() -> dict[str, JsonValue]:
    return {
        "source_work": "合成作品",
        "source_urls": ["https://example.invalid/source"],
        "claimed_source": "尚未凍結的出處主張",
        "note": "僅專案決定。",
    }


def test_source_claim_allows_omitted_attribution() -> None:
    data = claim()
    data.pop("claimed_source")
    result = SourceClaim.model_validate_json(canonical(data))
    assert result.claimed_source is None
    assert result.source_work == data["source_work"]
    assert list(result.source_urls) == data["source_urls"]
    assert result.note == data["note"]


@pytest.mark.parametrize("field", ["source_work", "claimed_source", "note"])
def test_source_claim_nonblank(field: str) -> None:
    data = claim()
    data[field] = " "
    with pytest.raises(ValueError, match=r"^Source claim text must be nonblank$"):
        checked(SourceClaim, data)


@pytest.mark.parametrize(
    "urls",
    [
        ["https://z.invalid", "https://a.invalid"],
        ["https://a.invalid", "https://a.invalid"],
    ],
)
def test_source_claim_url_order(urls: list[str]) -> None:
    data = claim()
    data["source_urls"] = list[JsonValue](urls)
    with pytest.raises(
        ValueError, match=r"^Source claim URLs must be sorted and unique$"
    ):
        checked(SourceClaim, data)


@pytest.mark.parametrize(
    "url",
    [
        "ftp://example.invalid",
        "https:///missing-host",
        "https://example.invalid/a b",
        "https://[malformed-ipv6",
    ],
)
def test_source_claim_url_scheme_host_and_space(url: str) -> None:
    data = claim()
    data["source_urls"] = [url]
    with pytest.raises(ValueError, match=r"^Source claim URL must be HTTP or HTTPS$"):
        checked(SourceClaim, data)


@pytest.mark.parametrize("value", [0, 1, "true", "false", [], {}])
def test_emphasis_strict_bool(value: JsonValue) -> None:
    data = object_value(emphasis(value=value)["data"])
    with pytest.raises(ValueError, match=r"^Input should be a valid boolean$"):
        checked(EmphasisData, data)


def checked(model: type[RecordData], data: dict[str, JsonValue]) -> None:
    """Anchor the single Pydantic diagnostic rather than accepting another failure."""
    try:
        model.model_validate_json(canonical(data))
    except ValidationError as error:
        errors = error.errors()
        assert len(errors) == 1
        raise ValueError(str(errors[0]["msg"]).removeprefix("Value error, ")) from error
