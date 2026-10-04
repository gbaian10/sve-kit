"""Minimal signed counterexamples for every new glossary adoption guard."""

import re
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.translations.loader import load_glossary, record_hash
from sve_carddb.translations.models import (
    AdoptionReview,
    EmphasisData,
    SourceClaim,
    TermData,
)

from .translation_fixtures import INSTANT, choice, envelope, term, write

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.records import RecordData

NOTE = "維護者委託；協調者決定；不是維護者親自核可"


def checked(model: type[RecordData], data: dict[str, JsonValue]) -> None:
    """Anchor the single Pydantic diagnostic rather than accepting another failure."""
    try:
        model.model_validate_json(canonical(data))
    except ValidationError as error:
        errors = error.errors()
        assert len(errors) == 1
        raise ValueError(str(errors[0]["msg"]).removeprefix("Value error, ")) from error


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


def emphasis(
    *, number: int = 1, value: JsonValue = True, predecessor: JsonValue = None
) -> dict[str, JsonValue]:
    return {
        "record_key": canonical(
            ["glossary_emphasis_choice", "term:rule.test", number]
        ).decode(),
        "kind": "glossary_emphasis_choice",
        "filing_key": "emphasis",
        "data": {
            "term_id": "term:rule.test",
            "value": value,
            "adoption_review": {"mode": "human", "delegation": None},
            "adoption_no": number,
            "predecessor": predecessor,
        },
        "evidence": [],
    }


def delegated(records: list[dict[str, JsonValue]]) -> dict[str, JsonValue]:
    scope = sorted(str(record["record_key"]) for record in records)
    for record in records:
        object_value(record["data"])["adoption_review"] = {
            "mode": "delegated_glossary",
            "delegation": {
                "authorized_by": "Synthetic maintainer",
                "authorization_basis": "Synthetic scoped maintainer delegation, section 1.",
                "authorization_date": "2026-10-02",
                "scope": list[JsonValue](scope),
                "decided_by": "Synthetic coordinator AI",
                "decided_at": INSTANT,
                "decided_precision": "day",
                "decision_basis": "Synthetic coordinator decision, section 2.",
            },
        }
    shard = envelope(records)
    decision = object_value(array(shard["decisions"])[0])
    decision.update({"reviewed_by": "Synthetic coordinator AI", "note": NOTE})
    return shard


def receipt(record: dict[str, JsonValue]) -> dict[str, JsonValue]:
    return object_value(
        object_value(object_value(record["data"])["adoption_review"])["delegation"]
    )


@pytest.mark.parametrize(
    "field", ["authorized_by", "authorization_basis", "decided_by", "decision_basis"]
)
def test_delegation_nonblank(field: str) -> None:
    record = authored()
    delegated([record])
    receipt(record)[field] = " "
    with pytest.raises(ValueError, match=r"^Delegation receipt text must be nonblank$"):
        checked(TermData, object_value(record["data"]))


@pytest.mark.parametrize("scope", [["z", "a"], ["a", "a"]])
def test_scope_order(scope: list[str]) -> None:
    record = authored()
    delegated([record])
    receipt(record)["scope"] = list[JsonValue](scope)
    with pytest.raises(
        ValueError, match=r"^Delegation scope must be sorted and unique$"
    ):
        checked(TermData, object_value(record["data"]))


def test_receipt_day_precision() -> None:
    record = authored()
    delegated([record])
    receipt(record)["decided_at"] = "2026-10-02T00:00:01Z"
    with pytest.raises(
        ValueError, match=r"^Delegation day precision must use UTC midnight$"
    ):
        checked(TermData, object_value(record["data"]))


@pytest.mark.parametrize("human", [True, False])
def test_review_mode_receipt_pair(human: bool) -> None:
    record = authored()
    delegated([record])
    review = object_value(object_value(record["data"])["adoption_review"])
    if human:
        review["mode"] = "human"
    else:
        review["delegation"] = None
    with pytest.raises(
        ValueError, match=r"^Glossary review mode and delegation disagree$"
    ):
        checked(AdoptionReview, review)


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


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("sampled", "Delegated glossary requires confirmed full checks"),
        ("reviewer", "Delegated glossary decision differs from receipt event"),
        ("time", "Delegated glossary decision differs from receipt event"),
        ("precision", "Delegated glossary decision differs from receipt event"),
        ("note", "Delegated glossary note must identify delegated approval"),
        ("excluded", "Delegation scope excludes current glossary record"),
        ("absent", "Delegation scope references absent glossary record"),
    ],
)
def test_delegated_envelope_and_closure(
    tmp_path: Path, fault: str, message: str
) -> None:
    record = authored()
    shard = delegated([record])
    decision = object_value(array(shard["decisions"])[0])
    if fault == "sampled":
        decision["state"] = "sampled"
    elif fault == "reviewer":
        decision["reviewed_by"] = "Synthetic different coordinator"
    elif fault == "time":
        decision["reviewed_at"] = "2026-10-03T00:00:00Z"
    elif fault == "precision":
        decision["reviewed_precision"] = "instant"
    elif fault == "note":
        decision["note"] = "Maintainer personally approved."
    else:
        receipt(record)["scope"] = (
            ["absent"]
            if fault == "excluded"
            else list[JsonValue](sorted(["absent", str(record["record_key"])]))
        )
        shard = envelope([record])
        object_value(array(shard["decisions"])[0]).update(
            {"reviewed_by": "Synthetic coordinator AI", "note": NOTE}
        )
    write(tmp_path, {"translations/glossary/concepts/001.yaml": shard})
    with pytest.raises(ValueError, match=r"^" + re.escape(message) + "$"):
        load_glossary(tmp_path)


def test_mixed_modes_rejected(tmp_path: Path) -> None:
    first, second = authored(), authored("rule.second")
    delegated([first])
    write(
        tmp_path, {"translations/glossary/concepts/001.yaml": envelope([first, second])}
    )
    with pytest.raises(
        ValueError, match=r"^Glossary review modes must be uniform within a shard$"
    ):
        load_glossary(tmp_path)


@pytest.mark.parametrize("second_human", [True, False])
def test_scope_checks_every_members_receipt(tmp_path: Path, second_human: bool) -> None:
    first, second = authored(), authored("rule.second")
    first_shard = delegated([first])
    receipt(first)["scope"] = list[JsonValue](
        sorted([str(first["record_key"]), str(second["record_key"])])
    )
    first_shard = envelope([first])
    object_value(array(first_shard["decisions"])[0]).update(
        {"reviewed_by": "Synthetic coordinator AI", "note": NOTE}
    )
    second_shard = envelope([second]) if second_human else delegated([second])
    write(
        tmp_path,
        {
            "translations/glossary/concepts/001.yaml": first_shard,
            "translations/glossary/concepts/002.yaml": second_shard,
        },
    )
    with pytest.raises(
        ValueError, match=r"^Delegation scope member has a different receipt$"
    ):
        load_glossary(tmp_path)


@pytest.mark.parametrize(
    ("definition", "message"),
    [
        (None, "Emphasis references an unadopted concept"),
        (authored(category="ability"), "Only rule terms accept emphasis choices"),
    ],
)
def test_emphasis_target(
    tmp_path: Path, definition: dict[str, JsonValue] | None, message: str
) -> None:
    shards = {"translations/glossary/emphasis/001.yaml": envelope([emphasis()])}
    if definition is not None:
        shards["translations/glossary/concepts/001.yaml"] = envelope([definition])
    write(tmp_path, shards)
    with pytest.raises(ValueError, match=r"^" + re.escape(message) + "$"):
        load_glossary(tmp_path)


@pytest.mark.parametrize("value", [True, False, None])
def test_effective_emphasis_and_delegation_counts(
    tmp_path: Path, value: bool | None
) -> None:
    definition, selected = authored(), choice()
    object_value(selected["data"])["source_claim"] = claim()
    revised = emphasis(value=value)
    write(
        tmp_path,
        {
            "translations/glossary/concepts/001.yaml": delegated([definition]),
            "translations/glossary/choices/001.yaml": delegated([selected]),
            "translations/glossary/emphasis/001.yaml": delegated([revised]),
        },
    )
    loaded = load_glossary(tmp_path)
    result = loaded.emphasis("term:rule.test")
    assert result.bold is value
    assert result.record_hash == record_hash(
        next(r for r, _ in loaded.records() if r.kind == "glossary_emphasis_choice")
    )
    assert result.decision_id is not None
    assert loaded.review_counts() == {
        "human_sampled_rows": 0,
        "delegated_glossary_rows": 3,
    }
    assert len(loaded.effective()) == 3
    with pytest.raises(ValueError, match=r"^Emphasis references an unadopted concept$"):
        loaded.emphasis("term:absent")


def test_emphasis_revision_and_withdrawal(tmp_path: Path) -> None:
    first = emphasis()
    first_shard = envelope([first])
    previous = {
        "record_key": first["record_key"],
        "record_hash": digest(canonical(first)),
        "decision_id": first_shard["default_decision_id"],
    }
    second = emphasis(number=2, value=None, predecessor=previous)
    write(
        tmp_path,
        {
            "translations/glossary/concepts/001.yaml": envelope([authored()]),
            "translations/glossary/emphasis/001.yaml": first_shard,
            "translations/glossary/emphasis/002.yaml": envelope([second]),
        },
    )
    loaded = load_glossary(tmp_path)
    assert loaded.emphasis("term:rule.test").bold is None
    assert len(loaded.records()) == 3
    assert len(loaded.effective()) == 2


def test_missing_and_type_derived_emphasis(tmp_path: Path) -> None:
    write(
        tmp_path,
        {
            "translations/glossary/concepts/001.yaml": envelope(
                [authored(), authored("trait.test", category="trait")]
            )
        },
    )
    loaded = load_glossary(tmp_path)
    assert loaded.emphasis("term:rule.test").bold is None
    assert loaded.emphasis("term:trait.test").bold is True
    assert loaded.emphasis("term:trait.test").record_hash is None
    assert loaded.review_counts() == {
        "human_sampled_rows": 2,
        "delegated_glossary_rows": 0,
    }


@pytest.mark.parametrize(
    "field", ["authored_source_ja", "missing_source_reason", "adoption_review"]
)
def test_new_term_fields_required(field: str) -> None:
    data = object_value(authored()["data"])
    data.pop(field)
    with pytest.raises(ValueError, match=r"^Field required$"):
        checked(TermData, data)


@pytest.mark.parametrize("field", ["source_claim", "adoption_review"])
def test_new_choice_fields_required(field: str) -> None:
    from sve_carddb.translations.models import ChoiceData  # ruff: ignore[import-outside-top-level] -- isolate the two added required fields

    data = object_value(choice()["data"])
    data.pop(field)
    with pytest.raises(ValueError, match=r"^Field required$"):
        checked(ChoiceData, data)


@pytest.mark.parametrize(
    "field",
    [
        "authorized_by",
        "authorization_basis",
        "authorization_date",
        "scope",
        "decided_by",
        "decided_at",
        "decided_precision",
        "decision_basis",
    ],
)
def test_receipt_fields_required(field: str) -> None:
    record = authored()
    delegated([record])
    receipt(record).pop(field)
    with pytest.raises(ValueError, match=r"^Field required$"):
        checked(TermData, object_value(record["data"]))


def test_receipt_scope_nonempty() -> None:
    record = authored()
    delegated([record])
    receipt(record)["scope"] = []
    with pytest.raises(
        ValueError, match=r"^Tuple should have at least 1 item after validation, not 0$"
    ):
        checked(TermData, object_value(record["data"]))


def test_emphasis_history_wrong_predecessor(tmp_path: Path) -> None:
    first = emphasis()
    first_shard = envelope([first])
    second = emphasis(
        number=2,
        predecessor={
            "record_key": first["record_key"],
            "record_hash": "sha256:" + "f" * 64,
            "decision_id": first_shard["default_decision_id"],
        },
    )
    write(
        tmp_path,
        {
            "translations/glossary/concepts/001.yaml": envelope([authored()]),
            "translations/glossary/emphasis/001.yaml": first_shard,
            "translations/glossary/emphasis/002.yaml": envelope([second]),
        },
    )
    with pytest.raises(ValueError, match=r"^Translation predecessor mismatch$"):
        load_glossary(tmp_path)


def test_delegated_confirmed_checks_every_member(tmp_path: Path) -> None:
    shard = delegated([authored(), authored("rule.second")])
    decision = object_value(array(shard["decisions"])[0])
    decision["sample_ids"] = array(decision["sample_ids"])[:1]
    write(tmp_path, {"translations/glossary/concepts/001.yaml": shard})
    with pytest.raises(
        ValueError, match=r"^Confirmed translation must check every member$"
    ):
        load_glossary(tmp_path)


@pytest.mark.parametrize(
    ("number", "predecessor", "message"),
    [
        (2, None, "Translation adoption sequence gap or fork"),
        (
            1,
            {
                "record_key": "prior",
                "record_hash": "sha256:" + "a" * 64,
                "decision_id": "d:" + "b" * 64,
            },
            "Initial choice must have no predecessor",
        ),
    ],
)
def test_emphasis_sequence_guards(
    tmp_path: Path, number: int, predecessor: JsonValue, message: str
) -> None:
    write(
        tmp_path,
        {
            "translations/glossary/concepts/001.yaml": envelope([authored()]),
            "translations/glossary/emphasis/001.yaml": envelope(
                [emphasis(number=number, predecessor=predecessor)]
            ),
        },
    )
    with pytest.raises(ValueError, match=r"^" + re.escape(message) + "$"):
        load_glossary(tmp_path)


@pytest.mark.parametrize("target", ["receipt", "claim", "missing_source", "bold"])
def test_new_metadata_is_membership_bound(tmp_path: Path, target: str) -> None:
    record = (
        choice()
        if target == "claim"
        else emphasis()
        if target == "bold"
        else authored()
    )
    if target == "claim":
        object_value(record["data"])["source_claim"] = claim()
    shard = delegated([record])
    data = object_value(record["data"])
    if target == "receipt":
        receipt(record)["authorization_basis"] = (
            "Different synthetic scoped delegation."
        )
    elif target == "claim":
        object_value(data["source_claim"])["note"] = "Changed unverified claim."
    elif target == "missing_source":
        data["missing_source_reason"] = "Changed synthetic missing source reason."
    else:
        data["value"] = False
    write(
        tmp_path,
        {"translations/glossary/" + str(record["filing_key"]) + "/001.yaml": shard},
    )
    with pytest.raises(
        ValueError, match=r"^Translation decision exact membership mismatch$"
    ):
        load_glossary(tmp_path)
