"""Exact secondary refusals; validated values isolate one boundary at a time."""

import dataclasses
import re
import shutil
from typing import TYPE_CHECKING

import pytest
from pydantic import BaseModel, JsonValue, ValidationError

from sve_carddb.registry.records import FaceData, PrintingData
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.text_observations.archive import FrozenTexts
from sve_carddb.wording_adoptions import loader, models, reconstruction, scope

from .test_wording_adoption_integration import adoption_case as adoption_case  # ruff: ignore[useless-import-alias] -- shared immutable inputs
from .wording_adoption_fixtures import commit, git

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.records import Region
    from sve_carddb.text_observations.models import TextCard

    from .wording_adoption_fixtures import AdoptionCase


@pytest.mark.parametrize(
    "change",
    [
        "keys",
        "objects",
        "presence",
        "answer",
        "indexes",
        "same-content",
        "adjacent",
        "range",
        "human",
        "mechanical-selection",
        "empty-level",
    ],
)
def test_secondary_model_refusals(  # ruff: ignore[complex-structure] -- table of independent closed-model violations
    adoption_case: AdoptionCase, change: str
) -> None:
    data = adoption_case.replayed[0].record.data
    model: type[BaseModel] | None = None
    wire: JsonValue = {}
    if change == "presence":
        model = models.Observation
        wire = data.observations[0].model_dump(mode="json")
        wire["source_index"] = data.observations[0].source_index + 1
    elif change == "answer":
        model = models.OrderReceipt
        wire = {
            "before_observation_keys": ["a"],
            "after_observation_keys": ["b"],
            "note": " ",
        }
    elif change in {"indexes", "same-content"}:
        model = models.PreviousOrder
        wire = {
            "basis": "printing_availability" if change == "indexes" else "same_content",
            "evidence_indexes": [1, 0] if change == "indexes" else [0],
            "review_receipt": None,
        }
    elif change == "adjacent":
        model = models.OrderEvidence
        wire = {
            "before_level": 0,
            "after_level": 2,
            "basis": "printing_availability",
            "evidence_indexes": [0],
            "review_receipt": None,
        }
    elif change == "range":
        model = models.RuleMatch
        wire = {
            "from_observation_key": "a",
            "to_observation_key": "b",
            "rule_id": "wp:eol-v1",
            "field": "text",
            "before_range": [2, 1],
            "after_range": [0, 1],
        }
    elif change == "human":
        model = models.Review
        wire = data.review.model_dump(mode="json") | {"mode": "human"}
    elif change in {"mechanical-selection", "empty-level"}:
        model = models.AdoptionData
        wire = data.model_dump(mode="json")
        if change == "mechanical-selection":
            object_value(wire["previous"])["selected_observation_key"] = (
                "synthetic-outside"
            )
        else:
            wire["wording_order"] = [[]]
    message = {
        "keys": "Receipt keys must be sorted and unique",
        "objects": "Receipt objects must be canonically sorted and unique",
        "presence": "Presence proof disagrees with observation source/index",
        "answer": "Order receipt requires an actual answer",
        "indexes": "Order evidence indexes must be sorted and unique",
        "same-content": "Same-content previous order has no evidence or answer",
        "adjacent": "Order evidence must join adjacent levels",
        "range": "Rule range must be a forward half-open interval",
        "human": "Human review cannot impersonate a policy receipt",
        "mechanical-selection": "Mechanical selected observation is outside its full root",
        "empty-level": "Wording order levels must be nonempty",
    }[change]
    if model is not None:
        with pytest.raises(ValidationError) as error:
            model.model_validate_json(canonical(wire))
        assert [e["msg"] for e in error.value.errors()] == ["Value error, " + message]
    elif change == "keys":
        with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
            models.ordered(("z", "a"))
    else:
        batch = data.review_context.source_batches[0]
        with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
            models.ordered_objects((batch, batch))


@pytest.mark.parametrize(
    "change", ["identity", "map", "raw", "scope", "confirmed", "version"]
)
def test_defensive_raw_scope_refusals(
    adoption_case: AdoptionCase, monkeypatch: pytest.MonkeyPatch, change: str
) -> None:
    case = adoption_case
    item = case.replayed[0].selected
    printing = case.scope.registry.records["printing:" + item.printing_id].data
    assert isinstance(printing, PrintingData)
    face = next(
        r.data
        for r in case.scope.registry.records.values()
        if isinstance(r.data, FaceData) and r.data.id == case.face_id
    )
    card = item.card
    registry = case.scope.registry
    provider = None
    if change == "identity":
        printing = printing.model_copy(update={"card_no": "Synthetic-other-number"})
    elif change == "map":
        printing = printing.model_copy(
            update={
                "source_face_map": tuple(
                    m.model_copy(update={"face_id": "f:" + "0" * 32})
                    for m in printing.source_face_map
                )
            }
        )
    elif change == "raw":
        card = card.model_copy(update={"raw": None, "effect_presence": ()})
    elif change == "scope":
        entry = registry.records["face:" + case.face_id]
        registry = dataclasses.replace(
            registry,
            records=dict(registry.records)
            | {
                entry.record_key: dataclasses.replace(
                    entry, data=face.model_copy(update={"card_id": "c:" + "0" * 32})
                )
            },
        )
    elif change == "confirmed":
        entry = registry.records["face:" + case.face_id]
        registry = dataclasses.replace(
            registry,
            records=dict(registry.records)
            | {entry.record_key: dataclasses.replace(entry, decision_id=None)},
        )
    else:
        provider = FrozenTexts(
            case.store,
            "wording-store",
            case.review.source_batches[0].batch_id,
            region="jp",
            parser_version="text-observations-v1",
        )
        original = provider.version

        def wrong_version(region: Region, number: str, version: str) -> TextCard:
            return original(region, number, version).model_copy(
                update={
                    "source": card.source.model_copy(
                        update={"id": "src:v1:" + "0" * 64}
                    )
                }
            )

        monkeypatch.setattr(provider, "version", wrong_version)
    message = {
        "identity": "Historical wording source identity/face inventory mismatch",
        "map": "Historical wording face map is not unique",
        "raw": "Historical wording requires reproducible raw/presence evidence",
        "scope": "Historical wording printing/card/face scope mismatch",
        "confirmed": "Historical wording identity scope is not confirmed",
        "version": "Historical wording source version mismatch",
    }[change]

    def operation() -> None:
        if change in {"identity", "map", "raw"}:
            scope._observe(printing, face, card)
        else:
            if change == "version":
                assert provider is not None
            scope.rebuild_raw_scope(
                registry,
                case.face_id,
                "jp",
                (provider,) if provider is not None else (),
            )

    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        operation()


@pytest.mark.parametrize("change", ["date", "entry"])
def test_configuration_scope_refusals(adoption_case: AdoptionCase, change: str) -> None:
    wire = object_value(parse(adoption_case.review.context.configuration.encode()))
    if change == "date":
        wire["errata_as_of"] = "20261002"
    else:
        object_value(wire["registry"])["index_path"] = "authored/wrong/index.yaml"
    message = (
        "Review errata cutoff must be a complete ISO date"
        if change == "date"
        else "Review authored entry path does not match its capability"
    )
    with pytest.raises(ValidationError) as error:
        reconstruction.Configuration.model_validate_json(canonical(wire))
    assert [e["msg"] for e in error.value.errors()] == ["Value error, " + message]


@pytest.mark.parametrize("change", ["store", "index", "region", "runtime", "git"])
def test_pinned_context_refusals(
    adoption_case: AdoptionCase,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    change: str,
) -> None:
    case = adoption_case
    rebuilt = reconstruction.Reconstructor(case.root, {"wording-store": case.store})
    config = reconstruction.Configuration.model_validate_json(
        case.review.context.configuration
    )
    revision = ""
    if change == "runtime":
        root = tmp_path / "repository"
        shutil.copytree(case.root, root)
        path = root / "carddb/src/sve_carddb/wording_adoptions/policy.py"
        path.write_bytes(
            path.read_bytes() + b"\n# Synthetic different historical implementation.\n"
        )
        revision = commit(root)
        rebuilt = reconstruction.Reconstructor(root, {})
    elif change == "git":
        monkeypatch.setattr(shutil, "which", lambda _name: None)
    message = {
        "store": "Source batch requires exactly one configured archive store",
        "index": "Reviewed authored index hash mismatch",
        "region": "Adoption region is outside the reviewed scope",
        "runtime": "Historical wording runtime cannot be replayed",
        "git": "Git is required for immutable adoption inputs",
    }[change]

    def operation() -> None:
        if change == "store":
            rebuilt.batch("sha256:" + "0" * 64)
        elif change == "index":
            rebuilt._inventory(
                tmp_path / "inventory",
                config.registry.model_copy(update={"index_hash": "sha256:" + "0" * 64}),
            )
        elif change == "region":
            rebuilt.scope(case.review, case.face_id, "en")
        elif change == "runtime":
            reconstruction.runtime_dependencies(rebuilt.repository, revision)
        else:
            loader._immutable(
                case.root / "authored",
                "wording-adoptions/index.yaml",
                case.snapshot.authored_revision,
                case.snapshot.index_bytes,
            )

    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        operation()


def test_reviewed_shard_path_is_refused_before_escaping_its_root(
    adoption_case: AdoptionCase, tmp_path: Path
) -> None:
    root = tmp_path / "repository"
    path = root / "authored/ids/index.yaml"
    path.parent.mkdir(parents=True)
    wire = object_value(read_yaml(adoption_case.root / "authored/ids/index.yaml"))
    wire["includes"] = {"../escape.yaml": "sha256:" + "0" * 64}
    path.write_bytes(canonical(wire))
    git(root, "init")
    revision = commit(root)
    config = reconstruction.Configuration.model_validate_json(
        adoption_case.review.context.configuration
    )
    pin = config.registry.model_copy(
        update={"authored_revision": revision, "index_hash": digest(canonical(wire))}
    )
    reconstructed = reconstruction.Reconstructor(root, {})
    with pytest.raises(ValueError, match=r"\AUnsafe reviewed authored shard path\Z"):
        reconstructed._inventory(tmp_path / "restored", pin)
    assert not (tmp_path / "escape.yaml").exists()


@pytest.mark.parametrize("change", ["face", "mapping"])
def test_loader_reference_refusals(adoption_case: AdoptionCase, change: str) -> None:
    case = adoption_case
    registry = case.scope.registry
    records = dict(registry.records)
    if change == "face":
        del records["face:" + case.face_id]
    else:
        entry = records["printing:" + case.replayed[0].selected.printing_id]
        assert isinstance(entry.data, PrintingData)
        records[entry.record_key] = dataclasses.replace(
            entry, data=entry.data.model_copy(update={"region": "en"})
        )
    registry = dataclasses.replace(registry, records=records)
    message = (
        "Adoption references an unregistered face"
        if change == "face"
        else "Adoption observation printing/face/region mapping mismatch"
    )
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        loader._references(
            dict(case.snapshot.records), dict(case.snapshot.record_decisions), registry
        )
