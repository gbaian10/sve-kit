"""Typed exact-text staging; complete diagnostics stay separate from output eligibility."""

from typing import TYPE_CHECKING

from sve_carddb.build.rows import insert_exact
from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.core.json import SAFE_INTEGER, parse
from sve_carddb.core.provenance import BuildContext, InputRecord, input_record
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.source_corrections.importer import (
    populate_corrections,
    verify_corrections,
)
from sve_carddb.domains.source_corrections.plan import selected_records
from sve_carddb.domains.text_observations.configuration import text_configuration
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.text_observations.models import candidate_revision_id
from sve_carddb.domains.text_observations.plan import verify_plan
from sve_carddb.domains.text_observations.type_binding import type_binding
from sve_carddb.domains.text_observations.wording import mark_wording_pending

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.domains.text_observations.models import FaceObservation
    from sve_carddb.domains.text_observations.plan import TextPlan
    from sve_carddb.domains.text_observations.vocabulary import Vocabulary


def stat(value: str) -> int | None:
    """Keep absent and symbolic stats unknown, retaining exact raw symbols in reports."""
    if value in {"-", "X", "Q"}:
        return None
    if not value.isascii() or not value.isdecimal() or int(value) > SAFE_INTEGER:
        raise ValueError("Unrepresentable source statistic")
    return int(value)


def revision_id(item: FaceObservation) -> str:
    """Use the shared content/correction recipe for stored candidate identities."""
    return candidate_revision_id(item)


def populate_vocabulary(
    db: Database, texts: TextInterner, vocabulary: Vocabulary
) -> None:
    """Register the explicit raw-to-code bindings without guessing source types."""
    vocabulary.verify()
    if vocabulary.terms:
        for term in vocabulary.terms:
            insert_exact(
                db,
                "vocabulary",
                {
                    "kind": term.kind,
                    "code": term.code,
                    "active": term.active,
                    "label_unit_id": texts.intern(term.label),
                },
                ("kind", "code"),
            )
        return
    existing = {
        (r.values["kind"], r.values["code"]): r.values for r in db.rows("vocabulary")
    }
    labels: dict[tuple[str, str], set[str]] = {}
    for binding in vocabulary.bindings:
        labels.setdefault((binding.kind, binding.code), set()).add(
            texts.intern(
                LocalizedText(
                    lang="ja" if binding.region == "jp" else "en", text=binding.raw
                )
            )
        )
    for binding in sorted(
        vocabulary.bindings,
        key=lambda item: (item.kind, item.code, item.region, item.raw),
    ):
        key = binding.kind, binding.code
        if key in existing:
            if (
                existing[key]["active"] is not True
                or existing[key]["label_unit_id"] not in labels[key]
            ):
                raise ValueError("Existing vocabulary conflicts with explicit binding")
            continue
        values: dict[str, Value] = {
            "kind": binding.kind,
            "code": binding.code,
            "active": True,
            "label_unit_id": texts.intern(
                LocalizedText(
                    lang="ja" if binding.region == "jp" else "en", text=binding.raw
                )
            ),
        }
        db.insert("vocabulary", values)
        existing[key] = values


def populate_revision(  # ruff: ignore[too-many-arguments] -- the original and corrected variants share one writer with explicit supersession
    db: Database,
    item: FaceObservation,
    ordinal: int,
    texts: TextInterner,
    vocabulary: Vocabulary,
    *,
    supersedes_id: str | None = None,
    plan: TextPlan | None = None,
) -> None:
    """Store exact candidate content with explicit provenance inside a transaction."""
    content = item.content
    assert content.effect is not None
    language = "ja" if item.region == "jp" else "en"
    if plan is None:
        kind = vocabulary.lookup(item.region, "type", content.type_raw)
    else:
        kind = type_binding(plan, item, vocabulary)
    card_class = (
        None
        if content.class_raw == "-"
        else vocabulary.lookup(item.region, "class", content.class_raw).code
    )
    identifier = revision_id(item)
    cost, attack, defense = (stat(value) for value in content.stats)
    db.insert(
        "face_revision",
        {
            "id": identifier,
            "face_id": item.face_id,
            "region": item.region,
            "revision": ordinal,
            "effective_from": None,
            "effective_until": None,
            "temporal_status": "unknown",
            "observed_at": item.card.source.fetched_at,
            "change_kind": "source_correction"
            if item.correction_keys
            else "initial"
            if ordinal == 1
            else "wording",
            "name_unit_id": texts.intern(
                LocalizedText(lang=language, text=content.name)
            ),
            "effect_unit_id": texts.intern(
                LocalizedText(lang=language, text=content.effect)
            ),
            "class_code": card_class,
            "type_code": kind.code,
            "cost": cost,
            "attack": attack,
            "defense": defense,
            "source_id": item.card.source.id,
            "decision_id": None,
            "supersedes_id": supersedes_id,
        },
    )
    for position, section in enumerate(content.sections):
        db.insert(
            "face_text_section",
            {
                "revision_id": identifier,
                "ordinal": position,
                "text_unit_id": texts.intern(
                    LocalizedText(lang=language, text=section)
                ),
                "kind": "unknown",
                "decision_id": None,
            },
        )
    for trait in sorted(set(content.traits)):
        db.insert(
            "face_trait",
            {
                "revision_id": identifier,
                "trait_code": vocabulary.lookup(item.region, "trait", trait).code,
            },
        )
    if content.title is not None:
        db.insert(
            "face_title",
            {
                "revision_id": identifier,
                "title_code": vocabulary.lookup(
                    item.region, "title", content.title
                ).code,
            },
        )
    for code in kind.special_kinds:
        db.insert(
            "face_special_kind", {"revision_id": identifier, "special_kind_code": code}
        )


def _physical(db: Database, item: FaceObservation, texts: TextInterner) -> None:
    db.insert(
        "printing_face_observation",
        {
            "printing_id": item.printing_id,
            "face_id": item.face_id,
            "source_id": item.card.source.id,
            "revision_id": revision_id(item),
            "observed_at": item.card.source.fetched_at,
        },
    )
    if item.content.flavor is not None:
        db.update(
            "printing_face",
            {"printing_id": item.printing_id, "face_id": item.face_id},
            {
                "flavor_unit_id": texts.intern(
                    LocalizedText(
                        lang="ja" if item.region == "jp" else "en",
                        text=item.content.flavor,
                    )
                )
            },
        )


def _intern_observations(texts: TextInterner, plan: TextPlan) -> None:
    for item in plan.observations:
        language = "ja" if item.region == "jp" else "en"
        for value in (
            item.content.name,
            item.content.effect,
            *item.content.sections,
            item.content.flavor,
        ):
            if value is not None:
                texts.intern(LocalizedText(lang=language, text=value))


def _groups_to_database(
    db: Database,
    plan: TextPlan,
    selected: set[tuple[str, str, str]],
    parents: dict[tuple[Value, Value], Mapping[str, Value]],
    texts: TextInterner,
    vocabulary: Vocabulary,
) -> None:
    raw_groups: dict[tuple[str, str], list[FaceObservation]] = {}
    for item in plan.observations:
        raw_groups.setdefault((item.face_id, item.region), []).append(item)
    for group in plan.groups:
        variants: dict[str, FaceObservation] = {}
        raw = raw_groups[group.face_id, group.region]
        for item in (*raw, *group.observations):
            if (item.printing_id, item.face_id, item.card.source.id) in selected:
                if (item.printing_id, item.face_id) not in parents or parents[
                    item.printing_id, item.face_id
                ]["source_id"] != item.card.source.id:
                    raise ValueError("Observation printing/face/source parent mismatch")
                variants.setdefault(revision_id(item), item)
        ordered = sorted(
            variants.values(),
            key=lambda item: (bool(item.correction_keys), revision_id(item)),
        )
        for ordinal, item in enumerate(ordered, 1):
            original = next(
                source for source in raw if source.printing_id == item.printing_id
            )
            populate_revision(
                db,
                item,
                ordinal,
                texts,
                vocabulary,
                supersedes_id=revision_id(original) if item.correction_keys else None,
                plan=plan,
            )
        current = group.current()
        if current is not None:
            db.insert(
                "face_current",
                {
                    "face_id": group.face_id,
                    "region": group.region,
                    "revision_id": revision_id(current),
                    "basis": "latest_observed_no_errata",
                    "decision_id": None,
                },
            )


def populate_text_observations(
    db: Database,
    plan: TextPlan,
    *,
    build: BuildContext,
    vocabulary: Vocabulary,
    published: tuple[LocalizedText, ...],
) -> InputRecord:
    """Populate in a caller-owned transaction, preserving missing-effect source uses."""
    verify_plan(plan)
    if plan.corrections is None and selected_records(plan.identity):
        raise ValueError("Scoped source corrections require pinned image evidence")
    if any(
        db.rows(table)
        for table in ("face_revision", "face_current", "printing_face_observation")
    ):
        raise ValueError("Initial observations require a fresh text staging graph")
    configuration = parse(build.configuration.encode())
    required = text_configuration(plan, vocabulary, published)
    if not isinstance(configuration, dict) or any(
        configuration.get(key) != value for key, value in required.items()
    ):
        raise ValueError("Explicit text observation configuration is not pinned")
    expected = plan.source_uses()
    insert_raw_sources(db, (use.source for use in expected))
    texts = TextInterner(db, published=published)
    populate_vocabulary(db, texts, vocabulary)
    _intern_observations(texts, plan)
    printings = {row.values["id"]: row.values for row in db.rows("printing")}
    for item in plan.materialized():
        parent = printings.get(item.printing_id)
        if parent is None or any(
            parent[key] != expected
            for key, expected in (
                ("card_id", item.card_id),
                ("region", item.region),
                ("card_no", item.card_no),
                ("source_id", item.card.source.id),
            )
        ):
            raise ValueError("Observation printing identity parent mismatch")
    parents = {
        (row.values["printing_id"], row.values["face_id"]): row.values
        for row in db.rows("printing_face")
    }
    materialized = plan.materialized()
    selected = {
        (item.printing_id, item.face_id, item.card.source.id) for item in materialized
    }
    _groups_to_database(db, plan, selected, parents, texts, vocabulary)
    for item in materialized:
        _physical(db, item, texts)
    populate_corrections(db, plan, texts)
    if plan.corrections is not None:
        verify_corrections(db, plan, vocabulary)
    mark_wording_pending(db, plan)
    return input_record(build, expected)


def import_text_observations(
    db: Database,
    plan: TextPlan,
    *,
    build: BuildContext,
    vocabulary: Vocabulary,
    published: tuple[LocalizedText, ...],
) -> InputRecord:
    """Roll back the entire observation graph on metadata, collision or row failures."""
    with db.transaction():
        return populate_text_observations(
            db, plan, build=build, vocabulary=vocabulary, published=published
        )
