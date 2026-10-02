"""Atomic authored adoption projection after independent immutable history replay."""

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING, TypedDict, Unpack

from pydantic import JsonValue

from sve_carddb.build_db import Json
from sve_carddb.build_inputs import (
    SourceUse,
    input_record,
    insert_raw_sources,
    uses_sorted,
)
from sve_carddb.products.models import LocalizedText
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.source_corrections.importer import correction_values, evidence_values
from sve_carddb.text_observations.importer import (
    populate_revision,
    populate_vocabulary,
    stat,
)
from sve_carddb.text_observations.intern import TextInterner
from sve_carddb.text_observations.models import candidate_revision_id
from sve_carddb.wording_adoptions.loader import INDEX_PATH, load_adoptions
from sve_carddb.wording_adoptions.models import Mechanical
from sve_carddb.wording_adoptions.reconstruction import (
    Reconstructor,
    runtime_dependencies,
)
from sve_carddb.wording_adoptions.replay import replay_adoptions
from sve_carddb.wording_adoptions.semantics import populate_semantics

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.build_db import Database, Value
    from sve_carddb.build_inputs import BuildContext, InputRecord
    from sve_carddb.registry.records import Region
    from sve_carddb.registry.snapshot import RegistrySnapshot
    from sve_carddb.text_observations.models import FaceObservation
    from sve_carddb.text_observations.vocabulary import Vocabulary
    from sve_carddb.wording_adoptions.loader import AdoptionSnapshot
    from sve_carddb.wording_adoptions.models import ReviewContext
    from sve_carddb.wording_adoptions.reconstruction import ReconstructedScope
    from sve_carddb.wording_adoptions.replay import ReplayedAdoption


class AdoptionInputs(TypedDict):
    repository_root: Path
    authored_root: Path
    authored_revision: str
    registry: RegistrySnapshot
    stores: Mapping[str, Path]
    build: BuildContext
    vocabulary: Vocabulary
    published: tuple[LocalizedText, ...]
    current_review: ReviewContext


def adoption_dependencies(
    snapshot: AdoptionSnapshot, replayed: tuple[ReplayedAdoption, ...]
) -> dict[str, bytes]:
    """Retain all historical bytes under context-qualified portable dependency names."""
    files = {"authored/" + INDEX_PATH: snapshot.index_bytes} | {
        "authored/" + s.path: s.exact_bytes for s in snapshot.shards
    }
    for item in replayed:
        review = item.record.data.review_context
        context_hash = digest(canonical(review.model_dump(mode="json"))).removeprefix(
            "sha256:"
        )
        files.update(
            {
                f"wording-reviews/{context_hash}/{name}": raw
                for name, raw in item.scope.dependencies.items()
            }
        )
        if (
            isinstance(item.record.data.previous, Mechanical)
            and item.predecessor_scope is not None
        ):
            review = item.record.data.previous.review_context
            key = digest(canonical(review.model_dump(mode="json"))).removeprefix(
                "sha256:"
            )
            files.update(
                {
                    f"wording-reviews/{key}/{name}": raw
                    for name, raw in item.predecessor_scope.dependencies.items()
                }
            )
        pin = item.record.data.review.rule_set
        if pin is not None:
            files.update(
                {
                    f"wording-policies/{pin.authored_revision}/{name}": raw
                    for name, raw in item.policy_files.items()
                }
            )
    return files


def adoption_configuration(
    snapshot: AdoptionSnapshot,
    replayed: tuple[ReplayedAdoption, ...],
    vocabulary: Vocabulary,
    published: tuple[LocalizedText, ...],
) -> dict[str, JsonValue]:
    """Keep contexts explicit so qualified dependency names remain independently replayable."""
    contexts = {
        canonical(
            i.record.data.review_context.model_dump(mode="json")
        ): i.record.data.review_context
        for i in replayed
    }
    contexts.update(
        {
            canonical(
                i.record.data.previous.review_context.model_dump(mode="json")
            ): i.record.data.previous.review_context
            for i in replayed
            if isinstance(i.record.data.previous, Mechanical)
        }
    )
    return {
        "wording_adoption": {
            "authored_revision": snapshot.authored_revision,
            "index_path": "authored/" + INDEX_PATH,
            "index_hash": snapshot.index_hash,
        },
        "wording_review_contexts": [
            {"hash": digest(key), "review_context": value.model_dump(mode="json")}
            for key, value in sorted(contexts.items())
        ],
        "wording_rulesets": list[JsonValue](
            sorted(
                {
                    canonical(
                        i.record.data.review.rule_set.model_dump(mode="json")
                    ).decode()
                    for i in replayed
                    if i.record.data.review.rule_set is not None
                }
            )
        ),
        "wording_vocabulary": canonical(vocabulary.model_dump(mode="json")).decode(),
        "wording_published_text_hash": digest(
            canonical(
                list[JsonValue](
                    sorted(
                        {
                            canonical(t.model_dump(mode="json")).decode()
                            for t in published
                        }
                    )
                )
            )
        ),
    }


def _envelopes(db: Database, snapshot: AdoptionSnapshot) -> None:
    existing = {r.values["id"] for r in db.rows("decision")}
    for shard in snapshot.shards:
        decision = shard.envelope.decisions[0]
        source_id = "authored:v1:" + digest(
            canonical(
                [shard.path, snapshot.authored_revision, digest(shard.exact_bytes)]
            )
        ).removeprefix("sha256:")
        db.insert(
            "source_record",
            {
                "id": source_id,
                "kind": "authored",
                "sha256": digest(shard.exact_bytes),
                "authored_path": "authored/" + shard.path,
                "authored_revision": snapshot.authored_revision,
                "parser_version": "wording-adoption-v1",
            },
        )
        if decision.id in existing:
            raise ValueError(
                "Adoption decision is already imported or has conflicting identity"
            )
        fields = decision.model_dump(
            mode="json", exclude={"members", "reviewed_precision"}
        )
        row: dict[str, Value] = {
            key: value
            for key, value in fields.items()
            if isinstance(value, str) or value is None
        }
        row["sample_ids"] = Json(list[JsonValue](decision.sample_ids))
        db.insert("decision", row)
        db.insert(
            "decision_source",
            {
                "decision_id": decision.id,
                "source_id": source_id,
                "role": "wording_adoption_envelope",
                "locator": shard.path,
            },
        )
        locators: dict[tuple[str, str], set[str]] = defaultdict(set)
        for record in shard.envelope.records:
            for evidence in record.evidence:
                source = snapshot.closure[evidence].source
                locators[source.id, evidence.role].add(evidence.locator)
        for (source_id, role), values in sorted(locators.items()):
            db.insert(
                "decision_source",
                {
                    "decision_id": decision.id,
                    "source_id": source_id,
                    "role": role,
                    "locator": canonical(list[JsonValue](sorted(values))).decode(),
                },
            )


def _revision(
    db: Database,
    item: FaceObservation,
    adoption: ReplayedAdoption,
    texts: TextInterner,
    vocabulary: Vocabulary,
    *,
    checked: bool = True,
) -> None:
    identifier = candidate_revision_id(item)
    existing = next(
        (
            row.values
            for row in db.rows("face_revision")
            if row.values["id"] == identifier
        ),
        None,
    )
    if existing is not None:
        _verify_revision(db, item, existing, vocabulary)
        return
    ordinal = (
        max(
            (
                r.values["revision"]
                for r in db.rows("face_revision")
                if r.values["face_id"] == item.face_id
                and r.values["region"] == item.region
                and isinstance(r.values["revision"], int)
            ),
            default=0,
        )
        + 1
    )
    if item.correction_keys:
        correction_decisions = {
            digest(r.content): r.decision_id
            for scope in (adoption.scope, adoption.predecessor_scope)
            if scope is not None
            for r in scope.registry.records.values()
        }
        decision_id = correction_decisions[item.correction_keys[-1]]
    else:
        decision_id = adoption.decision.id if checked else None
    raw = item.model_copy(
        update={
            "content": item.card.projected(item.source_index),
            "correction_keys": (),
        }
    )
    populate_revision(
        db,
        item,
        ordinal,
        texts,
        vocabulary,
        decision_id=decision_id,
        supersedes_id=candidate_revision_id(raw) if item.correction_keys else None,
    )


def _verify_revision(
    db: Database,
    item: FaceObservation,
    existing: Mapping[str, Value],
    vocabulary: Vocabulary,
) -> None:
    """An existing content-derived ID is not evidence that its stored graph is intact."""
    content = item.content
    kind = vocabulary.lookup(item.region, "type", content.type_raw)
    cost, attack, defense = (stat(v) for v in content.stats)
    required = {
        "face_id": item.face_id,
        "region": item.region,
        "class_code": None
        if content.class_raw == "-"
        else vocabulary.lookup(item.region, "class", content.class_raw).code,
        "type_code": kind.code,
        "cost": cost,
        "attack": attack,
        "defense": defense,
    }
    units = {
        r.values["id"]: (r.values["lang"], r.values["text"])
        for r in db.rows("text_unit")
    }
    lang = "ja" if item.region == "jp" else "en"
    identifier = existing["id"]
    sections = sorted(
        (
            r.values
            for r in db.rows("face_text_section")
            if r.values["revision_id"] == identifier
        ),
        key=lambda r: _ordinal(r["ordinal"]),
    )
    traits = {
        r.values["trait_code"]
        for r in db.rows("face_trait")
        if r.values["revision_id"] == identifier
    }
    titles = {
        r.values["title_code"]
        for r in db.rows("face_title")
        if r.values["revision_id"] == identifier
    }
    special = {
        r.values["special_kind_code"]
        for r in db.rows("face_special_kind")
        if r.values["revision_id"] == identifier
    }
    checks = (
        all(existing[k] == v for k, v in required.items()),
        units.get(existing["name_unit_id"]) == (lang, content.name),
        units.get(existing["effect_unit_id"]) == (lang, content.effect),
        [r["ordinal"] for r in sections] == list(range(len(content.sections))),
        [units.get(r["text_unit_id"]) for r in sections]
        == [(lang, s) for s in content.sections],
        traits
        == {vocabulary.lookup(item.region, "trait", t).code for t in content.traits},
        titles
        == (
            set()
            if content.title is None
            else {vocabulary.lookup(item.region, "title", content.title).code}
        ),
        special == set(kind.special_kinds),
    )
    if not all(checks):
        raise ValueError("Historical revision content graph conflicts")


def _ordinal(value: Value) -> int:
    if type(value) is not int:
        raise TypeError("Section ordinal must be an integer")
    return value


@dataclass(frozen=True)
class PreparedAdoptions:
    snapshot: AdoptionSnapshot
    reconstruction: Reconstructor
    replayed: tuple[ReplayedAdoption, ...]
    current_scopes: Mapping[tuple[str, Region], ReconstructedScope]
    uses: tuple[SourceUse, ...]


def prepare_adoptions(inputs: AdoptionInputs) -> PreparedAdoptions:
    """Independently enumerate expected sources before building or verifying a bundle."""
    registry = inputs["registry"]
    stores, build = inputs["stores"], inputs["build"]
    current_review = inputs["current_review"]
    snapshot = load_adoptions(
        inputs["authored_root"],
        authored_revision=inputs["authored_revision"],
        registry=registry,
        stores=stores,
    )
    reconstruction = Reconstructor(inputs["repository_root"], stores)
    replayed = replay_adoptions(snapshot, reconstruction)
    current_scopes = {
        (i.record.data.face_id, i.record.data.region): reconstruction.scope(
            current_review, i.record.data.face_id, i.record.data.region
        )
        for i in replayed
    }
    config = object_value(parse(build.configuration.encode()))
    required = adoption_configuration(
        snapshot, replayed, inputs["vocabulary"], inputs["published"]
    )
    required["wording_current_review"] = current_review.model_dump(mode="json")
    declared = {pin.name: pin.sha256 for pin in build.dependencies}
    current_key = digest(
        canonical(current_review.model_dump(mode="json"))
    ).removeprefix("sha256:")
    dependencies = adoption_dependencies(snapshot, replayed) | runtime_dependencies(
        reconstruction.repository, build.program_revision
    )
    for scope in current_scopes.values():
        if scope.registry.files != registry.files:
            raise ValueError("Current wording review and build registry disagree")
        dependencies.update(
            {
                f"wording-reviews/{current_key}/{name}": raw
                for name, raw in scope.dependencies.items()
            }
        )
    if any(config.get(key) != value for key, value in required.items()) or any(
        declared.get(name) != digest(raw) for name, raw in dependencies.items()
    ):
        raise ValueError(
            "Adoption build configuration or dependency closure is incomplete"
        )
    inputs["vocabulary"].verify()
    uses = uses_sorted(
        (
            *[u for i in replayed for u in i.scope.uses],
            *[
                u
                for i in replayed
                if i.predecessor_scope is not None
                for u in i.predecessor_scope.uses
            ],
            *[u for scope in current_scopes.values() for u in scope.uses],
            *ordering_uses(snapshot, replayed),
            *[
                SourceUse(
                    source=source.source,
                    usage="wording_evidence_closure",
                    locator=canonical(ref.model_dump(mode="json")).decode(),
                )
                for ref, source in snapshot.closure.items()
            ],
        )
    )
    return PreparedAdoptions(snapshot, reconstruction, replayed, current_scopes, uses)


def ordering_uses(
    snapshot: AdoptionSnapshot, replayed: tuple[ReplayedAdoption, ...]
) -> tuple[SourceUse, ...]:
    """Retain the source and record position used by each confirmed catalog date edge."""
    uses = []
    for item in replayed:
        record = item.record
        edges = (
            *record.data.order_evidence,
            *(
                ()
                if record.data.previous_order is None
                else (record.data.previous_order,)
            ),
        )
        for edge in edges:
            if edge.basis != "printing_availability":
                continue
            for index in edge.evidence_indexes:
                evidence = record.evidence[index]
                source = snapshot.closure[evidence].source.model_copy(
                    update={"parser_version": "authored-product-date-v1"}
                )
                uses.append(
                    SourceUse(
                        source=source,
                        usage="wording_order_evidence",
                        locator=canonical(
                            {
                                "record_key": record.record_key,
                                "edge": edge.model_dump(mode="json"),
                                "evidence": evidence.model_dump(mode="json"),
                            }
                        ).decode(),
                    )
                )
    return uses_sorted(uses)


def adoption_report(prepared: PreparedAdoptions) -> dict[str, JsonValue]:
    """Describe the exact selection and unchecked inventory without official wording."""
    return {
        "wording_adoptions": [
            {
                "record_key": i.record.record_key,
                "record_hash": digest(canonical(i.record.model_dump(mode="json"))),
                "decision_id": i.decision.id,
                "selected_observation_key": i.record.data.selected_observation_key,
                "previous": None
                if i.record.data.previous is None
                else i.record.data.previous.model_dump(mode="json"),
                "basis": i.basis,
                "unchecked_observation_keys": list[JsonValue](
                    sorted(
                        set(
                            prepared.current_scopes[
                                i.record.data.face_id, i.record.data.region
                            ].contents
                        )
                        - set(i.record.data.checked_observation_keys)
                    )
                ),
            }
            for i in prepared.replayed
        ]
    }


def populate_adoptions(db: Database, **inputs: Unpack[AdoptionInputs]) -> InputRecord:
    """Reload and replay immutable inputs inside the complete caller transaction."""
    prepared = prepare_adoptions(inputs)
    if prepared.replayed and not db.has_table("face_semantics"):
        raise ValueError("Wording adoption requires enabled semantics capability")
    insert_raw_sources(db, (u.source for u in prepared.uses))
    _envelopes(db, prepared.snapshot)
    texts = TextInterner(db, published=inputs["published"])
    populate_vocabulary(db, texts, inputs["vocabulary"])
    for adoption in prepared.replayed:
        _populate_record(db, adoption, texts, inputs["vocabulary"])
        _correction_history(db, adoption.scope, texts)
        if adoption.predecessor_scope is not None:
            _correction_history(db, adoption.predecessor_scope, texts)
    latest = {
        (i.record.data.face_id, i.record.data.region): i for i in prepared.replayed
    }
    for key, scope in prepared.current_scopes.items():
        for item in scope.contents.values():
            if item.content.effect is not None:
                raw = item.model_copy(
                    update={
                        "content": item.card.projected(item.source_index),
                        "correction_keys": (),
                    }
                )
                _revision(
                    db, raw, latest[key], texts, inputs["vocabulary"], checked=False
                )
                _revision(
                    db, item, latest[key], texts, inputs["vocabulary"], checked=False
                )
                _physical(db, raw)
        _correction_history(db, scope, texts)
    _freshness(db, prepared, inputs["current_review"])
    _support(db)
    record = input_record(inputs["build"], prepared.uses)
    record.verify(db, inputs["build"], prepared.uses, complete=False)
    return record


def _correction_history(
    db: Database, scope: ReconstructedScope, texts: TextInterner
) -> None:
    """Keep each raw-to-corrected application instead of borrowing a current-source row."""
    for application in scope.applications:
        if not all(
            db.has_table(t)
            for t in (
                "source_correction",
                "correction_evidence",
                "correction_application",
            )
        ):
            raise ValueError("Historical corrections require the correction capability")
        data = application.data
        lang = "ja" if application.observation.region == "jp" else "en"
        if data.field == "effect":
            texts.intern(LocalizedText(lang=lang, text=data.expected_raw_value))
            unit = texts.intern(LocalizedText(lang=lang, text=data.corrected_value))
        else:
            unit = None
        _exact_row(
            db,
            "source_correction",
            correction_values(application),
            ("id",),
            ignored=("state",),
        )
        for values in evidence_values(application):
            _exact_row(
                db,
                "correction_evidence",
                values,
                ("correction_id", "source_id", "kind", "locator"),
            )
        original = application.observation
        item = next(
            v
            for v in scope.contents.values()
            if (v.printing_id, v.card.source.id)
            == (original.printing_id, original.card.source.id)
        )
        if item.content.effect is None:
            raise ValueError(
                "Historical correction has no representable result revision"
            )
        _exact_row(
            db,
            "correction_application",
            {
                "correction_id": data.id,
                "source_id": original.card.source.id,
                "result_unit_id": unit,
                "face_revision_id": candidate_revision_id(item),
                "status": application.status,
            },
            ("correction_id", "source_id"),
        )


def _exact_row(
    db: Database,
    table: str,
    values: dict[str, Value],
    keys: tuple[str, ...],
    *,
    ignored: tuple[str, ...] = (),
) -> None:
    matches = [
        r.values for r in db.rows(table) if all(r.values[k] == values[k] for k in keys)
    ]
    if not matches:
        db.insert(table, values)
    elif any(matches[0][k] != v for k, v in values.items() if k not in ignored):
        raise ValueError("Historical correction provenance conflicts")


def _physical(db: Database, item: FaceObservation) -> None:
    key = {
        "printing_id": item.printing_id,
        "face_id": item.face_id,
        "source_id": item.card.source.id,
    }
    identifier = candidate_revision_id(item)
    existing = next(
        (
            r.values
            for r in db.rows("printing_face_observation")
            if all(r.values[k] == v for k, v in key.items())
        ),
        None,
    )
    if existing is None:
        db.insert(
            "printing_face_observation",
            key
            | {"revision_id": identifier, "observed_at": item.card.source.fetched_at},
        )
    elif existing["revision_id"] != identifier:
        raise ValueError(
            "Historical observation projection differs from existing raw observation"
        )


def _populate_record(
    db: Database,
    adoption: ReplayedAdoption,
    texts: TextInterner,
    vocabulary: Vocabulary,
) -> None:
    data = adoption.record.data
    items = list(adoption.scope.contents.values())
    if adoption.predecessor_scope is not None:
        items.extend(adoption.predecessor_scope.contents.values())
    if adoption.previous is not None:
        items.append(adoption.previous)
    for item in sorted(
        items, key=lambda i: (bool(i.correction_keys), candidate_revision_id(i))
    ):
        if item.correction_keys:
            raw = item.model_copy(
                update={
                    "content": item.card.projected(item.source_index),
                    "correction_keys": (),
                }
            )
            _revision(db, raw, adoption, texts, vocabulary)
        _revision(db, item, adoption, texts, vocabulary)
    for item in items:
        _physical(
            db,
            item.model_copy(
                update={
                    "content": item.card.projected(item.source_index),
                    "correction_keys": (),
                }
            ),
        )
    populate_semantics(db, adoption, texts)
    key = {"face_id": data.face_id, "region": data.region}
    values = {
        "revision_id": candidate_revision_id(adoption.selected),
        "basis": adoption.basis,
        "decision_id": adoption.decision.id,
    }
    if any(
        all(r.values[k] == v for k, v in key.items()) for r in db.rows("face_current")
    ):
        db.update("face_current", key, values)
    else:
        db.insert("face_current", key | values)


def _freshness(
    db: Database, prepared: PreparedAdoptions, current_review: ReviewContext
) -> None:
    receipts: dict[tuple[str, Value], list[JsonValue]] = defaultdict(list)
    latest = {
        (i.record.data.face_id, i.record.data.region): i for i in prepared.replayed
    }
    for key, adoption in latest.items():
        scope = prepared.current_scopes[key]
        original = adoption.selected.model_copy(
            update={
                "content": adoption.selected.card.projected(
                    adoption.selected.source_index
                ),
                "correction_keys": (),
            }
        )
        _, refreshed, _ = prepared.reconstruction.rebuild_observation(
            scope.registry, current_review, original
        )
        if (
            refreshed.content.wording_fields()
            != adoption.selected.content.wording_fields()
            or refreshed.correction_keys != adoption.selected.correction_keys
        ):
            raise ValueError(
                "Known current correction change invalidates adopted wording"
            )
        source_id = next(
            r.values["source_id"]
            for r in db.rows("decision_source")
            if r.values["decision_id"] == adoption.decision.id
            and r.values["role"] == "wording_adoption_envelope"
        )
        receipts[adoption.decision.id, source_id].append(
            {
                "face_id": adoption.record.data.face_id,
                "region": adoption.record.data.region,
                "checked": list(adoption.record.data.checked_observation_keys),
                "current": list[JsonValue](sorted(scope.contents)),
                "candidates": [
                    list[JsonValue](v)
                    for v in sorted(
                        {
                            (
                                i.printing_id,
                                None
                                if i.content.effect is None
                                else candidate_revision_id(i),
                            )
                            for i in scope.contents.values()
                        },
                        key=lambda p: (p[0], p[1] or ""),
                    )
                ],
            }
        )
    for (decision, source), items in receipts.items():
        db.insert(
            "decision_source",
            {
                "decision_id": decision,
                "source_id": source,
                "role": "wording_adoption_checked",
                "locator": canonical({"items": sorted(items, key=canonical)}).decode(),
            },
        )


def import_adoptions(db: Database, **inputs: Unpack[AdoptionInputs]) -> InputRecord:
    """Own the transaction; the typed population entry composes with other imports."""
    with db.transaction():
        return populate_adoptions(db, **inputs)


def _support(db: Database) -> None:
    currents = {
        (r.values["face_id"], r.values["region"]) for r in db.rows("face_current")
    }
    faces: dict[Value, set[Value]] = defaultdict(set)
    for row in db.rows("face"):
        faces[row.values["card_id"]].add(row.values["id"])
    for row in db.rows("card_engine_support"):
        values = row.values
        if all(
            (face, values["region"]) in currents for face in faces[values["card_id"]]
        ):
            reasons = values["reason_codes"]
            if not isinstance(reasons, Json) or not isinstance(reasons.value, list):
                raise TypeError("Invalid support reasons")
            db.update(
                "card_engine_support",
                {"card_id": values["card_id"], "region": values["region"]},
                {
                    "reason_codes": Json(
                        [
                            reason
                            for reason in reasons.value
                            if reason != "wording_pending"
                        ]
                    ),
                    "automatic": False,
                },
            )
