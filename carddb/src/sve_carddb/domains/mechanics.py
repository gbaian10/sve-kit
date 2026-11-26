"""Register keyword concepts, then project card mechanics from programs and wording."""

from collections import Counter
from dataclasses import dataclass
from typing import TYPE_CHECKING, Annotated, Literal, cast

from pydantic import Field, JsonValue, ValidationError, field_validator, model_validator

from sve_carddb.build.rows import insert_exact
from sve_carddb.core.authored import read
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.domains.ability_types import ADAPTER as ABILITY_ADAPTER
from sve_carddb.domains.ability_types import apply as apply_abilities
from sve_carddb.domains.keyword_self import ADAPTER as SELF_ADAPTER
from sve_carddb.domains.keyword_self import apply as apply_self
from sve_carddb.domains.keyword_self import current_fields
from sve_carddb.domains.mechanic_coverage import project_current
from sve_carddb.domains.mechanic_dsl import ADAPTER as DSL_ADAPTER
from sve_carddb.domains.mechanic_dsl import apply as apply_dsl
from sve_carddb.domains.printed_mechanisms import ADAPTER as PRINTED_ADAPTER
from sve_carddb.domains.printed_mechanisms import apply as apply_printed
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.translations.direct import write

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.build import Database, Value
    from sve_carddb.domains.translations.models import Origin

FILE = "rules/keyword-registry.yaml"
REGISTRY_PARSER = "keyword-registry-v1"
Code = Annotated[str, Field(pattern=r"^[a-z][a-z0-9_-]*\Z")]


class Adoption(RecordData):
    id: Text
    state: Literal["confirmed"]
    note: Text


class RuleName(RecordData):
    ja: Text
    rule: Text | None = None
    aliases: tuple[Text, ...] = ()


class NamedRule(RuleName):
    role: Literal["keyword", "ability-label", "counter"]


class Keyword(RecordData):
    id: Text
    code: Code
    term_id: Text
    term_category: Literal["keyword", "ability", "rule_term"] = "keyword"
    english: Text | None = None
    rule_name: RuleName | None = None

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
    rule_names: dict[Code, NamedRule] = Field(default_factory=dict)
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
        if self.rule_names.keys() & {record.code for record in self.records}:
            raise ValueError("A rule name may only be registered once per code")
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
            "registry_sha256": digest(self.exact),
            "dsl_adapter": DSL_ADAPTER,
            "self_adapter": SELF_ADAPTER,
            "ability_adapter": ABILITY_ADAPTER,
            "printed_adapter": PRINTED_ADAPTER,
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


def _registry_source(inputs: Inputs, revision: str) -> str:
    return (
        "authored:mechanics:"
        + digest(canonical([revision, FILE, digest(inputs.exact)]))[7:]
    )


def register(db: Database, inputs: Inputs, revision: str) -> None:
    """Store the adopted keyword concepts and their glossary names."""
    source_id = _registry_source(inputs, revision)
    insert_exact(
        db,
        "source_record",
        {
            "id": source_id,
            "kind": "authored",
            "sha256": digest(inputs.exact),
            "authored_path": "authored/" + FILE,
            "authored_revision": revision,
            "parser_version": REGISTRY_PARSER,
        },
        ("id",),
    )
    _adoption(db, inputs.registry.decision, source_id, "keyword_mapping")
    terms = {str(row.values["id"]): row.values for row in db.rows("glossary_term")}
    interner = TextInterner(db)
    for keyword in inputs.registry.records:
        term = terms.get(keyword.term_id)
        if term is None or term["category"] != keyword.term_category:
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


def apply(db: Database, inputs: Inputs, revision: str) -> dict[str, JsonValue]:
    """Store program evidence before suppression and one shared coverage projection."""
    hashes = source_bundles(db)
    native_fields = current_fields(db)
    dsl = apply_dsl(db, inputs, hashes, native_fields)
    tags = {str(row.values["id"]): row.values for row in db.rows("card_mechanic")}
    reasons: Counter[str] = Counter()
    _suppress(
        db,
        inputs.registry.suppressions,
        tags,
        _registry_source(inputs, revision),
        reasons,
    )
    project_current(db, hashes, frozenset({str(dsl["producer_version"])}))
    return {
        "dsl": dsl,
        "self": apply_self(db, inputs, native_fields),
        "ability_types": apply_abilities(db, native_fields),
        "printed_mechanisms": apply_printed(db, native_fields),
        "keywords": len(inputs.registry.records),
        "positive_tags": len(tags),
        "projection_rows": len(db.rows("mechanic_projection")),
        "coverage_rows": len(db.rows("card_mechanic_coverage")),
        "complete_cards": sum(
            row.values["complete_all"] is True
            for row in db.rows("card_mechanic_coverage")
        ),
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
