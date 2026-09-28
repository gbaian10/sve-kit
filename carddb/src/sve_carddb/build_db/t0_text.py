"""T0 declarations from docs/schema/build-db.md; domain validation is separate."""

from sve_carddb.build_db.domains import DATE, HASH, INSTANT, LANG
from sve_carddb.build_db.model import Check, Column, ForeignKey, Kind, Table, Unique

TABLES = (
    Table(
        "text_unit",
        (
            Column("id", Kind.ID),
            Column("lang", Kind.ID, pattern=LANG),
            Column("text", Kind.TEXT),
            Column("content_hash", Kind.TEXT, pattern=HASH),
        ),
        ("id",),
        foreign_keys=(ForeignKey(("lang",), "language", ("code",)),),
        unique=(Unique(("lang", "content_hash")),),
        without_rowid=False,
    ),
    Table(
        "face_revision",
        (
            Column("id", Kind.ID),
            Column("face_id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("revision", Kind.UINT),
            Column("effective_from", Kind.TEXT, nullable=True, pattern=DATE),
            Column("effective_until", Kind.TEXT, nullable=True, pattern=DATE),
            Column("temporal_status", Kind.TEXT, choices=("known", "unknown")),
            Column("observed_at", Kind.TEXT, pattern=INSTANT),
            Column(
                "change_kind",
                Kind.TEXT,
                choices=("initial", "errata", "wording", "source_correction"),
            ),
            Column("name_unit_id", Kind.ID),
            Column("effect_unit_id", Kind.ID),
            Column("class_code", Kind.ID, nullable=True),
            Column("type_code", Kind.ID),
            Column("cost", Kind.INT, nullable=True),
            Column("attack", Kind.INT, nullable=True),
            Column("defense", Kind.INT, nullable=True),
            Column("source_id", Kind.ID),
            Column("decision_id", Kind.ID, nullable=True),
            Column("supersedes_id", Kind.ID, nullable=True),
            Column("class_kind", Kind.TEXT, fixed="class"),
            Column("type_kind", Kind.TEXT, fixed="type"),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("class_kind", "class_code"), "vocabulary", ("kind", "code")),
            ForeignKey(("type_kind", "type_code"), "vocabulary", ("kind", "code")),
            ForeignKey(
                ("supersedes_id", "face_id", "region"),
                "face_revision",
                ("id", "face_id", "region"),
            ),
            ForeignKey(("face_id",), "face", ("id",)),
            ForeignKey(("name_unit_id",), "text_unit", ("id",)),
            ForeignKey(("effect_unit_id",), "text_unit", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
            ForeignKey(("supersedes_id",), "face_revision", ("id",)),
        ),
        unique=(
            Unique(("id", "face_id")),
            Unique(("face_id", "region", "revision")),
            Unique(("id", "face_id", "region")),
        ),
        checks=(
            Check("cost IS NULL OR cost >= 0"),
            Check("attack IS NULL OR attack >= 0"),
            Check("defense IS NULL OR defense >= 0"),
            Check(
                "effective_until IS NULL OR effective_from IS NULL OR effective_from < effective_until"
            ),
        ),
    ),
    Table(
        "face_text_section",
        (
            Column("revision_id", Kind.ID),
            Column("ordinal", Kind.UINT),
            Column("text_unit_id", Kind.ID),
            Column(
                "kind",
                Kind.TEXT,
                choices=("rule", "reminder", "token_definition", "unknown"),
            ),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("revision_id", "ordinal"),
        foreign_keys=(
            ForeignKey(("revision_id",), "face_revision", ("id",)),
            ForeignKey(("text_unit_id",), "text_unit", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
    ),
    Table(
        "printing_text_section",
        (
            Column("printing_id", Kind.ID),
            Column("face_id", Kind.ID),
            Column("ordinal", Kind.UINT),
            Column("text_unit_id", Kind.ID),
            Column(
                "kind",
                Kind.TEXT,
                choices=("rule", "reminder", "token_definition", "unknown"),
            ),
            Column("source_id", Kind.ID),
        ),
        ("printing_id", "face_id", "ordinal"),
        foreign_keys=(
            ForeignKey(
                ("printing_id", "face_id"), "printing_face", ("printing_id", "face_id")
            ),
            ForeignKey(("text_unit_id",), "text_unit", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
        ),
    ),
    Table(
        "face_trait",
        (
            Column("revision_id", Kind.ID),
            Column("trait_code", Kind.ID),
            Column("trait_kind", Kind.TEXT, fixed="trait"),
        ),
        ("revision_id", "trait_code"),
        foreign_keys=(
            ForeignKey(("trait_kind", "trait_code"), "vocabulary", ("kind", "code")),
            ForeignKey(("revision_id",), "face_revision", ("id",)),
        ),
    ),
    Table(
        "face_title",
        (
            Column("revision_id", Kind.ID),
            Column("title_code", Kind.ID),
            Column("title_kind", Kind.TEXT, fixed="title"),
        ),
        ("revision_id", "title_code"),
        foreign_keys=(
            ForeignKey(("title_kind", "title_code"), "vocabulary", ("kind", "code")),
            ForeignKey(("revision_id",), "face_revision", ("id",)),
        ),
    ),
    Table(
        "face_special_kind",
        (
            Column("revision_id", Kind.ID),
            Column("special_kind_code", Kind.ID),
            Column("special_kind_kind", Kind.TEXT, fixed="special_kind"),
        ),
        ("revision_id", "special_kind_code"),
        foreign_keys=(
            ForeignKey(
                ("special_kind_kind", "special_kind_code"),
                "vocabulary",
                ("kind", "code"),
            ),
            ForeignKey(("revision_id",), "face_revision", ("id",)),
        ),
    ),
    Table(
        "face_current",
        (
            Column("face_id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("revision_id", Kind.ID),
            Column(
                "basis",
                Kind.TEXT,
                choices=(
                    "dated_effective",
                    "latest_observed_no_errata",
                    "latest_adopted_wording",
                    "reviewed_override",
                ),
            ),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("face_id", "region"),
        foreign_keys=(
            ForeignKey(
                ("revision_id", "face_id", "region"),
                "face_revision",
                ("id", "face_id", "region"),
            ),
            ForeignKey(("face_id",), "face", ("id",)),
            ForeignKey(("revision_id",), "face_revision", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        checks=(Check("basis != 'reviewed_override' OR decision_id IS NOT NULL"),),
    ),
    Table(
        "printing_face_observation",
        (
            Column("printing_id", Kind.ID),
            Column("face_id", Kind.ID),
            Column("source_id", Kind.ID),
            Column("revision_id", Kind.ID),
            Column("observed_at", Kind.TEXT, pattern=INSTANT),
        ),
        ("printing_id", "face_id", "source_id"),
        foreign_keys=(
            ForeignKey(("revision_id", "face_id"), "face_revision", ("id", "face_id")),
            ForeignKey(
                ("printing_id", "face_id"), "printing_face", ("printing_id", "face_id")
            ),
            ForeignKey(("source_id",), "source_record", ("id",)),
            ForeignKey(("revision_id",), "face_revision", ("id",)),
        ),
    ),
    Table(
        "source_coverage",
        (
            Column("kind", Kind.TEXT, choices=("errata", "qa", "cardlist", "cr")),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("scope_key", Kind.TEXT),
            Column("from_date", Kind.TEXT, pattern=DATE),
            Column("until_date", Kind.TEXT, nullable=True, pattern=DATE),
            Column("as_of", Kind.TEXT, pattern=DATE),
            Column("state", Kind.TEXT, choices=("complete", "partial")),
            Column("source_id", Kind.ID),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("kind", "region", "scope_key", "from_date", "as_of"),
        foreign_keys=(
            ForeignKey(("source_id",), "source_record", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        checks=(
            Check("until_date IS NULL OR from_date < until_date"),
            Check("from_date <= as_of"),
        ),
    ),
)
