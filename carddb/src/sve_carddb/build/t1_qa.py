"""Q&A identity and immutable source versions, including unnumbered entries."""

from sve_carddb.build.model import Column, ForeignKey, Kind, QueryCheck, Table, Unique
from sve_carddb.core.dates import DATE, INSTANT

TABLES = (
    Table(
        "qa",
        (
            Column("id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("official_number", Kind.TEXT, nullable=True),
            Column("stable_source_key", Kind.TEXT),
            Column("source_url", Kind.TEXT),
        ),
        ("id",),
        unique=(
            Unique(("region", "stable_source_key")),
            Unique(("region", "official_number")),
        ),
    ),
    Table(
        "qa_version",
        (
            Column("id", Kind.ID),
            Column("qa_id", Kind.ID),
            Column("revision", Kind.UINT),
            Column("published_on", Kind.TEXT, nullable=True, pattern=DATE),
            Column("updated_on", Kind.TEXT, nullable=True, pattern=DATE),
            Column("date_raw", Kind.TEXT, nullable=True),
            Column("observed_at", Kind.TEXT, pattern=INSTANT),
            Column("question_unit_id", Kind.ID),
            Column("answer_unit_id", Kind.ID),
            Column("state", Kind.TEXT, choices=("active", "withdrawn")),
            Column("source_id", Kind.ID),
            Column("supersedes_id", Kind.ID, nullable=True),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("qa_id",), "qa", ("id",)),
            ForeignKey(("question_unit_id",), "text_unit", ("id",)),
            ForeignKey(("answer_unit_id",), "text_unit", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
            ForeignKey(("supersedes_id",), "qa_version", ("id",)),
        ),
        unique=(Unique(("qa_id", "revision")),),
        query_checks=(
            QueryCheck(
                "qa_supersedes_owner",
                "SELECT 1 FROM qa_version AS v JOIN qa_version AS p ON p.id = v.supersedes_id WHERE v.qa_id != p.qa_id LIMIT 1",
                ("qa_version",),
            ),
        ),
    ),
    Table(
        "qa_card",
        (Column("qa_version_id", Kind.ID), Column("card_id", Kind.ID)),
        ("qa_version_id", "card_id"),
        foreign_keys=(
            ForeignKey(("qa_version_id",), "qa_version", ("id",)),
            ForeignKey(("card_id",), "card", ("id",)),
        ),
    ),
)
