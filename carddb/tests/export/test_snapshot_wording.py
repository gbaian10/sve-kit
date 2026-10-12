"""Independent logical counterexamples for the public pending display contract."""

from copy import deepcopy
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.core.json import array, object_value, parse, string
from sve_carddb.export.reader import Fragment, _container, _current
from sve_carddb.export.semantics import (
    _annotation_partitions,
    _wording_display,
    validate_view,
)

from ..support.snapshot_contract_fixtures import fixture, payloads

if TYPE_CHECKING:
    from sve_carddb.export.reader import Row, View


@pytest.mark.parametrize(
    ("field", "partition"), [("name", "bootstrap"), ("effect", "detail")]
)
def test_candidate_original_annotations_are_available_without_display_or_current(
    field: str, partition: str
) -> None:
    view: View = {
        "face": [
            {
                "current": [],
                "wording": [
                    {
                        "display": {"revision_id": None},
                        "candidates": [{"revision_id": "own"}, {"revision_id": None}],
                    }
                ],
            }
        ],
        "face_revision": [{"id": "own", "name_concept_id": None}],
        "text_unit": [],
        "annotation_set": [],
    }
    fragment = Fragment(
        "synthetic",
        "field_annotation",
        {"partition": partition},
        [
            {
                "owner": {"kind": "face_revision", "id": "own"},
                "field": field,
                "annotation_set_id": "annotation",
            }
        ],
    )
    _annotation_partitions(view, [fragment])
    fragment.value["partition"] = "history"
    with pytest.raises(
        ValueError, match="Original annotation column partition mismatch"
    ):
        _annotation_partitions(view, [fragment])


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


def shared_basis_view(change: str | None = None) -> tuple[View, Row, list[Fragment]]:
    view, manifest, fragments = pending_view()
    face = view["face"][0]
    face["current"] = [
        {"region": "jp", "revision_id": "r:a2", "basis": "latest_known_release"}
    ]
    wording = object_value(array(face["wording"])[0])
    wording.update(
        display={"revision_id": "r:a2", "basis": "current"},
        classification="same_effect",
        method="ai",
        reason="Synthetic equivalence",
        basis_printing_id="p:a",
        basis_released_on="2026-01-01",
    )
    view["card_engine_support"][0]["region_blocks"] = []
    if change in {"newer_known", "manual_older"}:
        inclusion = view["printing_product"][0].copy()
        inclusion.update(
            printing_id="p:b", available_on="2027-01-01", date_precision="day"
        )
        view["printing_product"].append(inclusion)
        wording["undated_printing_ids"] = []
        if change == "manual_older":
            wording["method"] = "manual"
    elif change == "errata":
        wording["classification"] = "errata"
    elif change == "no_basis":
        wording.update(basis_printing_id=None, basis_released_on=None)
    elif change == "undated_basis":
        wording.update(basis_printing_id="p:b", basis_released_on=None)
    elif change == "wrong_day":
        wording["basis_released_on"] = "2025-12-31"
    return view, manifest, fragments


@pytest.mark.parametrize("change", [None, "manual_older"])
def test_shared_basis_links_a_dated_observation_to_current(change: str | None) -> None:
    validate_view(*shared_basis_view(change))


@pytest.mark.parametrize(
    ("change", "error"),
    [
        ("errata", "Non-equivalent wording cannot have a shared basis"),
        ("no_basis", "requires a basis printing"),
        ("undated_basis", "disagrees with the selected observation"),
        ("wrong_day", "release day disagrees"),
        ("newer_known", "must use the latest known release"),
    ],
)
def test_shared_basis_rejects_independent_public_constraint(
    change: str, error: str
) -> None:
    with pytest.raises(ValueError, match=error):
        validate_view(*shared_basis_view(change))


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
    exact["translations"] = deepcopy(latest["translations"])
    for item in array(exact["translations"]):
        translation = object_value(item)
        pointer = object_value(translation["source"])
        if pointer["owner"] == {"kind": "face_revision", "id": latest["id"]}:
            pointer["owner"] = {"kind": "face_revision", "id": "r:a1"}
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


@pytest.mark.parametrize(
    "mode", ["undated_full", "older_full", "empty_basis", "no_empty"]
)
def test_shared_basis_excludes_empty_printings_without_inventing_a_date(
    mode: str,
) -> None:
    view, manifest, fragments = shared_basis_view()
    face = view["face"][0]
    wording = object_value(array(face["wording"])[0])
    if mode != "no_empty":
        view["text_unit"].append(
            {"id": "t:ja:e3b0c44298fc1c14", "lang": "ja", "text": ""}
        )
        old = next(row for row in view["face_revision"] if row["id"] == "r:a1")
        old["effect_unit_id"] = "t:ja:e3b0c44298fc1c14"
        array(wording["candidates"])[1] = {"printing_id": "p:b", "revision_id": "r:a1"}
        physical = next(row for row in view["printing"] if row["id"] == "p:b")
        object_value(array(physical["faces"])[0])["observations"] = [
            {
                "revision_id": "r:a1",
                "state": "available",
                "source_url": "https://example.invalid/blank",
            }
        ]
        view["printing_product"].append(
            view["printing_product"][0]
            | {
                "printing_id": "p:b",
                "available_on": "2027-01-01",
                "date_precision": "day",
            }
        )
        wording["undated_printing_ids"] = []
    if mode in {"undated_full", "no_empty"}:
        view["product"][0].update(released_on=None, date_precision="unknown")
        wording["basis_released_on"] = None
        wording["undated_printing_ids"] = (
            ["p:a"] if mode != "no_empty" else ["p:a", "p:b"]
        )
        object_value(array(face["current"])[0])["basis"] = "latest_observed_no_errata"
    elif mode == "empty_basis":
        wording.update(
            basis_printing_id="p:b",
            basis_released_on="2027-01-01",
            display={"revision_id": "r:a1", "basis": "current"},
        )
        object_value(array(face["current"])[0])["revision_id"] = "r:a1"
    if mode == "empty_basis":
        with pytest.raises(ValueError, match="selected observation"):
            validate_view(view, manifest, fragments)
    elif mode == "no_empty":
        with pytest.raises(ValueError, match="latest known release"):
            validate_view(view, manifest, fragments)
    else:
        validate_view(view, manifest, fragments)
