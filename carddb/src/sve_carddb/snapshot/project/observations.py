"""Project frozen observations and externally adopted wording without choosing chronology."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import array, canonical, object_value, string

if TYPE_CHECKING:
    from sve_carddb.snapshot.project.evidence import Decisions
    from sve_carddb.snapshot.project.source import Record, Source


def observations(
    source: Source, view: dict[str, list[Record]], decisions: Decisions
) -> None:
    """Unknown main text is missing_effect, never an empty effect string."""
    for printing in view["printing"]:
        for raw_face in array(printing["faces"]):
            face = object_value(raw_face)
            records: dict[bytes, Record] = {}
            parent = string(printing["id"]), string(face["face_id"])
            rows = (
                []
                if parent in decisions.observed_texts
                else source.matching(
                    "printing_face_observation",
                    "printing_id,face_id,source_id,revision_id",
                    printing_id=printing["id"],
                    face_id=face["face_id"],
                )
            )
            for row in rows:
                key = (
                    string(printing["id"]),
                    string(face["face_id"]),
                    string(row["source_id"]),
                )
                state = decisions.observation_states.get(
                    key, "missing_effect" if row["revision_id"] is None else "available"
                )
                if state not in {"available", "missing_effect", "correction_conflict"}:
                    raise ValueError("Unknown observation state")
                if (state == "available" and row["revision_id"] is None) or (
                    state == "missing_effect" and row["revision_id"] is not None
                ):
                    raise ValueError("Observation state/revision mismatch")
                item: Record = {
                    "revision_id": row["revision_id"],
                    "state": state,
                    "source_url": source.url(row["source_id"]),
                }
                records[canonical(item)] = item
            if parent in decisions.observed_texts:
                records = {
                    canonical(item): dict(item)
                    for item in decisions.observed_texts[parent]
                }
            ordered = sorted(
                records.values(),
                key=lambda row: (
                    string(row["source_url"]),
                    string(row["state"]),
                    "" if row["revision_id"] is None else string(row["revision_id"]),
                ),
            )
            face["observations"] = list(ordered)
    selected_regions = {row["region"] for row in view["printing"]}
    for face in view["face"]:
        face["wording"] = [
            dict(row)
            for row in decisions.wording.get(string(face["id"]), ())
            if row["region"] in selected_regions
        ]
    validate_wording(view)


def _candidates(
    face: Record, wording: Record, printings: dict[str, Record]
) -> set[tuple[str, str | None]]:
    result: set[tuple[str, str | None]] = set()
    for raw in array(wording["candidates"]):
        candidate = object_value(raw)
        if set(candidate) != {"printing_id", "revision_id"}:
            raise ValueError("Wording candidate whitelist mismatch")
        identifier = string(candidate["printing_id"])
        revision_id = (
            None
            if candidate["revision_id"] is None
            else string(candidate["revision_id"])
        )
        marker = identifier, revision_id
        if marker in result:
            raise ValueError("Duplicate wording candidate")
        result.add(marker)
        printing = printings[identifier]
        if printing["region"] != wording["region"]:
            raise ValueError("Wording candidate crosses region")
        owners = [
            object_value(item)
            for item in array(printing["faces"])
            if object_value(item)["face_id"] == face["id"]
        ]
        if len(owners) != 1 or not any(
            object_value(item)["revision_id"] == revision_id
            for item in array(owners[0]["observations"])
        ):
            raise ValueError("Wording candidate has no matching observation")
    if not result:
        raise ValueError("Pending wording requires candidates")
    return result


def _display(
    face: Record,
    wording: Record,
    candidates: set[tuple[str, str | None]],
    revisions: dict[str, Record],
) -> None:
    display = object_value(wording["display"])
    if set(display) != {"revision_id", "basis"} or display["basis"] not in {
        "current",
        "latest_known_release",
        "candidates",
    }:
        raise ValueError("Wording display whitelist mismatch")
    if (display["basis"] == "candidates") != (display["revision_id"] is None):
        raise ValueError("Wording display basis/revision mismatch")
    current = [
        object_value(item)
        for item in array(face["current"])
        if object_value(item)["region"] == wording["region"]
    ]
    if current and (
        display["basis"] != "current"
        or display["revision_id"] != current[0]["revision_id"]
    ):
        raise ValueError("Wording display is not the adopted current")
    if display["basis"] == "current" and not current:
        raise ValueError("Wording display is not the adopted current")
    if display["revision_id"] is not None:
        revision = revisions[string(display["revision_id"])]
        if revision["face_id"] != face["id"] or revision["region"] != wording["region"]:
            raise ValueError("Wording display crosses face/region")
        if not current and not any(
            revision_id == display["revision_id"] for _, revision_id in candidates
        ):
            raise ValueError("Wording display outside candidates")


def validate_wording(view: dict[str, list[Record]]) -> None:
    """Validate adapter shape and same-printing/face/region public linkage."""
    revisions = {string(row["id"]): row for row in view["face_revision"]}
    printings = {string(row["id"]): row for row in view["printing"]}
    _observed_revisions(view, revisions)
    _wording_completeness(view)
    for face in view["face"]:
        regions: set[str] = set()
        for raw in array(face["wording"]):
            wording = object_value(raw)
            if (
                set(wording)
                != {"region", "state", "display", "candidates", "undated_printing_ids"}
                or wording["state"] != "pending"
            ):
                raise ValueError("Wording whitelist/state mismatch")
            region = string(wording["region"])
            if region not in {"jp", "en"} or region in regions:
                raise ValueError("Wording regions must be unique")
            regions.add(region)
            candidates = _candidates(face, wording, printings)
            _display(face, wording, candidates, revisions)
            undated = list(map(string, array(wording["undated_printing_ids"])))
            if undated != sorted(set(undated)) or not set(undated) <= {
                printing for printing, _ in candidates
            }:
                raise ValueError("Undated wording printing outside candidates")
            wording["candidates"] = [
                {"printing_id": printing, "revision_id": revision}
                for printing, revision in sorted(
                    candidates, key=lambda pair: (pair[0], pair[1] or "")
                )
            ]
        face["wording"] = sorted(
            array(face["wording"]), key=lambda raw: string(object_value(raw)["region"])
        )


def _observed_revisions(
    view: dict[str, list[Record]], revisions: dict[str, Record]
) -> None:
    for printing in view["printing"]:
        for raw in array(printing["faces"]):
            face = object_value(raw)
            for item in array(face["observations"]):
                observation = object_value(item)
                if set(observation) != {"revision_id", "state", "source_url"}:
                    raise ValueError("Observation whitelist mismatch")
                if observation["state"] not in {
                    "available",
                    "missing_effect",
                    "correction_conflict",
                }:
                    raise ValueError("Unknown observation state")
                if (
                    observation["state"] == "available"
                    and observation["revision_id"] is None
                ) or (
                    observation["state"] == "missing_effect"
                    and observation["revision_id"] is not None
                ):
                    raise ValueError("Observation state/revision mismatch")
                string(observation["source_url"])
                if observation["revision_id"] is not None:
                    revision = revisions[string(observation["revision_id"])]
                    if (
                        revision["face_id"] != face["face_id"]
                        or revision["region"] != printing["region"]
                    ):
                        raise ValueError("Observed revision crosses face/region")


def corrections(source: Source, view: dict[str, list[Record]]) -> None:
    """A correction marker belongs only to successfully changed reference uses."""
    records = source.index(
        "source_correction",
        "id,printing_id,face_id,field,expected_raw_value,reason,state",
    )
    printings = {string(row["id"]): row for row in view["printing"]}
    revisions = {string(row["id"]): row for row in view["face_revision"]}
    for application in source.rows(
        "correction_application", "correction_id,source_id,face_revision_id,status"
    ):
        if application["status"] != "applied":
            continue
        correction = records[string(application["correction_id"])]
        if correction["state"] not in {"active", "upstream_fixed"}:
            raise ValueError("Applied correction is not adopted")
        printing = printings.get(string(correction["printing_id"]))
        if printing is None:
            continue
        marker: Record = {
            "field": correction["field"],
            "corrected_from": correction["expected_raw_value"],
            "is_corrected": True,
            "reason": correction["reason"],
            "source_url": source.url(application["source_id"]),
        }
        revision_id = application["face_revision_id"]
        if revision_id is not None:
            revision = revisions[string(revision_id)]
            if (
                revision["face_id"] != correction["face_id"]
                or revision["region"] != printing["region"]
            ):
                raise ValueError("Correction revision crosses source owner")
            array(revision["corrections"]).append(dict(marker))
        for raw in array(printing["faces"]):
            face = object_value(raw)
            if face["face_id"] == correction["face_id"]:
                array(face["corrections"]).append(dict(marker))


def _wording_completeness(view: dict[str, list[Record]]) -> None:
    observed = {
        (object_value(raw)["face_id"], printing["region"])
        for printing in view["printing"]
        for raw in array(printing["faces"])
        if array(object_value(raw)["observations"])
    }
    for face in view["face"]:
        known = {object_value(raw)["region"] for raw in array(face["current"])} | {
            object_value(raw)["region"] for raw in array(face["wording"])
        }
        if any(
            face_id == face["id"] and region not in known
            for face_id, region in observed
        ):
            raise ValueError("Unadopted observations require wording projection")
