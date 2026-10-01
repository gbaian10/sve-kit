"""Versioned comprehensive rules; same version labels may identify different sources."""

from sve_carddb.build_db.domains import DATE
from sve_carddb.build_db.model import (
    Column,
    ForeignKey,
    Kind,
    QueryCheck,
    Table,
    Unique,
)

TABLES = (
    Table(
        "cr_version",
        (
            Column("id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("version", Kind.TEXT),
            Column("published_on", Kind.TEXT, nullable=True, pattern=DATE),
            Column("effective_on", Kind.TEXT, nullable=True, pattern=DATE),
            Column("source_id", Kind.ID),
            Column("source_url", Kind.TEXT),
        ),
        ("id",),
        foreign_keys=(ForeignKey(("source_id",), "source_record", ("id",)),),
        unique=(Unique(("region", "version", "source_id")),),
        query_checks=(
            QueryCheck(
                "profile_cr_region",
                "SELECT 1 FROM rules_profile_revision AS r JOIN rules_profile AS p ON p.id = r.profile_id "
                "JOIN cr_version AS c ON c.id = r.cr_version_id WHERE p.region != c.region LIMIT 1",
                ("rules_profile_revision", "rules_profile", "cr_version"),
            ),
        ),
    ),
    Table(
        "cr_clause",
        (
            Column("id", Kind.ID),
            Column("cr_version_id", Kind.ID),
            Column("number", Kind.TEXT),
            Column("text_unit_id", Kind.ID),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("cr_version_id",), "cr_version", ("id",)),
            ForeignKey(("text_unit_id",), "text_unit", ("id",)),
        ),
        unique=(Unique(("cr_version_id", "number")),),
    ),
)
