"""Optional cross-region review and art ownership declarations."""

from sve_carddb.build.model import (
    Check,
    Column,
    ForeignKey,
    Kind,
    QueryCheck,
    Table,
    Unique,
)
from sve_carddb.build.scalars import HASH
from sve_carddb.core.dates import DATE, INSTANT

TABLES = (
    Table(
        "art",
        (
            Column("id", Kind.ID),
            Column("card_id", Kind.ID),
            Column("face_id", Kind.ID),
            Column(
                "classification",
                Kind.TEXT,
                choices=("base", "alternate", "unclassified"),
            ),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("card_id",), "card", ("id",)),
            ForeignKey(("face_id",), "face", ("id",)),
            ForeignKey(("face_id", "card_id"), "face", ("id", "card_id")),
        ),
        unique=(Unique(("id", "face_id")),),
    ),
    Table(
        "region_mapping_review",
        (
            Column("card_id", Kind.ID),
            Column("target_region", Kind.TEXT, choices=("jp", "en")),
            Column("state", Kind.TEXT, choices=("pending", "confirmed_none")),
            Column("as_of", Kind.TEXT, pattern=DATE),
            Column("coverage_scope", Kind.TEXT),
            Column("source_id", Kind.ID),
        ),
        ("card_id", "target_region", "as_of"),
        foreign_keys=(
            ForeignKey(("card_id",), "card", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
        ),
    ),
    Table(
        "region_text_review",
        (
            Column("card_id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("source_jp_hash", Kind.TEXT, pattern=HASH),
            Column("source_region_hash", Kind.TEXT, pattern=HASH),
            Column("state", Kind.TEXT, choices=("pending", "aligned", "divergent")),
            Column("decision_id", Kind.ID, nullable=True),
            Column("checked_at", Kind.TEXT, pattern=INSTANT),
        ),
        ("card_id", "region", "source_jp_hash", "source_region_hash"),
        foreign_keys=(
            ForeignKey(("card_id",), "card", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        checks=(Check("state != 'aligned' OR decision_id IS NOT NULL"),),
        query_checks=(
            QueryCheck(
                "text_review_adopted",
                "SELECT 1 FROM region_text_review AS r JOIN decision AS d ON d.id = r.decision_id "
                "WHERE r.state = 'aligned' AND d.state NOT IN ('sampled', 'confirmed') LIMIT 1",
                ("region_text_review", "decision"),
            ),
        ),
    ),
    Table(
        "region_divergence",
        (
            Column("card_id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("field_scope", Kind.TEXT, choices=("rules", "name", "all")),
            Column("reason", Kind.TEXT),
            Column("effect", Kind.TEXT, choices=("manual", "override_dsl")),
            Column("override_dsl_id", Kind.ID, nullable=True),
            Column("source_id", Kind.ID),
            Column("decision_id", Kind.ID),
            Column("resolved", Kind.BOOL),
        ),
        ("card_id", "region", "field_scope"),
        foreign_keys=(
            ForeignKey(("card_id",), "card", ("id",)),
            ForeignKey(("override_dsl_id",), "dsl_document", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        checks=(Check("effect != 'override_dsl' OR override_dsl_id IS NOT NULL"),),
        query_checks=(
            QueryCheck(
                "divergence_confirmed",
                "SELECT 1 FROM region_divergence AS r JOIN decision AS d ON d.id = r.decision_id WHERE d.state != 'confirmed' LIMIT 1",
                ("region_divergence", "decision"),
            ),
        ),
    ),
)
