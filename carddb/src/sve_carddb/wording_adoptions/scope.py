"""Rebuild the complete raw face/region scope, before corrections or adoption."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.build_inputs import SourceUse, uses_sorted
from sve_carddb.registry.records import FaceData, PrintingData
from sve_carddb.snapshot.values import canonical
from sve_carddb.text_observations.archive import verify_card
from sve_carddb.text_observations.models import FaceObservation

if TYPE_CHECKING:
    from sve_carddb.registry.records import Region
    from sve_carddb.registry.snapshot import RegistrySnapshot
    from sve_carddb.text_observations.archive import FrozenTexts
    from sve_carddb.text_observations.models import TextCard
    from sve_carddb.wording_adoptions.models import Observation


@dataclass(frozen=True)
class RawScope:
    observations: tuple[FaceObservation, ...]
    uses: tuple[SourceUse, ...]


def _same_version(before: TextCard, after: TextCard) -> bool:
    return (
        before.source.values() == after.source.values()
        and before.source.parser_version == after.source.parser_version
        and before.observation == after.observation
        and before.faces == after.faces
        and before.effect_presence == after.effect_presence
    )


def _observe(printing: PrintingData, face: FaceData, card: TextCard) -> FaceObservation:
    verify_card(card)
    mappings = printing.source_face_map
    if (
        (card.observation.region, card.observation.card_no)
        != (printing.region, printing.card_no)
        or {m.source_index for m in mappings} != set(range(len(card.faces)))
        or len(mappings) != len(card.faces)
    ):
        raise ValueError("Historical wording source identity/face inventory mismatch")
    indexes = [m.source_index for m in mappings if m.face_id == face.id]
    if len(indexes) != 1:
        raise ValueError("Historical wording face map is not unique")
    index = indexes[0]
    if len(card.effect_presence) != len(card.faces) or card.raw is None:
        raise ValueError(
            "Historical wording requires reproducible raw/presence evidence"
        )
    return FaceObservation(
        card_id=printing.card_id,
        printing_id=printing.id,
        face_id=face.id,
        region=printing.region,
        card_no=printing.card_no,
        source_index=index,
        card=card,
        content=card.projected(index),
    )


def rebuild_raw_scope(
    registry: RegistrySnapshot,
    face_id: str,
    region: Region,
    providers: tuple[FrozenTexts, ...],
) -> RawScope:
    """Keep every version of every registered printing, including missing/null text.

    The caller must independently replay the review context and registry pins. This
    raw scope is not corrected content, an equivalence decision or a current choice.
    """
    faces = [
        record.data
        for record in registry.records.values()
        if isinstance(record.data, FaceData) and record.data.id == face_id
    ]
    if len(faces) != 1:
        raise ValueError("Historical wording face is absent from the reviewed registry")
    face = faces[0]
    printings = sorted(
        (
            record.data
            for record in registry.records.values()
            if isinstance(record.data, PrintingData)
            and record.data.region == region
            and any(
                mapping.face_id == face_id for mapping in record.data.source_face_map
            )
        ),
        key=lambda printing: printing.id,
    )
    if not printings or any(printing.card_id != face.card_id for printing in printings):
        raise ValueError("Historical wording printing/card/face scope mismatch")
    required = {"card:" + face.card_id, "face:" + face.id} | {
        "printing:" + p.id for p in printings
    }
    if any(
        registry.records[key].decision_id is None
        or registry.decisions[registry.records[key].decision_id or ""].state
        != "confirmed"
        for key in required
    ):
        raise ValueError("Historical wording identity scope is not confirmed")
    observations: list[FaceObservation] = []
    uses: list[SourceUse] = []
    for printing in printings:
        versions: dict[str, FaceObservation] = {}
        for provider in providers:
            for version in provider.versions(region, printing.card_no):
                card = provider.version(region, printing.card_no, version)
                if version != card.source.id:
                    raise ValueError("Historical wording source version mismatch")
                item = _observe(printing, face, card)
                if version in versions and not _same_version(
                    versions[version].card, card
                ):
                    raise ValueError(
                        "Overlapping batches disagree about a wording source"
                    )
                versions.setdefault(version, item)
                uses.append(
                    SourceUse(
                        source=card.source,
                        usage="wording_observation",
                        locator=canonical(
                            {
                                "printing_id": printing.id,
                                "face_id": face.id,
                                "source_index": item.source_index,
                            }
                        ).decode(),
                    )
                )
                proof = card.effect_presence[item.source_index]
                uses.append(
                    SourceUse(
                        source=card.source.model_copy(
                            update={"parser_version": proof.result.parser_version}
                        ),
                        usage="effect_presence",
                        locator=canonical(
                            {
                                "printing_id": printing.id,
                                "face_id": face.id,
                                "source_index": item.source_index,
                                "recipe": proof.result.recipe,
                                "template_id": proof.result.template_id,
                                "container_locator": proof.result.container_locator,
                                "state": proof.result.state,
                                "reason_code": proof.result.reason_code,
                                "result_hash": proof.result_hash,
                            }
                        ).decode(),
                    )
                )
        if not versions:
            raise ValueError(
                "Historical wording is missing a registered printing source"
            )
        observations.extend(versions[version] for version in sorted(versions))
    return RawScope(tuple(observations), uses_sorted(uses))


def verify_raw_inventory(
    scope: RawScope, observations: tuple[Observation, ...]
) -> None:
    """Check all raw/projection pins before separately verifying corrected content."""
    expected = {
        (item.printing_id, item.card.source.id, item.source_index): item
        for item in scope.observations
    }
    actual = {
        (item.printing_id, item.source_version_id, item.source_index): item
        for item in observations
    }
    if len(actual) != len(observations) or actual.keys() != expected.keys():
        raise ValueError(
            "Wording receipt does not cover the full historical raw inventory"
        )
    for key, observation in actual.items():
        item = expected[key]
        if (
            observation.raw_hash != item.card.source.sha256
            or observation.parser_version != item.card.source.parser_version
            or observation.raw_face_hash
            != item.card.faces[item.source_index].fingerprint()
            or observation.effect_presence
            != item.card.effect_presence[item.source_index]
        ):
            raise ValueError("Wording receipt raw/presence pins cannot be reproduced")
