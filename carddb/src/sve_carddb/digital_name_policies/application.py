"""Compose checked name policy, owner semantics and actual human selection membership."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from pydantic import JsonValue, TypeAdapter, ValidationError

from sve_carddb.build_db import Json
from sve_carddb.build_db.rows import insert_exact
from sve_carddb.build_inputs import input_record, insert_raw_sources, uses_sorted
from sve_carddb.digital_links.importer import review_context
from sve_carddb.digital_links.loader import link_id
from sve_carddb.digital_name_policies.evaluate import (
    catalogue,
    historical_sources,
    name_result,
    owner_text,
)
from sve_carddb.digital_name_policies.loader import load
from sve_carddb.digital_name_policies.owners import publication_owners
from sve_carddb.digital_name_policies.runtime import require_runtime
from sve_carddb.registry.records import Instant
from sve_carddb.snapshot.project.evidence import DisplayBinding
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.translations.importer import validate_choice
from sve_carddb.translations.loader import record_hash
from sve_carddb.translations.models import ChoiceRecord
from sve_carddb.translations.name_build import bind_name_use, name_context, name_source
from sve_carddb.translations.name_materialization import materialize_name
from sve_carddb.translations.name_selection import NameCandidate, select_owner_name
from sve_carddb.translations.names import _proof

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import BuildContext, InputRecord, SourceUse
    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.digital_links.importer import Result as LinkResult
    from sve_carddb.digital_name_policies.evaluate import Catalogue, NamePolicyResult
    from sve_carddb.digital_name_policies.loader import Snapshot
    from sve_carddb.text_observations.plan import TextPlan
    from sve_carddb.translations.name_build import NameOwner, NameSource
    from sve_carddb.translations.name_replay import NameReplay
    from sve_carddb.translations.name_selection import NameSelection
    from sve_carddb.translations.sources import Sources


@dataclass(frozen=True)
class Inputs:
    root: Path
    repository: Path
    authored_revision: str
    application_at: str = ""

    def __post_init__(self) -> None:
        """Reject missing audit time before reading any policy or archive."""
        if not self.application_at and self.load().current_names:
            return
        try:
            TypeAdapter(Instant, config={"regex_engine": "python-re"}).validate_python(
                self.application_at
            )
        except ValidationError:
            raise ValueError(
                "Name application requires an explicit UTC instant"
            ) from None

    def load(self) -> Snapshot:
        """Read the entire immutable authored closure, including historical versions."""
        return load(self.root, self.repository, self.authored_revision)

    def configuration(self) -> dict[str, JsonValue]:
        """Declare the selected policy and the explicit first-application baseline."""
        snapshot = self.load()
        if snapshot.current_names:
            return {
                "digital_name_application": {
                    "recipe": "owner-name-current-v2",
                    **snapshot.pins(),
                }
            }
        return {
            "digital_name_application": {
                "recipe": "owner-name-v1",
                "baseline": "explicit-empty-first-application",
                "application_at": self.application_at,
                "policy": digest(snapshot.effective("names").policy),
                **snapshot.pins(),
            }
        }


@dataclass(frozen=True)
class OwnerPlan:
    source: NameSource
    result: NamePolicyResult
    variant: str
    variant_decision: str | None
    selection: NameSelection
    semantic_reason: str


@dataclass(frozen=True)
class Plan:
    context: BuildContext
    snapshot: Snapshot
    owners: tuple[OwnerPlan, ...]
    uses: tuple[SourceUse, ...]
    unknown_owners: tuple[NameOwner, ...] = ()

    def report(self) -> dict[str, JsonValue]:
        """Report identifiers and exact hashes only, never official text."""
        rows: list[JsonValue] = []
        for owner in self.owners:
            selected = owner.selection.candidate
            rows.append(
                {
                    "owner": owner.source.owner.payload(),
                    "card_id": owner.source.card_id,
                    "face_id": owner.source.face_id,
                    "source_name_hash": owner.source.source_hash,
                    "policy_condition": owner.result.condition,
                    "semantic_reason": owner.semantic_reason,
                    "selection": owner.selection.reason,
                    "origin": None if selected is None else selected.origin,
                    "selected_name_hash": None
                    if selected is None
                    else digest(selected.text.encode()),
                    "different_name_hashes": list[JsonValue](
                        owner.selection.differences
                    ),
                }
            )
        return {
            "baseline": "explicit-empty-first-application",
            "owners": rows,
            "newly_covered_owners": sum(
                o.selection.candidate is not None for o in self.owners
            ),
            "policy_covered_owners": sum(
                o.selection.reason == "policy" for o in self.owners
            ),
            "policy_eligible_owners": sum(
                o.result.status == "eligible" for o in self.owners
            ),
            "human_first_owners": sum(
                o.selection.reason == "human" for o in self.owners
            ),
            "untranslated_owners": sum(
                o.selection.candidate is None for o in self.owners
            ),
            "excluded_owners": sum(o.result.status == "excluded" for o in self.owners),
            "warning_owners": sum(
                bool(o.selection.differences)
                or o.semantic_reason == "ambiguous_name_concept"
                or o.result.condition == "nonunique_or_missing_translation"
                for o in self.owners
            ),
            "by_game": {
                game: sum(
                    o.selection.candidate is not None
                    and o.selection.candidate.origin == "official_" + game
                    for o in self.owners
                )
                for game in ("sv1", "svwb")
            },
            "historical_warnings_unavailable": True,
            "human_checked_policy_members": 0,
            "unknown_owners": [owner.payload() for owner in self.unknown_owners],
        }


@dataclass(frozen=True)
class Result:
    record: InputRecord
    bindings: tuple[DisplayBinding, ...]
    report: dict[str, JsonValue]


def prepare(  # ruff: ignore[too-many-locals] -- policy histories and owner publication are independent verification stages
    db: Database,
    inputs: Inputs,
    texts: TextPlan,
    *,
    sources: Sources,
    replay: NameReplay,
    links: LinkResult | None = None,
) -> Plan:
    """Replay immutable policies before inspecting per-owner name candidates."""
    require_runtime(sources)
    snapshot = inputs.load()
    config = object_value(parse(sources.build.configuration.encode()))
    if (
        config.get("digital_name_application")
        != inputs.configuration()["digital_name_application"]
    ):
        raise ValueError("Build configuration does not pin complete name policy inputs")
    uses: list[SourceUse] = list(replay.uses)
    loaded = snapshot.effective("names")
    frozen: Catalogue | None = None
    terminal_sources: Sources | None = None
    for policy in snapshot.policies:
        if policy.document().purpose != "names":
            continue
        historical = historical_sources(policy, sources.stores, inputs.repository)
        checked = catalogue(policy, historical)
        uses.extend(checked.uses)
        if policy == loaded:
            frozen = checked
            terminal_sources = historical
    assert frozen is not None
    assert terminal_sources is not None
    owners = []
    unknown_owners = []
    review = review_context(sources)
    for owner, policy_owner in publication_owners(db, texts):
        evidence = owner_text(policy_owner, sources, review)
        result = name_result(evidence, frozen)
        source = name_source(db, owner)
        if source is None:
            unknown_owners.append(owner)
            continue
        if evidence.text != source.text:
            raise ValueError("Name policy evidence differs from the materialized owner")
        resolved = replay.resolve(db, owner)
        assert resolved is not None
        counterparts = _counterparts(
            db, sources, owner, links, name_ref=policy_owner.name_ref
        )
        choices = _choices(db, sources, replay, source, resolved.term_id, counterparts)
        candidate = _policy_candidate(result, frozen, terminal_sources)
        selection = select_owner_name(
            source,
            result,
            context_hash=evidence.context_hash,
            choices=choices,
            counterparts=counterparts,
            policy_candidate=candidate,
        )
        owners.append(
            OwnerPlan(
                source,
                result,
                resolved.variant,
                resolved.decision_ids[-1] if resolved.variant != "default" else None,
                selection,
                resolved.reason,
            )
        )
        uses.extend(result.uses)
    uses.extend(sources.uses)
    uses.extend(terminal_sources.uses)
    return Plan(
        sources.build, snapshot, tuple(owners), uses_sorted(uses), tuple(unknown_owners)
    )


def _policy_candidate(
    result: NamePolicyResult, frozen: Catalogue, sources: Sources
) -> NameCandidate | None:
    if result.status != "eligible":
        return None
    matching = [
        ref
        for ref in result.refs
        if ref.parser == "translation-" + str(result.game) + "-v1"
        and ref.text_hash == digest(str(result.text).encode())
    ]
    proofs = [sources.text(ref) for ref in matching]
    # Equal JA/Hant hashes cannot substitute a Japanese proof for target language.
    target = next(
        (
            proof
            for proof in proofs
            if proof[0] == "zh-Hant" and proof[1] == result.text
        ),
        None,
    )
    if target is None:
        raise ValueError("Eligible policy name lacks frozen target language proof")
    return NameCandidate(
        str(result.text),
        "official_" + str(result.game),
        "digital_official",
        None,
        frozen.reviewed_at,
        target[2],
        result.refs,
    )


def _counterparts(  # ruff: ignore[too-many-locals] -- relation, actual sample membership and frozen name proof are separate gates
    db: Database,
    sources: Sources,
    owner: NameOwner,
    links: LinkResult | None,
    *,
    name_ref: SourceRef | None = None,
) -> tuple[NameCandidate, ...]:
    if links is None:
        return ()
    eligible = links.eligible_owner(db, sources, owner, name_ref=name_ref)
    records = {r.values["id"]: r.values for r in db.rows("decision")}
    fresh = {
        k: r
        for members in links.by_owner.values()
        for r in members
        for k in (r.record_key,)
    }
    decisions = dict(links.decisions)
    checked = {
        link_id(fresh[key])
        for key, decision in decisions.items()
        if key in fresh
        and key in links.checked_members
        and records[decision]["reviewed_by"] == "gbaian10"
        and records[decision]["state"] in {"sampled", "confirmed"}
        and isinstance(records[decision]["sample_ids"], Json)
        and _sample_member(key, records[decision]["sample_ids"])
    }
    faces = {r.values["id"]: r.values for r in db.rows("digital_face")}
    cards = {r.values["id"]: r.values for r in db.rows("digital_card")}
    units = {r.values["id"]: r.values for r in db.rows("text_unit")}
    names = {
        r.values["digital_face_id"]: str(units[r.values["name_unit_id"]]["text"])
        for r in db.rows("digital_text")
        if r.values["lang"] == "zh-Hant"
    }
    result = []
    for row in db.rows("digital_link"):
        link = row.values
        if link["id"] not in eligible or link["digital_face_id"] not in names:
            continue
        face = faces[link["digital_face_id"]]
        card = cards[link["digital_card_id"]]
        text = names[face["id"]]
        ref, source = _proof(
            sources,
            str(card["game"]),
            str(card["official_id"]),
            str(face["phase"]),
            "zh-Hant",
            text,
        )
        decision = records[link["decision_id"]]
        result.append(
            NameCandidate(
                text,
                "official_" + str(card["game"]),
                "digital_official",
                str(decision["id"]),
                str(decision["reviewed_at"]),
                source,
                (ref,),
                counterpart_checked=str(link["id"]) in checked,
            )
        )
    return tuple(sorted(result, key=lambda c: (c.origin, c.text, c.decision_id or "")))


def _choices(
    db: Database,
    sources: Sources,
    replay: NameReplay,
    owner_source: NameSource,
    term_id: str | None,
    counterparts: tuple[NameCandidate, ...],
) -> tuple[NameCandidate, ...]:
    if term_id is None:
        return ()
    values = {
        r.values["term_id"]: r.values
        for r in db.rows("glossary_translation")
        if r.values["lang"] == "zh-Hant"
    }
    chosen = values.get(term_id)
    if chosen is None:
        return ()
    decisions = {d.id: d for s in replay.snapshot.envelopes() for d in s.decisions}
    records = [
        (r, d)
        for r, d in replay.snapshot.effective()
        if isinstance(r, ChoiceRecord)
        and r.data.term_id == term_id
        and r.data.lang == "zh-Hant"
        and r.data.value is not None
    ]
    if len(records) != 1:
        raise ValueError("Adopted name choice differs from complete glossary replay")
    record, identifier = records[0]
    assert isinstance(record, ChoiceRecord)
    decision = decisions[identifier]
    value = validate_choice(record, original=owner_source.text, sources=sources, db=db)
    if value is None or (
        chosen["text"],
        chosen["origin"],
        chosen["source_id"],
        chosen["decision_id"],
    ) != (value[0], record.data.origin, value[1], identifier):
        raise ValueError("Imported name choice differs from checked authored evidence")
    human = (
        record.data.adoption_review.mode == "human"
        and decision.reviewed_by == "gbaian10"
        and record.record_key in decision.sample_ids
    )
    official = str(chosen["origin"]).startswith("official_")
    if official:
        matches = [
            c
            for c in counterparts
            if c.text == chosen["text"]
            and c.origin == chosen["origin"]
            and any(
                e.kind == "digital_name"
                and e.sve_owner == owner_source.face_id
                and e.target_ref in c.refs
                for e in record.data.concept_evidence
            )
        ]
        if not matches:
            return ()
        return (
            replace(
                matches[0],
                human_selected=human,
                decision_id=identifier,
                reviewed_at=decision.reviewed_at,
                choice_hash=record_hash(record),
            ),
        )
    return (
        NameCandidate(
            str(chosen["text"]),
            str(chosen["origin"]),
            "unofficial",
            identifier,
            decision.reviewed_at,
            None,
            human_selected=human,
            choice_hash=record_hash(record),
        ),
    )


def populate(
    db: Database,
    inputs: Inputs,
    texts: TextPlan,
    *,
    sources: Sources,
    replay: NameReplay,
    links: LinkResult | None = None,
) -> Result:
    """Recompute at the write boundary; callers cannot substitute a forged plan."""
    if inputs.load().current_names:
        from sve_carddb.digital_name_policies.current_application import (  # ruff: ignore[import-outside-top-level] -- format dispatch avoids the legacy/current application import cycle
            populate as populate_current,
        )

        return populate_current(
            db, inputs, texts, sources=sources, replay=replay, links=links
        )
    plan = prepare(db, inputs, texts, sources=sources, replay=replay, links=links)
    selected = tuple(o for o in plan.owners if o.selection.reason == "policy")
    insert_raw_sources(db, (use.source for use in plan.uses))
    decision = _audit(db, plan, selected) if selected else None
    bindings = []
    for owner in plan.owners:
        candidate = owner.selection.candidate
        if candidate is None:
            continue
        if name_source(db, owner.source.owner) != owner.source:
            raise ValueError("Name application owner changed after checked planning")
        context = name_context(
            db, owner.source, variant=owner.variant, decision_id=owner.variant_decision
        )
        use = bind_name_use(db, owner.source.owner, context, replay=replay)
        if owner.selection.reason == "policy":
            candidate = replace(candidate, decision_id=decision)
        translation = materialize_name(db, owner.source, context, candidate)
        payload = owner.source.owner
        destination = (
            (payload.kind, payload.identifier)
            if payload.kind == "face_revision"
            else (payload.kind, payload.identifier, str(payload.face_id))
        )
        bindings.append(
            DisplayBinding(use, destination, "zh-Hant", "own_source", translation)
        )
    return Result(input_record(plan.context, plan.uses), tuple(bindings), plan.report())


def _application_at(plan: Plan) -> str:
    value = object_value(
        object_value(parse(plan.context.configuration.encode()))[
            "digital_name_application"
        ]
    )["application_at"]
    assert isinstance(value, str)
    return value


def _sample_member(key: str, value: object) -> bool:
    return (
        isinstance(value, Json) and isinstance(value.value, list) and key in value.value
    )


def _audit(db: Database, plan: Plan, selected: tuple[OwnerPlan, ...]) -> str:
    membership = digest(canonical([o.source.owner.payload() for o in selected]))
    loaded = plan.snapshot.effective("names")
    receipt = loaded.receipt()
    identifier = (
        "d:"
        + digest(
            canonical(
                [
                    "digital-name-policy-application-v1",
                    digest(loaded.policy),
                    membership,
                ]
            )
        )[7:]
    )
    insert_exact(
        db,
        "decision",
        {
            "id": identifier,
            "category": "digital_name_policy",
            "scope": "batch",
            "state": "confirmed",
            "policy_id": loaded.document().policy_id,
            "membership_hash": membership,
            "sample_ids": Json([]),
            "authored_by": "owner-name-v1",
            "authored_at": _application_at(plan),
            "reviewed_by": receipt.reviewed_by,
            "reviewed_at": receipt.reviewed_at,
            "confidence": None,
            "note": "Policy approval only; no per-owner human checks. Application time is the separately pinned build publication instant.",
        },
        ("id",),
    )
    for name, exact in plan.snapshot.files:
        source = (
            "authored:digital-name:"
            + digest(canonical([plan.snapshot.authored_revision, name, digest(exact)]))[
                7:
            ]
        )
        insert_exact(
            db,
            "source_record",
            {
                "id": source,
                "kind": "authored",
                "sha256": digest(exact),
                "authored_path": "authored/" + name,
                "authored_revision": plan.snapshot.authored_revision,
                "parser_version": "digital-name-policy-v1",
            },
            ("id",),
        )
        insert_exact(
            db,
            "decision_source",
            {
                "decision_id": identifier,
                "source_id": source,
                "role": "policy:" + digest(name.encode())[7:],
                "locator": name,
                "quote": None,
            },
            ("decision_id", "source_id", "role"),
        )
    for use in plan.uses:
        role = (
            "policy_application:"
            + digest(canonical([use.source.id, use.usage, use.locator]))[7:]
        )
        insert_exact(
            db,
            "decision_source",
            {
                "decision_id": identifier,
                "source_id": use.source.id,
                "role": role,
                "locator": use.locator,
                "quote": None,
            },
            ("decision_id", "source_id", "role"),
        )
    return identifier
