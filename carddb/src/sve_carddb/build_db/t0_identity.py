"""T0 declarations from docs/schema/build-db.md; domain validation is separate."""

from sve_carddb.build_db.domains import CODE, DATE, HASH, INSTANT, LANG
from sve_carddb.build_db.model import Check, Column, ForeignKey, Kind, Table, Unique

TABLES = (
    Table(
        "source_record",
        (
            Column("id", Kind.ID),
            Column(
                "kind",
                Kind.TEXT,
                choices=(
                    "official_page",
                    "official_api",
                    "official_pdf",
                    "image",
                    "third_party_page",
                    "third_party_audio",
                    "authored",
                ),
            ),
            Column("url", Kind.TEXT, nullable=True),
            Column("raw_locator", Kind.TEXT, nullable=True),
            Column("fetched_at", Kind.TEXT, nullable=True, pattern=INSTANT),
            Column("etag", Kind.TEXT, nullable=True),
            Column("last_modified", Kind.TEXT, nullable=True),
            Column("sha256", Kind.TEXT, pattern=HASH),
            Column("parser_version", Kind.TEXT, nullable=True),
            Column("authored_path", Kind.TEXT, nullable=True),
            Column("authored_revision", Kind.TEXT, nullable=True),
        ),
        ("id",),
        checks=(
            Check(
                "kind = 'authored' OR (url IS NOT NULL AND length(url) > 0 AND fetched_at IS NOT NULL)"
            ),
        ),
    ),
    Table(
        "decision",
        (
            Column("id", Kind.ID),
            Column(
                "state",
                Kind.TEXT,
                choices=(
                    "proposed",
                    "model_reviewed",
                    "sampled",
                    "confirmed",
                    "rejected",
                    "disputed",
                ),
            ),
            Column("scope", Kind.TEXT, choices=("record", "batch")),
            Column("category", Kind.ID),
            Column("membership_hash", Kind.TEXT, nullable=True, pattern=HASH),
            Column("policy_id", Kind.ID, nullable=True),
            Column(
                "sample_ids",
                Kind.JSON,
                nullable=True,
                json_schema="decision_sample_ids",
            ),
            Column("authored_by", Kind.TEXT),
            Column("authored_at", Kind.TEXT, pattern=INSTANT),
            Column("reviewed_by", Kind.TEXT, nullable=True),
            Column("reviewed_at", Kind.TEXT, nullable=True, pattern=INSTANT),
            Column(
                "confidence",
                Kind.TEXT,
                nullable=True,
                choices=("high", "medium", "low"),
            ),
            Column("note", Kind.TEXT),
        ),
        ("id",),
        checks=(
            Check(
                "state NOT IN ('sampled', 'confirmed') OR (reviewed_by IS NOT NULL AND length(trim(reviewed_by)) > 0 AND reviewed_at IS NOT NULL)"
            ),
            Check(
                "(scope = 'record' AND membership_hash IS NULL AND policy_id IS NULL AND sample_ids IS NULL) OR (scope = 'batch' AND membership_hash IS NOT NULL AND policy_id IS NOT NULL AND sample_ids IS NOT NULL)"
            ),
            Check(
                "scope != 'batch' OR state != 'sampled' OR (sample_ids IS NOT NULL AND json_array_length(sample_ids) > 0)"
            ),
        ),
    ),
    Table(
        "decision_source",
        (
            Column("decision_id", Kind.ID),
            Column("source_id", Kind.ID),
            Column("role", Kind.TEXT),
            Column("locator", Kind.TEXT, nullable=True),
            Column("quote", Kind.TEXT, nullable=True),
        ),
        ("decision_id", "source_id", "role"),
        foreign_keys=(
            ForeignKey(("decision_id",), "decision", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
        ),
    ),
    Table(
        "language",
        (
            Column("code", Kind.ID, pattern=LANG),
            Column("fallback_order", Kind.JSON, json_schema="language_fallback_order"),
            Column("display_name", Kind.TEXT),
        ),
        ("code",),
    ),
    Table(
        "vocabulary",
        (
            Column("kind", Kind.ID, pattern=CODE),
            Column("code", Kind.ID, pattern=CODE),
            Column("label_unit_id", Kind.ID),
            Column("active", Kind.BOOL),
        ),
        ("kind", "code"),
        foreign_keys=(ForeignKey(("label_unit_id",), "text_unit", ("id",)),),
    ),
    Table(
        "card",
        (
            Column("id", Kind.ID),
            Column("layout", Kind.TEXT, choices=("single", "double_faced")),
            Column(
                "identity_state",
                Kind.TEXT,
                choices=("confirmed", "provisional", "retired"),
            ),
            Column("home_set_id", Kind.ID),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("home_set_id",), "product_family", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
    ),
    Table(
        "face",
        (
            Column("id", Kind.ID),
            Column("card_id", Kind.ID),
            Column("ordinal", Kind.UINT),
            Column("side", Kind.TEXT, choices=("front", "back")),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("card_id",), "card", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        unique=(
            Unique(("card_id", "ordinal")),
            Unique(("card_id", "side")),
            Unique(("id", "card_id")),
        ),
    ),
    Table(
        "identity_change",
        (
            Column("id", Kind.ID),
            Column("kind", Kind.TEXT, choices=("merge", "split", "reassign_printing")),
            Column("old_card_id", Kind.ID),
            Column("new_card_id", Kind.ID),
            Column("printing_id", Kind.ID, nullable=True),
            Column("data_version", Kind.TEXT),
            Column("decision_id", Kind.ID),
            Column("reason", Kind.TEXT),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("old_card_id",), "card", ("id",)),
            ForeignKey(("new_card_id",), "card", ("id",)),
            ForeignKey(("printing_id",), "printing", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        checks=(
            Check("old_card_id != new_card_id"),
            Check("(kind = 'reassign_printing') = (printing_id IS NOT NULL)"),
        ),
    ),
    Table(
        "card_int_id",
        (
            Column("int_id", Kind.UINT32),
            Column("printing_id", Kind.ID),
        ),
        ("int_id",),
        foreign_keys=(ForeignKey(("printing_id",), "printing", ("id",)),),
        unique=(Unique(("printing_id",)),),
    ),
    Table(
        "rules_name",
        (
            Column("id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("official_name", Kind.TEXT),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("id",),
        foreign_keys=(ForeignKey(("decision_id",), "decision", ("id",)),),
        unique=(
            Unique(("id", "region")),
            Unique(("region", "official_name")),
        ),
    ),
    Table(
        "face_rules_name",
        (
            Column("face_id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("rules_name_id", Kind.ID),
            Column("role", Kind.TEXT, choices=("primary", "collab", "treated_as")),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("face_id", "region", "rules_name_id", "role"),
        foreign_keys=(
            ForeignKey(("rules_name_id", "region"), "rules_name", ("id", "region")),
            ForeignKey(("face_id",), "face", ("id",)),
            ForeignKey(("rules_name_id",), "rules_name", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
    ),
    Table(
        "product_family",
        (
            Column("id", Kind.ID),
            Column("code", Kind.ID),
            Column("public_code", Kind.ID),
            Column(
                "kind",
                Kind.TEXT,
                choices=(
                    "booster",
                    "promo",
                    "deck",
                    "collaboration",
                    "special_pack",
                    "special",
                    "other",
                ),
            ),
            Column("name_unit_id", Kind.ID),
            Column("decision_id", Kind.ID),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("name_unit_id",), "text_unit", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        unique=(
            Unique(("code",)),
            Unique(("public_code",)),
        ),
    ),
    Table(
        "product",
        (
            Column("id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("family_id", Kind.ID, nullable=True),
            Column("product_code", Kind.TEXT, nullable=True),
            Column("name_unit_id", Kind.ID),
            Column("product_type", Kind.ID, nullable=True),
            Column("released_on", Kind.TEXT, nullable=True, pattern=DATE),
            Column(
                "date_precision", Kind.TEXT, choices=("day", "month", "year", "unknown")
            ),
            Column("date_raw", Kind.TEXT, nullable=True),
            Column("source_id", Kind.ID),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("family_id",), "product_family", ("id",)),
            ForeignKey(("name_unit_id",), "text_unit", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
        ),
        checks=(
            Check("(date_precision = 'day') = (released_on IS NOT NULL)"),
            Check("date_precision NOT IN ('month', 'year') OR date_raw IS NOT NULL"),
        ),
    ),
    Table(
        "printing",
        (
            Column("id", Kind.ID),
            Column("card_id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("card_no", Kind.TEXT),
            Column("card_no_state", Kind.TEXT, choices=("official", "provisional")),
            Column("catalog_state", Kind.TEXT, choices=("official", "unlisted")),
            Column("decklog_available", Kind.BOOL),
            Column(
                "decklog_verification", Kind.TEXT, choices=("unverified", "verified")
            ),
            Column("decklog_source_id", Kind.ID, nullable=True),
            Column("decklog_checked_on", Kind.TEXT, nullable=True, pattern=DATE),
            Column(
                "listing_confidence",
                Kind.TEXT,
                nullable=True,
                choices=("high", "medium", "low"),
            ),
            Column("variant_key", Kind.TEXT),
            Column("home_set_id", Kind.ID),
            Column("rarity_code", Kind.ID, nullable=True),
            Column("rarity_raw", Kind.TEXT),
            Column("premium", Kind.BOOL, nullable=True),
            Column("serial_total", Kind.UINT, nullable=True),
            Column("source_id", Kind.ID),
            Column("decision_id", Kind.ID, nullable=True),
            Column("rarity_kind", Kind.TEXT, fixed="rarity"),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("rarity_kind", "rarity_code"), "vocabulary", ("kind", "code")),
            ForeignKey(("card_id",), "card", ("id",)),
            ForeignKey(("decklog_source_id",), "source_record", ("id",)),
            ForeignKey(("home_set_id",), "product_family", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        unique=(
            Unique(("id", "card_id", "region")),
            Unique(
                ("region", "card_no", "variant_key"), where="card_no_state = 'official'"
            ),
            Unique(("id", "card_id")),
        ),
        checks=(
            Check("serial_total IS NULL OR serial_total > 0"),
            Check(
                "(decklog_verification = 'verified' AND decklog_source_id IS NOT NULL AND decklog_checked_on IS NOT NULL) OR (decklog_verification = 'unverified' AND decklog_checked_on IS NULL AND decklog_available = (catalog_state = 'official'))"
            ),
        ),
    ),
    Table(
        "printing_product",
        (
            Column("printing_id", Kind.ID),
            Column("product_id", Kind.ID),
            Column("first_available_on", Kind.TEXT, nullable=True, pattern=DATE),
            Column(
                "first_available_precision",
                Kind.TEXT,
                nullable=True,
                choices=("day", "month", "year", "unknown"),
            ),
            Column("first_available_raw", Kind.TEXT, nullable=True),
            Column(
                "inclusion_kind",
                Kind.TEXT,
                choices=(
                    "pack",
                    "box",
                    "first_edition_campaign",
                    "qr_redemption",
                    "event_prize",
                    "other",
                ),
            ),
            Column("note_unit_id", Kind.ID, nullable=True),
            Column("source_id", Kind.ID),
        ),
        ("printing_id", "product_id"),
        foreign_keys=(
            ForeignKey(("printing_id",), "printing", ("id",)),
            ForeignKey(("product_id",), "product", ("id",)),
            ForeignKey(("note_unit_id",), "text_unit", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
        ),
        checks=(
            Check(
                "(first_available_precision IS 'day') = (first_available_on IS NOT NULL)"
            ),
            Check(
                "first_available_precision NOT IN ('month', 'year') OR first_available_raw IS NOT NULL"
            ),
            Check(
                "first_available_precision IS NOT NULL OR first_available_raw IS NULL"
            ),
        ),
    ),
    Table(
        "printing_face",
        (
            Column("printing_id", Kind.ID),
            Column("face_id", Kind.ID),
            Column("card_id", Kind.ID),
            Column("art_id", Kind.ID, nullable=True),
            Column("frame_code", Kind.ID, nullable=True),
            Column("signed", Kind.BOOL, nullable=True),
            Column(
                "embellishment_state",
                Kind.TEXT,
                choices=("unreviewed", "sampled", "confirmed"),
            ),
            Column("printed_name_unit_id", Kind.ID, nullable=True),
            Column("printed_effect_unit_id", Kind.ID, nullable=True),
            Column("flavor_unit_id", Kind.ID, nullable=True),
            Column(
                "printed_text_state",
                Kind.TEXT,
                choices=(
                    "verified",
                    "derived_no_errata",
                    "derived_from_errata",
                    "unknown",
                    "omitted",
                ),
            ),
            Column("credit_raw", Kind.TEXT, nullable=True),
            Column("source_id", Kind.ID),
            Column("frame_kind", Kind.TEXT, fixed="frame"),
        ),
        ("printing_id", "face_id"),
        foreign_keys=(
            ForeignKey(("frame_kind", "frame_code"), "vocabulary", ("kind", "code")),
            ForeignKey(("printing_id", "card_id"), "printing", ("id", "card_id")),
            ForeignKey(("face_id", "card_id"), "face", ("id", "card_id")),
            ForeignKey(("art_id", "face_id"), "art", ("id", "face_id")),
            ForeignKey(("printing_id",), "printing", ("id",)),
            ForeignKey(("face_id",), "face", ("id",)),
            ForeignKey(("card_id",), "card", ("id",)),
            ForeignKey(("art_id",), "art", ("id",)),
            ForeignKey(("printed_name_unit_id",), "text_unit", ("id",)),
            ForeignKey(("printed_effect_unit_id",), "text_unit", ("id",)),
            ForeignKey(("flavor_unit_id",), "text_unit", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
        ),
    ),
)
