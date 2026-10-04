"""Complete receipt replay, explicit ordering and predecessor validation."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.products.models import InclusionRecord, ProductRecord
from sve_carddb.snapshot.values import canonical
from sve_carddb.wording_adoptions.models import Mechanical, PreviousAdoption
from sve_carddb.wording_adoptions.policy import (
    equivalent_matches,
    load_policy,
    verify_matches,
    verify_policy_reviewer,
)
from sve_carddb.wording_adoptions.reconstruction import scope_evidence

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.products.models import Evidence
    from sve_carddb.text_observations.models import FaceObservation
    from sve_carddb.wording_adoptions.loader import AdoptionSnapshot
    from sve_carddb.wording_adoptions.models import AdoptionRecord, Decision
    from sve_carddb.wording_adoptions.reconstruction import (
        ReconstructedScope,
        Reconstructor,
    )


@dataclass(frozen=True)
class ReplayedAdoption:
    record: AdoptionRecord
    decision: Decision
    scope: ReconstructedScope
    selected: FaceObservation
    previous: FaceObservation | None
    predecessor_scope: ReconstructedScope | None
    basis: str
    policy_files: Mapping[str, bytes]


def _membership(record: AdoptionRecord, scope: ReconstructedScope) -> None:
    if any(item.content.effect is None for item in scope.contents.values()):
        raise ValueError("Unknown effect cannot confirm wording equivalence")
    if record.data.observations != scope.observations:
        raise ValueError("Adoption does not reproduce the complete corrected inventory")
    if not set(scope_evidence(scope)) <= set(record.evidence):
        raise ValueError("Adoption evidence omits raw, presence or correction closure")
    for level in record.data.wording_order:
        if (
            len(
                {
                    canonical(scope.contents[key].content.wording_fields())
                    for key in level
                }
            )
            != 1
        ):
            raise ValueError(
                "One wording level contains different reconstructed contents"
            )


def _mechanical(
    record: AdoptionRecord, reconstruction: Reconstructor
) -> tuple[FaceObservation | None, ReconstructedScope | None]:
    data = record.data
    previous = data.previous
    if not isinstance(previous, Mechanical):
        return None, None
    scope = reconstruction.scope(previous.review_context, data.face_id, data.region)
    if (
        previous.observations != scope.observations
        or any(item.content.effect is None for item in scope.contents.values())
        or len({canonical(o.content.wording_fields()) for o in scope.contents.values()})
        != 1
    ):
        raise ValueError(
            "Mechanical predecessor is not the complete exact initial current"
        )
    selected = min(
        scope.contents.items(),
        key=lambda pair: (pair[1].card.source.id, pair[1].printing_id),
    )
    if previous.selected_observation_key != selected[0]:
        raise ValueError(
            "Mechanical predecessor selected representative cannot be replayed"
        )
    if not set(scope_evidence(scope)) <= set(record.evidence):
        raise ValueError("Mechanical predecessor source closure is missing")
    return selected[1], scope


def _availability(
    scope: ReconstructedScope, observation: FaceObservation
) -> tuple[str, set[Evidence]]:
    catalog = scope.products
    if catalog is None:
        raise ValueError("Printing order requires a pinned formal product catalog")
    adopted = {
        r.record_key
        for shard in catalog.shards
        if shard.envelope.decisions[0].state == "confirmed"
        for r in shard.envelope.records
    }
    inclusions = [
        r
        for r in catalog.records.values()
        if isinstance(r, InclusionRecord)
        and r.data.printing_id == observation.printing_id
    ]
    if not inclusions or any(r.record_key not in adopted for r in inclusions):
        raise ValueError("First availability has missing or unconfirmed inclusions")
    dates = []
    evidence: set[Evidence] = set()
    for inclusion in inclusions:
        product = catalog.records[
            canonical(["product", inclusion.data.product_id]).decode()
        ]
        if (
            not isinstance(product, ProductRecord)
            or product.record_key not in adopted
            or product.data.region != observation.region
        ):
            raise ValueError(
                "Printing order product is unconfirmed or in another region"
            )
        data = inclusion.data
        precision = (
            data.first_available_precision
            if data.first_available_precision is not None
            else product.data.date_precision
        )
        value = (
            data.first_available_on
            if data.first_available_precision is not None
            else product.data.released_on
        )
        if precision != "day" or value is None:
            raise ValueError(
                "Unknown, month or year precision cannot establish first availability"
            )
        dates.append(value)
        evidence.update((*inclusion.evidence, *product.evidence))
    return min(dates), evidence


def _official_order(
    record: AdoptionRecord,
    scope: ReconstructedScope,
    before: tuple[FaceObservation, ...],
    after: tuple[FaceObservation, ...],
    basis: str,
    indexes: tuple[int, ...],
) -> None:
    if basis == "source_update":
        raise ValueError("Official source-update recipe is explicitly unimplemented")
    if basis != "printing_availability":
        return
    old = [_availability(scope, item) for item in before]
    new = [_availability(scope, item) for item in after]
    if max(day for day, _ in old) >= min(day for day, _ in new):
        raise ValueError(
            "Official evidence does not prove strict adjacent wording order"
        )
    required = set().union(*(proof for _, proof in (*old, *new)))
    supplied = {record.evidence[i] for i in indexes}
    if supplied != required:
        raise ValueError(
            "Order evidence must cover exactly every first-availability source"
        )


def _order(
    record: AdoptionRecord,
    scope: ReconstructedScope,
    previous: FaceObservation | None,
    previous_key: str | None,
) -> str:
    data = record.data
    override = False
    for edge in data.order_evidence:
        _official_order(
            record,
            scope,
            tuple(scope.contents[k] for k in data.wording_order[edge.before_level]),
            tuple(scope.contents[k] for k in data.wording_order[edge.after_level]),
            edge.basis,
            edge.evidence_indexes,
        )
        override |= edge.basis == "reviewed_order"
    if previous is not None:
        proof = data.previous_order
        if proof is None:
            raise ValueError("Predecessor requires independent ordering evidence")
        selected = scope.contents[data.selected_observation_key]
        if proof.basis == "same_content":
            if previous.content.wording_fields() != selected.content.wording_fields():
                raise ValueError(
                    "Same-content predecessor order differs in exact content"
                )
        elif proof.basis == "reviewed_order":
            answer = proof.review_receipt
            if (
                answer is None
                or answer.before_observation_keys != (previous_key,)
                or answer.after_observation_keys != (data.selected_observation_key,)
            ):
                raise ValueError(
                    "Predecessor order answer does not bind both selected keys"
                )
            override = True
        else:
            _official_order(
                record,
                scope,
                (previous,),
                (selected,),
                proof.basis,
                proof.evidence_indexes,
            )
    return "reviewed_override" if override else "latest_adopted_wording"


def replay_adoptions(
    snapshot: AdoptionSnapshot, reconstruction: Reconstructor
) -> tuple[ReplayedAdoption, ...]:
    """Reconstruct a complete confirmed history without reading a previous dist DB."""
    result: dict[str, ReplayedAdoption] = {}
    for record in sorted(
        snapshot.records.values(),
        key=lambda r: (r.data.face_id, r.data.region, r.data.adoption_no),
    ):
        data = record.data
        scope = reconstruction.scope(data.review_context, data.face_id, data.region)
        _membership(record, scope)
        previous, previous_scope = _mechanical(record, reconstruction)
        previous_key = (
            data.previous.selected_observation_key
            if isinstance(data.previous, Mechanical)
            else None
        )
        if isinstance(data.previous, PreviousAdoption):
            prior = result.get(data.previous.record_key)
            if prior is None:
                raise ValueError("Adoption predecessor has not been replayed")
            previous, previous_key = (
                prior.selected,
                prior.record.data.selected_observation_key,
            )
            previous_scope = prior.scope
        elif (
            data.previous is None
            and len(
                {canonical(o.content.wording_fields()) for o in scope.contents.values()}
            )
            == 1
        ):
            raise ValueError(
                "Null predecessor cannot discard an available mechanical current"
            )
        batches = {b.batch_id for b in data.review_context.source_batches}
        if previous_scope is not None and isinstance(data.previous, Mechanical):
            batches.update(
                b.batch_id for b in data.previous.review_context.source_batches
            )
        if isinstance(data.previous, PreviousAdoption):
            batches.update(
                e.batch_id for e in result[data.previous.record_key].record.evidence
            )
        if any(e.batch_id not in batches for e in record.evidence):
            raise ValueError(
                "Evidence batch is outside its corresponding reviewed contexts"
            )
        decision = snapshot.decisions[snapshot.record_decisions[record.record_key]]
        policy_files = _review(record, scope, decision, previous, reconstruction)
        result[record.record_key] = ReplayedAdoption(
            record,
            decision,
            scope,
            scope.contents[data.selected_observation_key],
            previous,
            previous_scope,
            _order(record, scope, previous, previous_key),
            policy_files,
        )
    return tuple(result.values())


def _review(
    record: AdoptionRecord,
    scope: ReconstructedScope,
    decision: Decision,
    previous: FaceObservation | None,
    reconstruction: Reconstructor,
) -> Mapping[str, bytes]:
    """Keep policy approval provenance separate from a named human answer."""
    data = record.data
    policy_files: Mapping[str, bytes] = {}
    if data.review.mode == "approved_rules":
        if data.review.rule_set is None:
            raise ValueError("Approved-rule adoption has no policy pin")
        policy, approval, policy_files = load_policy(
            reconstruction.repository, data.review.rule_set
        )
        verify_policy_reviewer(approval, decision)
        verify_matches(
            data.review.rule_matches,
            equivalent_matches(
                policy,
                {key: item.content for key, item in scope.contents.items()},
                data.selected_observation_key,
                region=data.region,
                previous=None if previous is None else previous.content,
            ),
        )
    else:
        selected = scope.contents[data.selected_observation_key].content
        protected = selected.wording_fields() | {"text": None, "sections": []}
        candidates = [o.content for o in scope.contents.values()]
        if previous is not None:
            candidates.append(previous.content)
        if any(
            (c.wording_fields() | {"text": None, "sections": []}) != protected
            for c in candidates
        ):
            raise ValueError(
                "Wording equivalence cannot override rule dependencies or identity"
            )
    return policy_files
