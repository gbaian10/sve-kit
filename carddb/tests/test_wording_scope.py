"""Every frozen version participates; raw scopes never claim equivalence or current."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

import pytest

from sve_carddb.manifest import Kind
from sve_carddb.registry.records import PrintingData
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.text_observations import FrozenTexts
from sve_carddb.wording_adoptions.models import Observation
from sve_carddb.wording_adoptions.scope import rebuild_raw_scope, verify_raw_inventory

from .test_effect_presence import page
from .test_registry import make_inputs
from .test_source_archive import _put, _resource, _store
from .text_observation_fixtures import make_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.records import Region
    from sve_carddb.registry.snapshot import RegistrySnapshot
    from sve_carddb.text_observations.models import TextCard


@dataclass(frozen=True)
class Case:
    registry: RegistrySnapshot
    face_id: str
    store: Path
    batches: tuple[str, str]
    printings: tuple[PrintingData, ...]

    def provider(self, batch: str) -> FrozenTexts:
        return FrozenTexts(
            self.store,
            "test-store",
            batch,
            region="jp",
            parser_version="wording-scope-synthetic-v1",
        )


@pytest.fixture(scope="module")
def historical_case(tmp_path_factory: pytest.TempPathFactory) -> Case:
    root = tmp_path_factory.mktemp("wording-scope")
    base = make_case(root / "authored", make_inputs(), regions=("jp",))
    face = base.plan.groups[0].face_id
    printings = tuple(
        record.data
        for record in base.identity.snapshot.records.values()
        if isinstance(record.data, PrintingData)
        and record.data.region == "jp"
        and any(m.face_id == face for m in record.data.source_face_map)
    )
    assert len(printings) == 2
    store = _store(root / "historical")
    batches = []
    for generation in (0, 1):
        for index, printing in enumerate(printings):
            effect = (
                '<div class="detail">Synthetic old wording</div>'
                if generation == 0
                else '<div class="detail">Synthetic new wording</div>'
                if index == 0
                else "Synthetic unknown loose text"
            )
            raw = page("jp", effect).replace(b"SYN-01", printing.card_no.encode())
            _put(
                store,
                _resource(
                    card_url(printing.card_no), f"raw/{index}.html", raw, Kind.CARD
                ),
                raw,
            )
        batches.append(seal_batch(store).batch_id)
    return Case(
        base.identity.snapshot, face, store.root, (batches[0], batches[1]), printings
    )


def test_all_versions_survive_current_selection_and_unknown_effects(
    historical_case: Case,
) -> None:
    case = historical_case
    provider = case.provider(case.batches[1])
    provider.current.clear()
    scope = rebuild_raw_scope(case.registry, case.face_id, "jp", (provider,))
    assert len(scope.observations) == 4
    assert len(scope.uses) == 8
    assert sum(o.content.effect is None for o in scope.observations) == 1
    assert {o.printing_id for o in scope.observations} == {p.id for p in case.printings}
    assert not any(o.correction_keys for o in scope.observations)
    assert all(
        o.content == o.card.projected(o.source_index) for o in scope.observations
    )
    assert len({o.card.source.id for o in scope.observations}) == 4
    assert {use.usage for use in scope.uses} == {
        "wording_observation",
        "effect_presence",
    }


def test_overlapping_batches_deduplicate_observations_but_keep_full_batch_uses(
    historical_case: Case,
) -> None:
    case = historical_case
    providers = tuple(case.provider(batch) for batch in case.batches)
    scope = rebuild_raw_scope(case.registry, case.face_id, "jp", providers)
    assert len(scope.observations) == 4
    assert len(scope.uses) == 12
    assert {use.source.archive.batch_id for use in scope.uses} == set(case.batches)


@pytest.mark.parametrize(
    "change", ["face", "region", "missing", "identity", "map", "raw", "overlap"]
)
def test_bad_scope_inputs_fail_without_borrowing_another_source(
    historical_case: Case, monkeypatch: pytest.MonkeyPatch, change: str
) -> None:
    case = historical_case
    provider = case.provider(case.batches[1])
    providers: tuple[FrozenTexts, ...] = (provider,)
    registry = case.registry
    if change in {"identity", "map", "raw", "overlap"}:
        original = provider.version

        def altered(region: Region, number: str, version: str) -> TextCard:
            card = original(region, number, version)
            if change == "identity":
                return card.model_copy(
                    update={
                        "observation": card.observation.model_copy(
                            update={"card_no": "OTHER"}
                        )
                    }
                )
            if change == "map":
                return card.model_copy(
                    update={
                        "faces": (*card.faces, *card.faces),
                        "effect_presence": (
                            *card.effect_presence,
                            *card.effect_presence,
                        ),
                    }
                )
            if change == "raw":
                return card.model_copy(update={"raw": None})
            return card.model_copy(
                update={
                    "source": card.source.model_copy(
                        update={"parser_version": "different-parser"}
                    )
                }
            )

        if change == "overlap":
            providers = (case.provider(case.batches[1]), provider)
        monkeypatch.setattr(provider, "version", altered)
    with pytest.raises(
        ValueError, match=r"Historical wording|Overlapping batches|Effect presence"
    ):
        rebuild_raw_scope(
            registry,
            "f:" + "0" * 32 if change == "face" else case.face_id,
            "en" if change == "region" else "jp",
            () if change == "missing" else providers,
        )


@pytest.mark.parametrize(
    "change",
    ["none", "missing", "duplicate", "raw", "parser", "face", "presence", "index"],
)
def test_receipt_must_reproduce_every_historical_raw_pin(
    historical_case: Case, change: str
) -> None:
    case = historical_case
    scope = rebuild_raw_scope(
        case.registry, case.face_id, "jp", (case.provider(case.batches[1]),)
    )
    observations = tuple(
        Observation(
            observation_key="synthetic-key-checked-separately-by-receipt-model",
            printing_id=item.printing_id,
            source_index=item.source_index,
            source_version_id=item.card.source.id,
            raw_hash=item.card.source.sha256,
            parser_version=item.card.source.parser_version,
            raw_face_hash=item.card.faces[item.source_index].fingerprint(),
            effect_presence=item.card.effect_presence[item.source_index],
            corrections=(),
            content_hash=item.content.fingerprint(),
        )
        for item in scope.observations
    )
    if change == "missing":
        observations = observations[:-1]
    elif change == "duplicate":
        observations = (*observations, observations[0])
    elif change in {"raw", "parser", "face", "index", "presence"}:
        before = observations[0]
        updated = (
            before.model_copy(update={"raw_hash": "sha256:" + "0" * 64})
            if change == "raw"
            else before.model_copy(update={"parser_version": "different-parser"})
            if change == "parser"
            else before.model_copy(update={"raw_face_hash": "sha256:" + "0" * 64})
            if change == "face"
            else before.model_copy(update={"source_index": 1})
            if change == "index"
            else before.model_copy(
                update={"effect_presence": observations[-1].effect_presence}
            )
        )
        observations = (updated, *observations[1:])
    if change == "none":
        verify_raw_inventory(scope, observations)
    else:
        with pytest.raises(ValueError, match=r"full historical|cannot be reproduced"):
            verify_raw_inventory(scope, observations)
