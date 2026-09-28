"""Card relationships retain printing evidence without conflating reskins with identity."""

from sve_carddb.build_db.model import (
    Check,
    Column,
    ForeignKey,
    Kind,
    QueryCheck,
    Table,
    Unique,
)

TABLES = (
    Table(
        "card_related",
        (
            Column("id", Kind.ID),
            Column("from_card_id", Kind.ID),
            Column("to_card_id", Kind.ID),
            Column("target_printing_id", Kind.ID, nullable=True),
            Column(
                "relation",
                Kind.TEXT,
                choices=(
                    "official_unspecified",
                    "evolves_to",
                    "advances_to",
                    "produces_token",
                    "mentions",
                    "extra_aid",
                    "same_rules_reskin",
                ),
            ),
            Column("suggested_count", Kind.UINT, nullable=True),
            Column("source_kind", Kind.TEXT, choices=("official", "authored", "dsl")),
            Column("source_id", Kind.ID, nullable=True),
            Column("dsl_id", Kind.ID, nullable=True),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("from_card_id",), "card", ("id",)),
            ForeignKey(("to_card_id",), "card", ("id",)),
            ForeignKey(("target_printing_id",), "printing", ("id",)),
            ForeignKey(
                ("target_printing_id", "to_card_id"), "printing", ("id", "card_id")
            ),
            ForeignKey(("source_id",), "source_record", ("id",)),
            ForeignKey(("dsl_id",), "dsl_document", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        unique=(Unique(("from_card_id",), where="relation = 'same_rules_reskin'"),),
        checks=(
            Check("from_card_id != to_card_id"),
            Check("suggested_count IS NULL OR suggested_count > 0"),
            Check("relation != 'same_rules_reskin' OR source_kind = 'authored'"),
            Check("relation != 'same_rules_reskin' OR decision_id IS NOT NULL"),
            Check("relation != 'same_rules_reskin' OR suggested_count IS NULL"),
            Check("relation != 'same_rules_reskin' OR dsl_id IS NULL"),
        ),
        query_checks=(
            QueryCheck(
                "reskin_confirmed",
                "SELECT 1 FROM card_related AS r JOIN decision AS d ON d.id = r.decision_id "
                "WHERE r.relation = 'same_rules_reskin' AND d.state != 'confirmed' LIMIT 1",
                ("card_related", "decision"),
            ),
            QueryCheck(
                "reskin_reverse",
                "SELECT 1 FROM card_related AS a JOIN card_related AS b ON a.from_card_id = b.to_card_id AND a.to_card_id = b.from_card_id "
                "WHERE a.relation = 'same_rules_reskin' AND b.relation = 'same_rules_reskin' LIMIT 1",
                ("card_related",),
            ),
        ),
    ),
)
