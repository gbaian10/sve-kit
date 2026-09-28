"""Source correction records and their checked applications."""

from sve_carddb.build_db.domains import DATE, HASH
from sve_carddb.build_db.model import Check, Column, ForeignKey, Kind, QueryCheck, Table

TABLES = (
    Table(
        "source_correction",
        (
            Column("id", Kind.ID),
            Column("printing_id", Kind.ID),
            Column("face_id", Kind.ID),
            Column("field", Kind.TEXT),
            Column("expected_source_unit_id", Kind.ID, nullable=True),
            Column("expected_raw_value", Kind.JSON, json_schema="correction_value"),
            Column("corrected_value", Kind.JSON, json_schema="correction_value"),
            Column("expected_source_hash", Kind.TEXT, pattern=HASH),
            Column("reason", Kind.TEXT),
            Column("decision_id", Kind.ID),
            Column("reported_to_official", Kind.BOOL),
            Column("reported_on", Kind.TEXT, nullable=True, pattern=DATE),
            Column("report_url", Kind.TEXT, nullable=True),
            Column(
                "state",
                Kind.TEXT,
                choices=("active", "upstream_fixed", "needs_review", "retired"),
            ),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("printing_id",), "printing", ("id",)),
            ForeignKey(("face_id",), "face", ("id",)),
            ForeignKey(
                ("printing_id", "face_id"), "printing_face", ("printing_id", "face_id")
            ),
            ForeignKey(("expected_source_unit_id",), "text_unit", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        checks=(
            Check(
                "sve_json_valid('change_values', json_array(face_id, field, json(expected_raw_value), json(corrected_value))) = 1"
            ),
        ),
        query_checks=(
            QueryCheck(
                "correction_adoption",
                "SELECT 1 FROM source_correction AS c JOIN decision AS d ON d.id = c.decision_id "
                "WHERE c.state IN ('active', 'upstream_fixed') AND d.state != 'confirmed' LIMIT 1",
                ("source_correction", "decision"),
            ),
        ),
    ),
    Table(
        "correction_evidence",
        (
            Column("correction_id", Kind.ID),
            Column("source_id", Kind.ID),
            Column(
                "kind",
                Kind.TEXT,
                choices=("card_image", "other_printing", "official_page"),
            ),
            Column("locator", Kind.TEXT),
            Column("quote", Kind.TEXT, nullable=True),
        ),
        ("correction_id", "source_id", "kind"),
        foreign_keys=(
            ForeignKey(("correction_id",), "source_correction", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
        ),
    ),
    Table(
        "correction_application",
        (
            Column("correction_id", Kind.ID),
            Column("source_id", Kind.ID),
            Column("result_unit_id", Kind.ID, nullable=True),
            Column("face_revision_id", Kind.ID, nullable=True),
            Column(
                "status", Kind.TEXT, choices=("applied", "already_fixed", "conflict")
            ),
        ),
        ("correction_id", "source_id"),
        foreign_keys=(
            ForeignKey(("correction_id",), "source_correction", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
            ForeignKey(("result_unit_id",), "text_unit", ("id",)),
            ForeignKey(("face_revision_id",), "face_revision", ("id",)),
        ),
        query_checks=(
            QueryCheck(
                "application_text_result",
                "SELECT 1 FROM correction_application AS a JOIN source_correction AS c ON c.id = a.correction_id "
                "WHERE a.status != 'conflict' AND c.field IN ('effect','name','flavor','other') AND a.result_unit_id IS NULL LIMIT 1",
                ("correction_application", "source_correction"),
            ),
            QueryCheck(
                "application_rule_result",
                "SELECT 1 FROM correction_application AS a JOIN source_correction AS c ON c.id = a.correction_id "
                "WHERE a.status != 'conflict' AND c.field IN ('card_type','cost','attack','defense','traits','titles','special_kinds') AND a.face_revision_id IS NULL LIMIT 1",
                ("correction_application", "source_correction"),
            ),
            QueryCheck(
                "application_revision_scope",
                "SELECT 1 FROM correction_application AS a JOIN source_correction AS c ON c.id = a.correction_id "
                "JOIN printing AS p ON p.id = c.printing_id JOIN face_revision AS r ON r.id = a.face_revision_id "
                "WHERE r.face_id != c.face_id OR r.region != p.region LIMIT 1",
                (
                    "correction_application",
                    "source_correction",
                    "printing",
                    "face_revision",
                ),
            ),
            QueryCheck(
                "application_adoption",
                "SELECT 1 FROM correction_application AS a JOIN source_correction AS c ON c.id = a.correction_id "
                "JOIN decision AS d ON d.id = c.decision_id WHERE a.status != 'conflict' AND d.state != 'confirmed' LIMIT 1",
                ("correction_application", "source_correction", "decision"),
            ),
        ),
    ),
)
