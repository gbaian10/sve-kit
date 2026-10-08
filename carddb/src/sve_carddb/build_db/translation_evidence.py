"""Optional #51 closure; no template renderer, art, voice or runtime routing."""

from sve_carddb.build_db.domains import DATE, HASH, LANG
from sve_carddb.build_db.model import (
    Check,
    Column,
    ForeignKey,
    Kind,
    QueryCheck,
    Table,
    Unique,
)


def _fk(column: str, table: str) -> ForeignKey:
    return ForeignKey((column,), table, ("id",))


QUALITY = (
    Column("authored_source_id", Kind.ID),
    Column("record_key", Kind.TEXT),
    Column("origin", Kind.TEXT, choices=("official", "project", "machine")),
    Column("low_confidence", Kind.BOOL),
)
SOURCE = ForeignKey(("authored_source_id",), "source_record", ("id",))

TABLES = (
    Table(
        "digital_card",
        (
            Column("id", Kind.ID),
            Column("game", Kind.TEXT, choices=("sv1", "svwb")),
            Column("official_id", Kind.TEXT),
            Column("base_card_id", Kind.ID, nullable=True),
            Column("original_card_id", Kind.ID, nullable=True),
            Column("resource_id", Kind.TEXT, nullable=True),
            Column("is_token", Kind.BOOL, nullable=True),
            Column("source_id", Kind.ID),
        ),
        ("id",),
        foreign_keys=(
            _fk("base_card_id", "digital_card"),
            _fk("original_card_id", "digital_card"),
            _fk("source_id", "source_record"),
        ),
        unique=(Unique(("game", "official_id")),),
        checks=(
            Check(
                "(game = 'sv1' AND sve_fullmatch('[0-9]{9}', official_id)) OR (game = 'svwb' AND sve_fullmatch('[0-9]{8}', official_id))"
            ),
        ),
    ),
    Table(
        "digital_face",
        (
            Column("id", Kind.ID),
            Column("digital_card_id", Kind.ID),
            Column("phase", Kind.TEXT, choices=("normal", "evolved", "super_evolved")),
            Column("source_id", Kind.ID),
        ),
        ("id",),
        foreign_keys=(
            _fk("digital_card_id", "digital_card"),
            _fk("source_id", "source_record"),
        ),
        unique=(Unique(("digital_card_id", "phase")),),
    ),
    Table(
        "digital_text",
        (
            Column("digital_face_id", Kind.ID),
            Column("lang", Kind.ID, pattern=LANG),
            Column("name_unit_id", Kind.ID),
            Column("effect_unit_id", Kind.ID, nullable=True),
            Column("flavor_unit_id", Kind.ID, nullable=True),
            Column("cv", Kind.TEXT, nullable=True),
        ),
        ("digital_face_id", "lang"),
        foreign_keys=(
            _fk("digital_face_id", "digital_face"),
            _fk("name_unit_id", "text_unit"),
            _fk("effect_unit_id", "text_unit"),
            _fk("flavor_unit_id", "text_unit"),
        ),
        query_checks=(
            QueryCheck(
                "digital_text_language",
                "SELECT 1 FROM digital_text AS d JOIN text_unit AS t ON t.id IN (d.name_unit_id,d.effect_unit_id,d.flavor_unit_id) WHERE d.lang != t.lang LIMIT 1",
                ("digital_text", "text_unit"),
            ),
        ),
    ),
    Table(
        "digital_link",
        (
            Column("id", Kind.ID),
            Column("card_id", Kind.ID),
            Column("face_id", Kind.ID, nullable=True),
            Column("digital_card_id", Kind.ID),
            Column("digital_face_id", Kind.ID, nullable=True),
            Column(
                "relation",
                Kind.TEXT,
                choices=("same_card", "same_character", "name_only"),
            ),
            Column(
                "effect_similarity",
                Kind.TEXT,
                nullable=True,
                choices=("near_identical", "core_kept", "reworked"),
            ),
            Column("decision_id", Kind.ID),
        ),
        ("id",),
        foreign_keys=(
            _fk("card_id", "card"),
            _fk("face_id", "face"),
            _fk("digital_card_id", "digital_card"),
            _fk("digital_face_id", "digital_face"),
            _fk("decision_id", "decision"),
        ),
        query_checks=(
            QueryCheck(
                "digital_link_parents",
                "SELECT 1 FROM digital_link AS l LEFT JOIN face AS f ON f.id=l.face_id LEFT JOIN digital_face AS d ON d.id=l.digital_face_id WHERE f.card_id != l.card_id OR d.digital_card_id != l.digital_card_id LIMIT 1",
                ("digital_link", "face", "digital_face"),
            ),
            QueryCheck(
                "digital_link_adopted",
                "SELECT 1 FROM digital_link AS l JOIN decision AS d ON d.id=l.decision_id WHERE d.state NOT IN ('sampled','confirmed') LIMIT 1",
                ("digital_link", "decision"),
            ),
            QueryCheck(
                "digital_link_unique_target",
                "SELECT 1 FROM digital_link GROUP BY card_id,face_id,digital_card_id,digital_face_id HAVING count(*) > 1 LIMIT 1",
                ("digital_link",),
            ),
        ),
    ),
    Table(
        "digital_link_coverage",
        (
            Column("card_id", Kind.ID),
            Column("game", Kind.TEXT, choices=("sv1", "svwb")),
            Column(
                "state",
                Kind.TEXT,
                choices=("unreviewed", "partial", "reviewed_none", "reviewed_matches"),
            ),
            Column("as_of", Kind.TEXT, pattern=DATE),
            Column("decision_id", Kind.ID),
        ),
        ("card_id", "game"),
        foreign_keys=(_fk("card_id", "card"), _fk("decision_id", "decision")),
        query_checks=(
            QueryCheck(
                "digital_coverage_adopted",
                "SELECT 1 FROM digital_link_coverage AS c JOIN decision AS d ON d.id=c.decision_id WHERE d.state NOT IN ('sampled','confirmed') LIMIT 1",
                ("digital_link_coverage", "decision"),
            ),
        ),
    ),
    Table(
        "glossary_term",
        (
            Column("id", Kind.ID),
            Column(
                "category",
                Kind.TEXT,
                choices=("keyword", "ability", "trait", "rule_term", "card_name"),
            ),
            Column("source_ja", Kind.TEXT),
            Column("concept_key", Kind.TEXT, pattern=r"[a-z][a-z0-9_.-]*"),
            Column("emphasis", Kind.BOOL, nullable=True),
            *QUALITY,
        ),
        ("id",),
        foreign_keys=(SOURCE,),
        unique=(Unique(("concept_key",)),),
        checks=(Check("id = 'term:' || concept_key"),),
    ),
    Table(
        "glossary_translation",
        (
            Column("term_id", Kind.ID),
            Column("lang", Kind.ID, pattern=LANG),
            Column("text", Kind.TEXT),
            Column("source_id", Kind.ID, nullable=True),
            *QUALITY,
        ),
        ("term_id", "lang"),
        foreign_keys=(
            ForeignKey(("term_id",), "glossary_term", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
            SOURCE,
        ),
    ),
    Table(
        "translation_context",
        (
            Column("id", Kind.ID),
            Column("source_unit_id", Kind.ID),
            Column("semantic_variant", Kind.ID),
        ),
        ("id",),
        foreign_keys=(ForeignKey(("source_unit_id",), "text_unit", ("id",)),),
        unique=(Unique(("source_unit_id", "semantic_variant")),),
    ),
    Table(
        "translation",
        (
            Column("id", Kind.ID),
            Column("context_id", Kind.ID),
            Column("target_lang", Kind.ID, pattern=LANG),
            Column("revision", Kind.UINT),
            Column("text", Kind.TEXT),
            Column("tokens", Kind.JSON, nullable=True, json_schema="TranslationTokens"),
            Column("origin", Kind.TEXT, choices=("official", "project", "machine")),
            Column(
                "authority",
                Kind.TEXT,
                choices=("sve_official", "digital_official", "unofficial"),
            ),
            Column("low_confidence", Kind.BOOL),
            Column("source_hash", Kind.TEXT, pattern=HASH),
            Column("source_id", Kind.ID, nullable=True),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("context_id",), "translation_context", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
        ),
        unique=(Unique(("context_id", "target_lang", "revision")),),
    ),
)
