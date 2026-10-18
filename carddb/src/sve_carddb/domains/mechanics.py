"""Derive sparse H1 labels from exact current source owners, independently of rendering."""

from collections import Counter
from dataclasses import dataclass
from typing import TYPE_CHECKING, Annotated, Literal

from pydantic import Field, JsonValue, ValidationError, field_validator, model_validator

from sve_carddb.build import Json
from sve_carddb.build.rows import insert_exact
from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.contracts.four_layer import (
    FaceRevisionOwner,
    GlossaryReference,
    OwnerField,
)
from sve_carddb.core.authored import read
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.translations.direct import write
from sve_carddb.domains.translations.four_layer_analysis import analyze_source
from sve_carddb.domains.translations.four_layer_normalizer import VERSION as NORMALIZER
from sve_carddb.domains.translations.four_layer_sources import (
    card_source,
    descriptor,
    empty_field,
)

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.build import Database, Value
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.sources import Sources

FILE = "rules/keyword-registry.yaml"
ADAPTER = "manual-keywords-h1-v1"
Code = Annotated[str, Field(pattern=r"^[a-z][a-z0-9_-]*\Z")]


class Adoption(RecordData):
    id: Text
    state: Literal["confirmed"]
    note: Text


class Keyword(RecordData):
    id: Text
    code: Code
    term_id: Text

    @model_validator(mode="after")
    def permanent_id(self) -> Keyword:
        """An explicit registration owns its ID; spelling never allocates an identity."""
        if self.id != "kw:" + self.code:
            raise ValueError("Keyword ID must match its registered code")
        return self


class Suppression(RecordData):
    id: Text
    target_tag_id: Text
    source_text_hash: Hash
    producer_version: Text
    reason: Text
    decision: Adoption


class Registry(RecordData):
    format: Literal[1]
    kind: Literal["keyword_registry"]
    decision: Adoption
    records: tuple[Keyword, ...]
    suppressions: tuple[Suppression, ...] = ()

    @field_validator("format", mode="before")
    @classmethod
    def integer_format(cls, value: object) -> object:
        """Reject YAML booleans before Literal can coerce them to integer one."""
        if type(value) is not int:
            raise ValueError("Keyword registry format must be an integer")
        return value

    @model_validator(mode="after")
    def unique_keys(self) -> Registry:
        """A source spelling or review must not silently replace another registration."""
        for field in ("id", "code", "term_id"):
            if len({getattr(record, field) for record in self.records}) != len(
                self.records
            ):
                raise ValueError("Duplicate keyword registration " + field)
        if len({record.id for record in self.suppressions}) != len(self.suppressions):
            raise ValueError("Duplicate mechanic suppression ID")
        return self


@dataclass(frozen=True)
class Inputs:
    registry: Registry
    exact: bytes

    def configuration(self) -> dict[str, JsonValue]:
        """Pin accepted authored bytes without recording a machine-local location."""
        return {"adapter": ADAPTER, "registry_sha256": digest(self.exact)}


def load(authored: Path) -> Inputs:
    """Read the declared registry through the bounded, symlink-safe authored boundary."""
    exact, content = read(authored / FILE, root=authored)
    try:
        registry = Registry.model_validate_json(content)
    except ValidationError as error:
        locations = [
            ".".join(map(str, item["loc"])) + ":" + item["type"]
            for item in error.errors(include_input=False, include_context=False)
        ]
        raise ValueError("Invalid keyword registry: " + ";".join(locations)) from None
    return Inputs(registry, exact)


@dataclass(frozen=True)
class Settings:
    sources: Sources
    classifier: Classifier
    batch_id: str
    authored_revision: str


def _adoption(db: Database, decision: Adoption, source_id: str) -> None:
    insert_exact(
        db,
        "decision",
        {
            "id": decision.id,
            "state": decision.state,
            "scope": "record",
            "category": "keyword_mapping",
            "note": decision.note,
        },
        ("id",),
    )
    insert_exact(
        db,
        "decision_source",
        {
            "decision_id": decision.id,
            "source_id": source_id,
            "role": "authored",
            "locator": None,
            "quote": None,
        },
        ("decision_id", "source_id", "role"),
    )


def _register(db: Database, inputs: Inputs, revision: str) -> None:
    source_id = (
        "authored:mechanics:"
        + digest(canonical([revision, FILE, digest(inputs.exact)]))[7:]
    )
    insert_exact(
        db,
        "source_record",
        {
            "id": source_id,
            "kind": "authored",
            "sha256": digest(inputs.exact),
            "authored_path": "authored/" + FILE,
            "authored_revision": revision,
            "parser_version": ADAPTER,
        },
        ("id",),
    )
    _adoption(db, inputs.registry.decision, source_id)
    terms = {str(row.values["id"]): row.values for row in db.rows("glossary_term")}
    interner = TextInterner(db)
    for keyword in inputs.registry.records:
        term = terms.get(keyword.term_id)
        if term is None or term["category"] != "keyword":
            raise ValueError(
                "Keyword registration requires its existing keyword concept: "
                + keyword.term_id
            )
        db.insert(
            "keyword",
            {
                "id": keyword.id,
                "code": keyword.code,
                "term_id": keyword.term_id,
                "kind": "printed_keyword",
                "definition_unit_id": None,
                "decision_id": inputs.registry.decision.id,
            },
        )
        source_unit = interner.intern(
            LocalizedText(lang="ja", text=str(term["source_ja"]))
        )
        for row in db.select(
            "glossary_translation",
            db.columns("glossary_translation"),
            where={"term_id": keyword.term_id},
        ):
            value = row.values
            write(
                db,
                {"keyword_id": keyword.id},
                field="label",
                lang=str(value["lang"]),
                source_unit_id=source_unit,
                text=str(value["text"]),
                origin="official"
                if value["origin"] == "official"
                else "machine"
                if value["origin"] == "machine"
                else "project",
                low_confidence=value["low_confidence"] is True
                or term["low_confidence"] is True,
                semantic_variant="keyword:" + keyword.id,
                source_id=None
                if value["source_id"] is None
                else str(value["source_id"]),
            )
    for suppression in inputs.registry.suppressions:
        _adoption(db, suppression.decision, source_id)


def producer_version(inputs: Inputs, classifier: Classifier) -> str:
    """Presentation choices and unrelated commits cannot invalidate source labels."""
    payload: dict[str, JsonValue] = {
        "adapter": ADAPTER,
        "normalizer": NORMALIZER,
        "rules": classifier.rules.model_dump(mode="json"),
        "keywords": [
            record.model_dump(mode="json")
            for record in sorted(inputs.registry.records, key=lambda record: record.id)
        ],
        "concepts": [
            [term.id, term.category, term.source_ja]
            for term in sorted(classifier.terms.values(), key=lambda term: term.id)
            if term.category == "keyword"
        ],
    }
    return ADAPTER + ":" + digest(canonical(payload))


def source_bundles(db: Database) -> dict[str, str]:
    """Include every face, current basis, main field and section, including missing faces."""
    units = {
        str(row.values["id"]): str(row.values["content_hash"])
        for row in db.rows("text_unit")
    }
    revisions = {str(row.values["id"]): row.values for row in db.rows("face_revision")}
    current = {
        str(row.values["face_id"]): row.values
        for row in db.rows("face_current")
        if row.values["region"] == "jp"
    }
    sections: dict[str, list[JsonValue]] = {}
    for row in db.rows("face_text_section"):
        value = row.values
        sections.setdefault(str(value["revision_id"]), []).append(
            [
                str(value["ordinal"]),
                str(value["kind"]),
                units[str(value["text_unit_id"])],
            ]
        )
    faces: dict[str, list[JsonValue]] = {}
    for row in sorted(db.rows("face"), key=lambda row: str(row.values["id"])):
        face = row.values
        selected = current.get(str(face["id"]))
        revision = None if selected is None else revisions[str(selected["revision_id"])]
        faces.setdefault(str(face["card_id"]), []).append(
            {
                "face_id": str(face["id"]),
                "ordinal": str(face["ordinal"]),
                "side": str(face["side"]),
                "basis": None if selected is None else str(selected["basis"]),
                "revision_id": None if revision is None else str(revision["id"]),
                "source_id": None if revision is None else str(revision["source_id"]),
                "effect_hash": None
                if revision is None
                else units[str(revision["effect_unit_id"])],
                "sections": []
                if revision is None
                else sections.get(str(revision["id"]), []),
            }
        )
    return {
        cid: digest(
            canonical(
                {
                    "recipe": "mechanic-source-bundle-v1",
                    "card_id": cid,
                    "scope": "shared",
                    "faces": fields,
                }
            )
        )
        for cid, fields in faces.items()
    }


def _shortest(selected: set[str], universe: set[str]) -> tuple[str, list[JsonValue]]:
    complement = universe - selected
    return (
        ("include", list[JsonValue](sorted(selected)))
        if len(selected) <= len(complement)
        else ("exclude", list[JsonValue](sorted(complement)))
    )


def project_current(  # ruff: ignore[complex-structure] -- exact freshness and confirmed suppressions precede each sparse projection
    db: Database, hashes: Mapping[str, str], version: str
) -> None:
    """Only exact fresh evidence can enter the public sparse sets."""
    decisions = {
        str(row.values["id"]): row.values["state"] for row in db.rows("decision")
    }
    overrides: dict[str, list[Mapping[str, Value]]] = {}
    for row in db.rows("mechanic_override"):
        overrides.setdefault(str(row.values["target_tag_id"]), []).append(row.values)
    positive: dict[tuple[str, str, str], tuple[set[str], set[str]]] = {}
    for row in db.rows("card_mechanic"):
        tag = row.values
        if (
            tag["scope"] != "shared"
            or tag["source_text_hash"] != hashes.get(str(tag["card_id"]))
            or tag["producer_version"] != version
        ):
            continue
        if any(
            override["source_text_hash"] == tag["source_text_hash"]
            and override["producer_version"] == tag["producer_version"]
            and decisions[str(override["decision_id"])] == "confirmed"
            for override in overrides.get(str(tag["id"]), [])
        ):
            continue
        key = (str(tag["card_id"]), str(tag["keyword_id"]), str(tag["scope"]))
        relations, actions = positive.setdefault(key, (set(), set()))
        relations.add(str(tag["relation"]))
        if tag["action"] is not None:
            actions.add(str(tag["action"]))
    for (cid, keyword, scope), (relations, actions) in sorted(positive.items()):
        db.insert(
            "mechanic_projection",
            {
                "card_id": cid,
                "keyword_id": keyword,
                "scope": scope,
                "relations": Json(list[JsonValue](sorted(relations))),
                "actions": Json(list[JsonValue](sorted(actions))),
            },
        )
    universe = {str(row.values["id"]) for row in db.rows("keyword")}
    partial: dict[tuple[str, str], set[str]] = {}
    for row in db.rows("mechanic_coverage"):
        value = row.values
        if (
            value["scope"] != "shared"
            or value["source_text_hash"] != hashes.get(str(value["card_id"]))
            or value["producer_version"] != version
        ):
            continue
        if value["state"] != "partial":
            raise ValueError("The H1 producer only accepts partial mechanism coverage")
        partial.setdefault((str(value["card_id"]), str(value["scope"])), set()).add(
            str(value["keyword_id"])
        )
    for (cid, scope), keywords in sorted(partial.items()):
        mode, ids = _shortest(keywords, universe)
        db.insert(
            "card_mechanic_coverage",
            {
                "card_id": cid,
                "scope": scope,
                "complete_all": False,
                "complete_mode": "include",
                "complete_keyword_ids": Json([]),
                "partial_mode": mode,
                "partial_keyword_ids": Json(ids),
            },
        )


def apply(  # ruff: ignore[complex-structure,too-many-locals,too-many-branches] -- one owner-checked H1 pass retains all rejection reasons without render dependencies
    db: Database, inputs: Inputs, settings: Settings
) -> dict[str, JsonValue]:
    """H1 proves positive declarations; every other relation remains incomplete."""
    _register(db, inputs, settings.authored_revision)
    hashes = source_bundles(db)
    version = producer_version(inputs, settings.classifier)
    keywords = {record.term_id: record.id for record in inputs.registry.records}
    faces = {
        str(row.values["id"]): str(row.values["card_id"]) for row in db.rows("face")
    }
    cards = {
        str(row.values["id"]): row.values["identity_state"] for row in db.rows("card")
    }
    reasons: Counter[str] = Counter()
    checked: set[tuple[str, str]] = set()
    for row in db.rows("face_current"):
        current = row.values
        if current["region"] != "jp":
            continue
        cid = faces[str(current["face_id"])]
        if cards[cid] != "confirmed":
            reasons["unconfirmed_identity"] += 1
            continue
        field = OwnerField(
            owner=FaceRevisionOwner(
                kind="face_revision", revision_id=str(current["revision_id"])
            ),
            field="effect",
            ordinal=None,
        )
        if empty_field(db, field):
            reasons["empty_source_field"] += 1
            continue
        source = descriptor(db, settings.sources, field, settings.batch_id)
        original = card_source(db, settings.sources, source)
        derived = analyze_source(original.text, source, settings.classifier)
        for part, found in zip(derived.field.parts, derived.recognized, strict=True):
            if part.source_span.role != "body":
                continue
            if (
                found.issues
                or found.semantics is None
                or found.semantics.key != "card_keywords.v2"
            ):
                reasons["not_h1_declaration"] += 1
                continue
            frame, binding = found.bind(source, part)
            part.verify(original.text, frame, binding, settings.classifier.domains)
            for slot in frame.leaf_schema.slots:
                concept = binding.values[slot.name]
                if (
                    slot.type != "Concept"
                    or slot.role != "keyword"
                    or not isinstance(concept, GlossaryReference)
                ):
                    raise ValueError(
                        "H1 declaration requires only typed keyword concepts"
                    )
                keyword = keywords.get(concept.key)
                if keyword is None:
                    reasons["unregistered_keyword"] += 1
                    continue
                identifier = (
                    "mechanic:"
                    + digest(
                        canonical(
                            {
                                "card_id": cid,
                                "keyword_id": keyword,
                                "scope": "shared",
                                "relation": "has",
                                "source_text_hash": hashes[cid],
                                "producer_version": version,
                                "binding_id": binding.id,
                            }
                        )
                    )[7:]
                )
                insert_exact(
                    db,
                    "card_mechanic",
                    {
                        "id": identifier,
                        "card_id": cid,
                        "keyword_id": keyword,
                        "relation": "has",
                        "action": None,
                        "scope": "shared",
                        "source_kind": "manual",
                        "source_text_hash": hashes[cid],
                        "producer_version": version,
                        "dsl_id": None,
                        "source_id": original.source_id,
                        "decision_id": inputs.registry.decision.id,
                    },
                    ("id",),
                )
                checked.add((cid, keyword))
    insert_raw_sources(db, (use.source for use in settings.sources.uses))
    for cid, keyword in sorted(checked):
        db.insert(
            "mechanic_coverage",
            {
                "card_id": cid,
                "keyword_id": keyword,
                "scope": "shared",
                "state": "partial",
                "source_kind": "manual",
                "source_text_hash": hashes[cid],
                "producer_version": version,
                "decision_id": inputs.registry.decision.id,
                "dsl_id": None,
            },
        )
    tags = {str(row.values["id"]): row.values for row in db.rows("card_mechanic")}
    for suppression in inputs.registry.suppressions:
        tag = tags.get(suppression.target_tag_id)
        if (
            tag is None
            or tag["source_text_hash"] != suppression.source_text_hash
            or tag["producer_version"] != suppression.producer_version
        ):
            reasons["stale_suppression"] += 1
            continue
        db.insert(
            "mechanic_override",
            {
                "id": suppression.id,
                "target_tag_id": suppression.target_tag_id,
                "operation": "suppress",
                "source_text_hash": suppression.source_text_hash,
                "producer_version": suppression.producer_version,
                "reason": suppression.reason,
                "decision_id": suppression.decision.id,
            },
        )
    project_current(db, hashes, version)
    return {
        "adapter": ADAPTER,
        "producer_version": version,
        "keywords": len(keywords),
        "positive_tags": len(tags),
        "projection_rows": len(db.rows("mechanic_projection")),
        "coverage_rows": len(db.rows("card_mechanic_coverage")),
        "complete_cards": 0,
        "reasons": dict[str, JsonValue](sorted(reasons.items())),
    }
