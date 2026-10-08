"""Apply editable name rules with owner-local eligibility and render-v2 outputs."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.build_db.rows import insert_exact
from sve_carddb.build_db.source_rows import insert_raw_sources
from sve_carddb.build_db.t2_translation import OWNERS
from sve_carddb.core.json import canonical, digest, object_value, parse
from sve_carddb.core.provenance import input_record, uses_sorted
from sve_carddb.digital_links.importer import review_context
from sve_carddb.digital_name_policies.application import Result, _counterparts
from sve_carddb.digital_name_policies.current_evaluate import catalogue
from sve_carddb.digital_name_policies.evaluate import name_result, owner_text
from sve_carddb.digital_name_policies.owners import publication_owners
from sve_carddb.snapshot.project.evidence import DisplayBinding
from sve_carddb.translations.counterparts import first_counterpart
from sve_carddb.translations.current_models import ChoiceRecord, TermRecord
from sve_carddb.translations.importer import validate_choice
from sve_carddb.translations.name_sources import name_source

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build_db import Database, Value
    from sve_carddb.core.provenance import BuildContext, Source, SourceUse
    from sve_carddb.digital_links.importer import Result as LinkResult
    from sve_carddb.digital_name_policies.application import Inputs
    from sve_carddb.digital_name_policies.loader import Snapshot
    from sve_carddb.text_observations.plan import TextPlan
    from sve_carddb.translations.counterparts import NameCandidate
    from sve_carddb.translations.current_names import Names
    from sve_carddb.translations.name_sources import NameSource
    from sve_carddb.translations.sources import Sources


@dataclass(frozen=True)
class Candidate:
    text: str
    origin: str
    authority: str
    low_confidence: bool
    source: Source | None
    dependency: JsonValue


@dataclass(frozen=True)
class Selected:
    source: NameSource
    variant: str
    candidate: Candidate | None
    reason: str
    semantic_reason: str


@dataclass(frozen=True)
class Plan:
    snapshot: Snapshot
    owners: tuple[Selected, ...]
    uses: tuple[SourceUse, ...]

    def report(self) -> dict[str, JsonValue]:
        """Diagnostics contain IDs and counts; never official card text or private events."""
        rows: list[JsonValue] = [
            {
                "owner": o.source.owner.payload(),
                "reason": o.reason,
                "semantic_reason": o.semantic_reason,
                "low_confidence": o.candidate is not None
                and o.candidate.low_confidence,
            }
            for o in self.owners
        ]
        return {
            "owners": rows,
            "covered_owners": sum(o.candidate is not None for o in self.owners),
            "warning_owners": sum(
                o.semantic_reason == "ambiguous_name_concept"
                or (o.candidate is not None and o.candidate.low_confidence)
                for o in self.owners
            ),
            "untranslated_owners": sum(o.candidate is None for o in self.owners),
        }


def _choice(
    db: Database,
    sources: Sources,
    replay: Names,
    owner: NameSource,
    term_id: str | None,
    counterparts: tuple[NameCandidate, ...],
) -> Candidate | None:
    if term_id is None:
        return None
    records = replay.snapshot.current_records()
    choices = [
        r
        for r in records
        if isinstance(r, ChoiceRecord)
        and r.data.term_id == term_id
        and r.data.lang == "zh-Hant"
    ]
    term = next(
        (r for r in records if isinstance(r, TermRecord) and r.data.id == term_id), None
    )
    if not choices or term is None:
        return None
    record = choices[0]
    value = validate_choice(record, original=owner.text, sources=sources, db=db)
    if value is None:
        return None
    official = record.origin == "official"
    matching = [
        c
        for c in counterparts
        if c.text == value[0]
        and any(
            e.kind == "digital_name"
            and e.sve_owner == owner.face_id
            and e.target_ref in c.refs
            for e in record.data.concept_evidence
        )
    ]
    if official and not matching:
        return None
    source = None if not official else matching[0].source
    low = record.low_confidence or term.low_confidence
    return Candidate(
        value[0],
        record.origin,
        "digital_official" if official else "unofficial",
        low,
        source,
        [
            {
                "kind": "glossary_choice",
                "key": [term_id, "zh-Hant"],
                "value": {
                    "text": value[0],
                    "origin": record.origin,
                    "low_confidence": low,
                },
            }
        ],
    )


def prepare(  # ruff: ignore[complex-structure,too-many-branches,too-many-locals,too-many-statements] -- each owner has independent source, policy eligibility and explicit selection
    db: Database,
    inputs: Inputs,
    texts: TextPlan,
    *,
    sources: Sources,
    replay: Names,
    links: LinkResult | None = None,
) -> Plan:
    """Complete catalogues and each own source are checked in the current build."""
    snapshot = inputs.load()
    if len(snapshot.current_names) != 1:
        raise ValueError("Current name application needs one current name policy")
    config = object_value(parse(sources.build.configuration.encode()))
    if (
        config.get("digital_name_application")
        != inputs.configuration()["digital_name_application"]
    ):
        raise ValueError("Build configuration does not pin complete name policy inputs")
    policy = snapshot.current_names[0]
    frozen = catalogue(policy, sources)
    review = review_context(sources)
    owners: list[Selected] = []
    originals = dict(replay.originals)
    for owner, policy_owner in publication_owners(db, texts):
        evidence = owner_text(policy_owner, sources, review)
        outcome = name_result(evidence, frozen)
        source = name_source(db, owner)
        if source is None:
            continue
        if evidence.text != source.text:
            raise ValueError("Name policy evidence differs from the materialized owner")
        resolved = replay.resolve(db, owner)
        if resolved is None:
            raise ValueError("Current name resolver omitted a known owner")
        counterparts = _counterparts(
            db, sources, owner, links, name_ref=policy_owner.name_ref
        )
        overrides = [
            o
            for o in policy.content.name_overrides
            if o.owner.model_dump(mode="json") == owner.payload()
            and o.source_hash == source.source_hash
        ]
        chosen = None
        reason = "untranslated"
        if overrides:
            selected_term = overrides[0].term_id
            if originals.get(selected_term) != source.text:
                raise ValueError(
                    "Name override differs from its owner's exact concept source"
                )
            chosen = _choice(db, sources, replay, source, selected_term, counterparts)
            reason = (
                "override" if chosen is not None else "missing_override_translation"
            )
        elif outcome.status == "eligible":
            if outcome.text is None or outcome.game is None or not outcome.refs:
                raise ValueError("Eligible name policy result lacks a complete target")
            # Target language, rather than string equality, locates the official evidence.
            targets = [sources.text(ref) for ref in outcome.refs]
            matching = [
                item
                for item in targets
                if item[0] == "zh-Hant" and item[1] == outcome.text
            ]
            if not matching:
                raise ValueError(
                    "Name policy result lacks Traditional Chinese evidence"
                )
            chosen = Candidate(
                outcome.text,
                "official",
                "digital_official",
                policy.low_confidence,
                matching[0][2],
                [
                    {
                        "kind": "digital_name",
                        "key": [outcome.game, source.source_hash],
                        "value": outcome.text,
                    }
                ],
            )
            reason = "policy"
        elif counterparts and outcome.status != "excluded":
            counterpart = first_counterpart(counterparts)
            if counterpart is None:
                raise ValueError("Confirmed counterpart lacks a supported game")
            chosen = Candidate(
                counterpart.text,
                "official",
                "digital_official",
                False,
                counterpart.source,
                [
                    {
                        "kind": "counterpart",
                        "key": source.source_hash,
                        "value": counterpart.text,
                    }
                ],
            )
            reason = "counterpart"
        else:
            chosen = _choice(db, sources, replay, source, resolved.term_id, ())
            reason = "choice" if chosen is not None else "untranslated"
        owners.append(
            Selected(source, resolved.variant, chosen, reason, resolved.reason)
        )
    return Plan(snapshot, tuple(owners), uses_sorted(sources.uses))


def populate(
    db: Database,
    inputs: Inputs,
    texts: TextPlan,
    *,
    sources: Sources,
    replay: Names,
    links: LinkResult | None = None,
) -> Result:
    """Generate current IDs and bindings without any review event or decision row."""
    plan = prepare(db, inputs, texts, sources=sources, replay=replay, links=links)
    return apply(db, plan, sources.build)


def apply(db: Database, plan: Plan, build: BuildContext) -> Result:
    """Project one checked owner-name plan into the build database."""
    insert_raw_sources(db, (u.source for u in plan.uses))
    bindings = []
    for owner in plan.owners:
        candidate = owner.candidate
        if candidate is None:
            continue
        if name_source(db, owner.source.owner) != owner.source:
            raise ValueError("Name application owner changed after current planning")
        context = (
            "ctx:"
            + digest(
                canonical(
                    {
                        "recipe": "context-v1",
                        "source_unit_id": owner.source.unit_id,
                        "semantic_variant": owner.variant,
                    }
                )
            )[7:]
        )
        insert_exact(
            db,
            "translation_context",
            {
                "id": context,
                "source_unit_id": owner.source.unit_id,
                "semantic_variant": owner.variant,
            },
            ("id",),
        )
        use = (
            "use:"
            + digest(
                canonical(
                    {
                        "recipe": "use-v1",
                        "owner": owner.source.owner.payload(),
                        "context_id": context,
                        "field": "name",
                        "ordinal": None,
                    }
                )
            )[7:]
        )
        values: dict[str, Value] = {name: None for group in OWNERS for name in group}
        values.update(id=use, context_id=context, field="name", ordinal=None)
        payload = owner.source.owner
        destination: tuple[str, ...]
        if payload.kind == "face_revision":
            values["face_revision_id"] = payload.identifier
            destination = (payload.kind, payload.identifier)
        else:
            values.update(printing_id=payload.identifier, face_id=payload.face_id)
            destination = (payload.kind, payload.identifier, str(payload.face_id))
        insert_exact(db, "translation_use", values, ("id",))
        translation = materialize(db, owner.source, context, candidate)
        bindings.append(
            DisplayBinding(use, destination, "zh-Hant", "own_source", translation)
        )
    return Result(input_record(build, plan.uses), tuple(bindings), plan.report())


def materialize(
    db: Database, source: NameSource, context: str, candidate: Candidate
) -> str:
    """Render IDs include semantic dependencies and quality, excluding notes and proofs."""
    checksum = digest(
        canonical(
            {
                "recipe": "render-v2",
                "context_id": context,
                "target_lang": "zh-Hant",
                "dependency_key": candidate.dependency,
                "text": candidate.text,
                "origin": candidate.origin,
                "authority": candidate.authority,
                "low_confidence": candidate.low_confidence,
            }
        )
    )[7:]
    if candidate.authority == "digital_official" and candidate.source is None:
        raise ValueError("Official current name lacks its own frozen target source")
    if candidate.source is not None:
        insert_raw_sources(db, (candidate.source,))
    identifier = "tr:" + checksum
    values: dict[str, Value] = {
        "id": identifier,
        "context_id": context,
        "target_lang": "zh-Hant",
        "revision": int(checksum[:13], 16),
        "text": candidate.text,
        "tokens": None,
        "origin": candidate.origin,
        "authority": candidate.authority,
        "low_confidence": candidate.low_confidence,
        "source_hash": source.source_hash,
        "source_id": None if candidate.source is None else candidate.source.id,
    }
    existing = db.select(
        "translation", db.columns("translation"), where={"id": identifier}
    )
    if existing:
        if any(
            existing[0].values[k] != v for k, v in values.items() if k != "source_id"
        ):
            raise ValueError("Stable current name translation ID collision")
    else:
        insert_exact(db, "translation", values, ("id",))
    return identifier
