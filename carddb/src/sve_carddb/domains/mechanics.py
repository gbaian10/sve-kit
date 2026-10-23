"""Compose keyword producers from exact current owners, independently of rendering."""

from collections import Counter
from dataclasses import dataclass
from typing import TYPE_CHECKING, Annotated, Literal, cast

from pydantic import Field, JsonValue, ValidationError, field_validator, model_validator

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
from sve_carddb.domains.mechanic_batch import FIRST_BATCH_CARDS
from sve_carddb.domains.mechanic_coverage import (
    FieldCheck,
    Inspection,
    project_current,
    store_evidence,
)
from sve_carddb.domains.mechanic_grants import ADAPTER as GRANTS_ADAPTER
from sve_carddb.domains.mechanic_grants import apply as apply_grants
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
    from sve_carddb.domains.translations.models import Origin
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
        return {
            "adapter": ADAPTER,
            "registry_sha256": digest(self.exact),
            "grants_adapter": GRANTS_ADAPTER,
            "grants_batch_sha256": digest(
                canonical(list[JsonValue](sorted(FIRST_BATCH_CARDS)))
            ),
        }


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
    first_batch_cards: frozenset[str] = FIRST_BATCH_CARDS


def _adoption(
    db: Database,
    decision: Adoption,
    source_id: str,
    category: Literal["keyword_mapping", "mechanic_suppression"],
) -> None:
    insert_exact(
        db,
        "decision",
        {
            "id": decision.id,
            "state": decision.state,
            "scope": "record",
            "category": category,
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


def _register(db: Database, inputs: Inputs, revision: str) -> str:
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
    _adoption(db, inputs.registry.decision, source_id, "keyword_mapping")
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
                origin=cast("Origin", value["origin"]),
                low_confidence=value["low_confidence"] is True
                or term["low_confidence"] is True,
                semantic_variant="keyword:" + keyword.id,
                source_id=None
                if value["source_id"] is None
                else str(value["source_id"]),
            )
    return source_id


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


def apply(  # ruff: ignore[complex-structure,too-many-locals] -- one owner-checked declaration pass retains all rejection reasons without render dependencies
    db: Database, inputs: Inputs, settings: Settings
) -> dict[str, JsonValue]:
    """Store every producer before suppression and one shared coverage projection."""
    registry_source_id = _register(db, inputs, settings.authored_revision)
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
    positives: dict[str, dict[str, Value]] = {}
    checked: dict[tuple[str, str], set[FieldCheck]] = {}
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
                        "Self keyword declaration requires only typed keyword concepts"
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
                positives[identifier] = {
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
                }
                checked.setdefault((cid, keyword), set()).add(
                    FieldCheck(
                        face_id=str(current["face_id"]),
                        revision_id=str(current["revision_id"]),
                        field="effect",
                        ordinal=None,
                        relation="has",
                        whole_field=False,
                    )
                )
    store_evidence(
        db,
        tuple(positives.values()),
        tuple(
            Inspection(
                card_id=cid,
                keyword_id=keyword,
                scope="shared",
                source_kind="manual",
                source_text_hash=hashes[cid],
                producer_version=version,
                decision_id=inputs.registry.decision.id,
                dsl_id=None,
                checks=tuple(sorted(checks, key=lambda check: check.model_dump_json())),
            )
            for (cid, keyword), checks in sorted(checked.items())
        ),
    )
    grants = apply_grants(db, inputs, settings, hashes)
    insert_raw_sources(db, (use.source for use in settings.sources.uses))
    tags = {str(row.values["id"]): row.values for row in db.rows("card_mechanic")}
    _suppress(db, inputs.registry.suppressions, tags, registry_source_id, reasons)
    project_current(db, hashes, frozenset({version, str(grants["producer_version"])}))
    return {
        "adapter": ADAPTER,
        "description": "Self keyword declarations: body parts containing only keywords.",
        "producer_version": version,
        "grants": grants,
        "keywords": len(keywords),
        "positive_tags": len(tags),
        "projection_rows": len(db.rows("mechanic_projection")),
        "coverage_rows": len(db.rows("card_mechanic_coverage")),
        "complete_cards": 0,
        "reasons": dict[str, JsonValue](sorted(reasons.items())),
    }


def _suppress(
    db: Database,
    suppressions: tuple[Suppression, ...],
    tags: dict[str, Mapping[str, Value]],
    registry_source_id: str,
    reasons: Counter[str],
) -> None:
    for suppression in suppressions:
        tag = tags.get(suppression.target_tag_id)
        if (
            tag is None
            or tag["source_text_hash"] != suppression.source_text_hash
            or tag["producer_version"] != suppression.producer_version
        ):
            reasons["stale_suppression"] += 1
            continue
        _adoption(
            db,
            suppression.decision,
            registry_source_id,
            "mechanic_suppression",
        )
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
