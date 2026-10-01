"""Independent logical counterexamples for the public pending display contract."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.snapshot.reader import _container, _current
from sve_carddb.snapshot.semantics import _wording_display, validate_view
from sve_carddb.snapshot.values import array, object_value, parse, string

from .test_snapshot_contract import fixture, payloads

if TYPE_CHECKING:
    from sve_carddb.snapshot.reader import Fragment, Row, View


def pending_view() -> tuple[View, Row, list[Fragment]]:
    raw = object_value(fixture("expected-logical.json"))
    view = {
        key: [object_value(row) for row in array(rows)] for key, rows in raw.items()
    }
    manifest = object_value(fixture("manifest.json"))
    blobs = payloads()
    fragments = [
        fragment
        for raw_file in array(manifest["files"])
        for file in (object_value(raw_file),)
        if file["role"] not in {"config", "programs"}
        for fragment in _container(
            file, object_value(parse(blobs[string(file["key"])]))
        )
    ]
    face = view["face"][0]
    face["current"] = []
    wording = object_value(array(face["wording"])[0])
    wording["display"] = {"revision_id": "r:a2", "basis": "latest_known_release"}
    wording["undated_printing_ids"] = ["p:b"]
    view["product"][0].update(released_on="2026-01-01", date_precision="day")
    view["printing_product"] = [
        {
            "printing_id": "p:a",
            "product_id": "prod:null",
            "available_on": None,
            "date_precision": None,
            "date_raw": None,
            "inclusion_kind": "other",
            "note_unit_id": None,
            "first_inclusion_state": "unknown",
        }
    ]
    view["card_engine_support"][0]["region_blocks"] = [
        {"region": "jp", "reasons": ["wording_pending"]}
    ]
    return view, manifest, fragments


def test_latest_display_is_public_without_adopted_front_current() -> None:
    view, manifest, fragments = pending_view()
    _current(view, fragments)
    validate_view(view, manifest, fragments)
    assert not view["face"][0]["current"]
    assert view["face"][1]["current"]


@pytest.mark.parametrize(
    ("change", "error"),
    [
        ("latest_missing", "unavailable/latest"),
        ("same_day_different", "unavailable/latest"),
        ("latest_conflict", "unavailable/latest"),
        ("missing_block", "wording_pending"),
        ("missing_pending", "requires pending"),
        ("wrong_candidate", "no printing observation"),
        ("wrong_undated", "undated wording"),
    ],
)
def test_pending_display_rejects_independent_public_constraint(
    change: str, error: str
) -> None:
    view, manifest, fragments = pending_view()
    face = view["face"][0]
    wording = object_value(array(face["wording"])[0])
    printing = view["printing"][1]
    physical = object_value(array(printing["faces"])[0])
    if change in {"latest_missing", "same_day_different"}:
        inclusion = view["printing_product"][0].copy()
        inclusion.update(
            printing_id="p:b",
            available_on="2027-01-01" if change == "latest_missing" else "2026-01-01",
            date_precision="day",
        )
        view["printing_product"].append(inclusion)
        wording["undated_printing_ids"] = []
        if change == "same_day_different":
            physical["observations"] = [
                {
                    "revision_id": "r:a1",
                    "state": "available",
                    "source_url": "https://example.invalid/b",
                }
            ]
            object_value(array(wording["candidates"])[1])["revision_id"] = "r:a1"
    elif change == "latest_conflict":
        observed = object_value(
            array(object_value(array(view["printing"][0]["faces"])[0])["observations"])[
                0
            ]
        )
        observed["state"] = "correction_conflict"
    elif change == "missing_block":
        view["card_engine_support"][0]["region_blocks"] = []
    elif change == "missing_pending":
        face["wording"] = []
    elif change == "wrong_candidate":
        object_value(array(wording["candidates"])[1])["revision_id"] = "r:a1"
    elif change == "wrong_undated":
        wording["undated_printing_ids"] = []
    with pytest.raises(ValueError, match=error):
        validate_view(view, manifest, fragments)


def test_same_day_exact_public_content_can_use_distinct_correction_revision_ids() -> (
    None
):
    view, manifest, fragments = pending_view()
    inclusion = view["printing_product"][0].copy()
    inclusion["printing_id"] = "p:b"
    view["printing_product"].append(inclusion)
    wording = object_value(array(view["face"][0]["wording"])[0])
    wording["undated_printing_ids"] = []
    object_value(array(wording["candidates"])[1])["revision_id"] = "r:a1"
    physical = object_value(array(view["printing"][1]["faces"])[0])
    physical["observations"] = [
        {
            "revision_id": "r:a1",
            "state": "available",
            "source_url": "https://example.invalid/b",
        }
    ]
    original = view["face_revision"][0]
    latest = view["face_revision"][1]
    exact: dict[str, JsonValue] = latest | {
        "id": "r:a1",
        "revision": 1,
        "change_kind": "initial",
    }
    original.update(exact)
    validate_view(view, manifest, fragments)


@pytest.mark.parametrize("boundary", ["candidate", "face", "region"])
def test_display_boundary_independently_of_partition_and_observation_checks(
    boundary: str,
) -> None:
    view, _, _ = pending_view()
    face = view["face"][0]
    wording = object_value(array(face["wording"])[0])
    display = object_value(wording["display"])
    revisions = {string(row["id"]): row for row in view["face_revision"]}
    current: dict[str, JsonValue] = {}
    if boundary == "candidate":
        display["revision_id"] = "r:a1"
        error = "not a pending candidate"
    else:
        display["basis"] = "current"
        identifier = "r:b1" if boundary == "face" else "r:a2"
        display["revision_id"] = identifier
        current["jp"] = identifier
        if boundary == "region":
            revisions[identifier]["region"] = "en"
        error = "another face/region"
    with pytest.raises(ValueError, match=error):
        _wording_display(face, wording, revisions, current, {"jp": ["wording_pending"]})
