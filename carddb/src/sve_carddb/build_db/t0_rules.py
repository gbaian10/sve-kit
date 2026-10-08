"""T0 declarations from docs/schema/build-db.md; domain validation is separate."""

from sve_carddb.build_db.domains import HASH
from sve_carddb.build_db.model import (
    Check,
    Column,
    ForeignKey,
    Kind,
    QueryCheck,
    Table,
    Unique,
)
from sve_carddb.core.dates import DATE

TABLES = (
    Table(
        "rules_profile",
        (
            Column("id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("format_code", Kind.TEXT),
            Column("name_unit_id", Kind.ID),
        ),
        ("id",),
        foreign_keys=(ForeignKey(("name_unit_id",), "text_unit", ("id",)),),
        unique=(Unique(("region", "format_code")),),
    ),
    Table(
        "rules_profile_revision",
        (
            Column("id", Kind.ID),
            Column("profile_id", Kind.ID),
            Column("effective_from", Kind.TEXT, pattern=DATE),
            Column("effective_until", Kind.TEXT, nullable=True, pattern=DATE),
            Column("cr_version_id", Kind.ID, nullable=True),
            Column("source_id", Kind.ID),
            Column("default_copy_limit", Kind.UINT, nullable=True),
            Column("construction_rules_ref", Kind.TEXT, nullable=True),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("profile_id",), "rules_profile", ("id",)),
            ForeignKey(("cr_version_id",), "cr_version", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
        ),
        checks=(Check("effective_until IS NULL OR effective_from < effective_until"),),
        query_checks=(
            QueryCheck(
                "profile_revision_overlap",
                "SELECT 1 FROM rules_profile_revision AS a JOIN rules_profile_revision AS b "
                "ON a.profile_id = b.profile_id AND a.id < b.id "
                "WHERE (a.effective_until IS NULL OR b.effective_from < a.effective_until) "
                "AND (b.effective_until IS NULL OR a.effective_from < b.effective_until) LIMIT 1",
                ("rules_profile_revision",),
            ),
        ),
    ),
    Table(
        "restriction",
        (
            Column("id", Kind.ID),
            Column("profile_id", Kind.ID),
            Column("announced_on", Kind.TEXT, nullable=True, pattern=DATE),
            Column("effective_from", Kind.TEXT, pattern=DATE),
            Column("effective_until", Kind.TEXT, nullable=True, pattern=DATE),
            Column("kind", Kind.TEXT, choices=("copy_limit", "choice_group")),
            Column("state", Kind.TEXT, choices=("confirmed", "announced", "withdrawn")),
            Column("max_copies", Kind.UINT, nullable=True),
            Column("max_selected_groups", Kind.UINT, nullable=True),
            Column("source_id", Kind.ID),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("profile_id",), "rules_profile", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        checks=(
            Check("effective_until IS NULL OR effective_from < effective_until"),
            Check(
                "(kind = 'copy_limit' AND max_copies IS NOT NULL AND max_selected_groups IS NULL) OR (kind = 'choice_group' AND max_copies IS NULL AND max_selected_groups IS NOT NULL)"
            ),
        ),
        query_checks=(
            QueryCheck(
                "restriction_confirmed_decision",
                "SELECT 1 FROM restriction AS r JOIN decision AS d ON d.id = r.decision_id "
                "WHERE r.state = 'confirmed' AND d.state != 'confirmed' LIMIT 1",
                ("restriction", "decision"),
            ),
        ),
    ),
    Table(
        "restriction_member",
        (
            Column("restriction_id", Kind.ID),
            Column("rules_name_id", Kind.ID),
            Column("choice_option", Kind.UINT),
            Column("deck_scope", Kind.TEXT, choices=("main", "evolve", "all")),
        ),
        ("restriction_id", "rules_name_id", "deck_scope"),
        foreign_keys=(
            ForeignKey(("restriction_id",), "restriction", ("id",)),
            ForeignKey(("rules_name_id",), "rules_name", ("id",)),
        ),
        query_checks=(
            QueryCheck(
                "restriction_member_region",
                "SELECT 1 FROM restriction_member AS m JOIN restriction AS r ON r.id = m.restriction_id "
                "JOIN rules_profile AS p ON p.id = r.profile_id JOIN rules_name AS n ON n.id = m.rules_name_id "
                "WHERE p.region != n.region LIMIT 1",
                ("restriction_member", "restriction", "rules_profile", "rules_name"),
            ),
        ),
    ),
    Table(
        "restriction_coverage",
        (
            Column("profile_id", Kind.ID),
            Column("from_date", Kind.TEXT, pattern=DATE),
            Column("until_date", Kind.TEXT, nullable=True, pattern=DATE),
            Column("state", Kind.TEXT, choices=("complete", "partial")),
            Column("source_id", Kind.ID),
        ),
        ("profile_id", "from_date"),
        foreign_keys=(
            ForeignKey(("profile_id",), "rules_profile", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
        ),
        checks=(Check("until_date IS NULL OR from_date < until_date"),),
    ),
    Table(
        "deck_role_override",
        (
            Column("card_id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("role", Kind.TEXT, choices=("main", "evolve", "leader", "extra")),
            Column("decision_id", Kind.ID),
        ),
        ("card_id", "region"),
        foreign_keys=(
            ForeignKey(("card_id",), "card", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        query_checks=(
            QueryCheck(
                "deck_role_regional_card",
                "SELECT 1 FROM deck_role_override AS o WHERE NOT EXISTS "
                "(SELECT 1 FROM printing AS p WHERE p.card_id = o.card_id AND p.region = o.region) LIMIT 1",
                ("deck_role_override", "printing"),
            ),
        ),
    ),
    Table(
        "card_engine_support",
        (
            Column("card_id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column(
                "status",
                Kind.TEXT,
                choices=(
                    "missing_dsl",
                    "draft",
                    "reviewed",
                    "engine_passed",
                    "load_rejected",
                ),
            ),
            Column("dsl_id", Kind.ID, nullable=True),
            Column("candidate_hash", Kind.TEXT, nullable=True, pattern=HASH),
            Column("dsl_version", Kind.TEXT, nullable=True),
            Column("engine_version", Kind.TEXT, nullable=True),
            Column("engine_build_hash", Kind.TEXT, nullable=True, pattern=HASH),
            Column("validation_policy_id", Kind.TEXT, nullable=True),
            Column("load_id", Kind.ID, nullable=True),
            Column(
                "validation_state",
                Kind.TEXT,
                choices=("fresh", "stale", "not_applicable"),
            ),
            Column(
                "reason_codes",
                Kind.JSON,
                json_schema="card_engine_support_reason_codes",
            ),
            Column("automatic", Kind.BOOL),
        ),
        ("card_id", "region"),
        foreign_keys=(
            ForeignKey(("card_id",), "card", ("id",)),
            ForeignKey(("dsl_id",), "dsl_document", ("id",)),
            ForeignKey(("load_id",), "dsl_load", ("id",)),
        ),
        checks=(
            Check("status = 'missing_dsl'"),
            Check(
                "candidate_hash IS NULL AND dsl_version IS NULL AND engine_version IS NULL AND engine_build_hash IS NULL AND validation_policy_id IS NULL"
            ),
            Check("automatic = 0"),
            Check("validation_state = 'not_applicable'"),
            Check("json_array_length(reason_codes) > 0"),
            Check("sve_sorted_unique(reason_codes) = 1"),
        ),
    ),
)
