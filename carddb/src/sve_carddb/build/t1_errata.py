"""Official errata versions, scoped changes and printing applicability."""

from sve_carddb.build.model import (
    Check,
    Column,
    ForeignKey,
    Kind,
    QueryCheck,
    Table,
    Unique,
)
from sve_carddb.build.scalars import CODE
from sve_carddb.core.dates import DATE

TABLES = (
    Table(
        "errata",
        (
            Column("id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("official_url", Kind.TEXT),
        ),
        ("id",),
        unique=(Unique(("official_url",)),),
    ),
    Table(
        "errata_version",
        (
            Column("id", Kind.ID),
            Column("errata_id", Kind.ID),
            Column("revision", Kind.UINT),
            Column("announced_on", Kind.TEXT, nullable=True, pattern=DATE),
            Column("effective_on", Kind.TEXT, nullable=True, pattern=DATE),
            Column("date_raw", Kind.TEXT, nullable=True),
            Column("reason_unit_id", Kind.ID, nullable=True),
            Column("exchange_offered", Kind.BOOL, nullable=True),
            Column("source_id", Kind.ID),
            Column("supersedes_id", Kind.ID, nullable=True),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("errata_id",), "errata", ("id",)),
            ForeignKey(("reason_unit_id",), "text_unit", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
            ForeignKey(("supersedes_id",), "errata_version", ("id",)),
        ),
        unique=(Unique(("errata_id", "revision")),),
        query_checks=(
            QueryCheck(
                "errata_supersedes_owner",
                "SELECT 1 FROM errata_version AS v JOIN errata_version AS p ON p.id = v.supersedes_id WHERE v.errata_id != p.errata_id LIMIT 1",
                ("errata_version",),
            ),
        ),
    ),
    Table(
        "errata_change",
        (
            Column("id", Kind.ID),
            Column("errata_version_id", Kind.ID),
            Column("face_id", Kind.ID),
            Column("before_revision_id", Kind.ID, nullable=True),
            Column("after_revision_id", Kind.ID, nullable=True),
            Column("before_value", Kind.JSON, json_schema="correction_value"),
            Column("after_value", Kind.JSON, json_schema="correction_value"),
            Column("field", Kind.ID, pattern=CODE),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("errata_version_id",), "errata_version", ("id",)),
            ForeignKey(("face_id",), "face", ("id",)),
            ForeignKey(("before_revision_id",), "face_revision", ("id",)),
            ForeignKey(("after_revision_id",), "face_revision", ("id",)),
        ),
        checks=(
            Check(
                "sve_json_valid('change_values', json_array(face_id, field, json(before_value), json(after_value))) = 1"
            ),
        ),
        query_checks=tuple(
            QueryCheck(
                f"errata_{side}_scope",
                f"SELECT 1 FROM errata_change AS c JOIN face_revision AS r ON r.id = c.{side}_revision_id "  # ruff: ignore[hardcoded-sql-expression] -- side is a closed declaration constant
                "JOIN errata_version AS v ON v.id = c.errata_version_id JOIN errata AS e ON e.id = v.errata_id "
                "WHERE r.face_id != c.face_id OR r.region != e.region LIMIT 1",
                ("errata_change", "face_revision", "errata_version", "errata"),
            )
            for side in ("before", "after")
        ),
    ),
    Table(
        "errata_printing",
        (
            Column("errata_version_id", Kind.ID),
            Column("printing_id", Kind.ID),
            Column("scope", Kind.TEXT, choices=("listed", "confirmed_applies")),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("errata_version_id", "printing_id"),
        foreign_keys=(
            ForeignKey(("errata_version_id",), "errata_version", ("id",)),
            ForeignKey(("printing_id",), "printing", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        checks=(Check("scope != 'confirmed_applies' OR decision_id IS NOT NULL"),),
        query_checks=(
            QueryCheck(
                "errata_printing_region",
                "SELECT 1 FROM errata_printing AS x JOIN printing AS p ON p.id = x.printing_id "
                "JOIN errata_version AS v ON v.id = x.errata_version_id JOIN errata AS e ON e.id = v.errata_id "
                "WHERE p.region != e.region LIMIT 1",
                ("errata_printing", "printing", "errata_version", "errata"),
            ),
            QueryCheck(
                "errata_printing_confirmed",
                "SELECT 1 FROM errata_printing AS x JOIN decision AS d ON d.id = x.decision_id "
                "WHERE x.scope = 'confirmed_applies' AND d.state != 'confirmed' LIMIT 1",
                ("errata_printing", "decision"),
            ),
        ),
    ),
)
