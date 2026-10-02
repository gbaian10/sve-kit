"""Opt-in immutable semantics for confirmed wording adoption, without DSL approval."""

from sve_carddb.build_db.domains import HASH
from sve_carddb.build_db.model import Column, ForeignKey, Kind, QueryCheck, Table

TABLES = (
    Table(
        "face_semantics",
        (
            Column("id", Kind.ID),
            Column("face_id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("rule_text_unit_id", Kind.ID),
            Column("rule_sections", Kind.JSON, json_schema="semantic_sections"),
            Column("normalizer_version", Kind.TEXT),
            Column("rule_hash", Kind.TEXT, pattern=HASH),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("face_id",), "face", ("id",)),
            ForeignKey(("rule_text_unit_id",), "text_unit", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        query_checks=(
            QueryCheck(
                "semantic_section_closure",
                "SELECT 1 FROM face_semantics AS s, json_each(s.rule_sections) AS j "
                "LEFT JOIN text_unit AS t ON t.id = j.value WHERE t.id IS NULL LIMIT 1",
                ("face_semantics", "text_unit"),
            ),
            QueryCheck(
                "semantic_language",
                "SELECT 1 FROM face_semantics AS s JOIN text_unit AS t ON t.id = s.rule_text_unit_id "
                "WHERE t.lang != CASE s.region WHEN 'jp' THEN 'ja' ELSE 'en' END LIMIT 1",
                ("face_semantics", "text_unit"),
            ),
            QueryCheck(
                "semantic_section_language",
                "SELECT 1 FROM face_semantics AS s, json_each(s.rule_sections) AS j "
                "JOIN text_unit AS t ON t.id = j.value WHERE t.lang != CASE s.region WHEN 'jp' THEN 'ja' ELSE 'en' END LIMIT 1",
                ("face_semantics", "text_unit"),
            ),
            QueryCheck(
                "semantic_confirmed",
                "SELECT 1 FROM face_semantics AS s JOIN decision AS d ON d.id = s.decision_id WHERE d.state != 'confirmed' LIMIT 1",
                ("face_semantics", "decision"),
            ),
        ),
    ),
    Table(
        "revision_semantics",
        (
            Column("revision_id", Kind.ID),
            Column("semantic_id", Kind.ID),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("revision_id",),
        foreign_keys=(
            ForeignKey(("revision_id",), "face_revision", ("id",)),
            ForeignKey(("semantic_id",), "face_semantics", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        query_checks=(
            QueryCheck(
                "semantic_same_face_region",
                "SELECT 1 FROM revision_semantics AS m JOIN face_revision AS r ON r.id = m.revision_id "
                "JOIN face_semantics AS s ON s.id = m.semantic_id WHERE r.face_id != s.face_id OR r.region != s.region LIMIT 1",
                ("revision_semantics", "face_revision", "face_semantics"),
            ),
            QueryCheck(
                "revision_semantic_confirmed",
                "SELECT 1 FROM revision_semantics AS m JOIN decision AS d ON d.id = m.decision_id WHERE d.state != 'confirmed' LIMIT 1",
                ("revision_semantics", "decision"),
            ),
        ),
    ),
    Table(
        "semantic_reference",
        (
            Column("semantic_id", Kind.ID),
            Column("target_face_id", Kind.ID),
            Column(
                "relation", Kind.TEXT, choices=("token_definition", "rule_reference")
            ),
        ),
        ("semantic_id", "target_face_id", "relation"),
        foreign_keys=(
            ForeignKey(("semantic_id",), "face_semantics", ("id",)),
            ForeignKey(("target_face_id",), "face", ("id",)),
        ),
    ),
)
