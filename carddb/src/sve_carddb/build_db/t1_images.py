"""Image source metadata and public derivatives from build-db sections 11 and 17."""

from sve_carddb.build_db.domains import CODE, HASH
from sve_carddb.build_db.model import Check, Column, ForeignKey, Kind, QueryCheck, Table

TABLES = (
    Table(
        "image_asset",
        (
            Column("id", Kind.ID),
            Column("origin", Kind.TEXT, choices=("official", "third_party")),
            Column(
                "publication_state",
                Kind.TEXT,
                choices=("pending", "approved", "withdrawn"),
            ),
            Column("withdrawal_reason", Kind.TEXT, nullable=True),
            Column("review_decision_id", Kind.ID, nullable=True),
            Column("source_id", Kind.ID),
            Column("source_url", Kind.TEXT),
            Column("source_src_raw", Kind.TEXT),
            Column("content_hash", Kind.TEXT, nullable=True, pattern=HASH),
            Column("mime", Kind.TEXT, nullable=True),
            Column("width", Kind.UINT, nullable=True),
            Column("height", Kind.UINT, nullable=True),
            Column("bytes", Kind.UINT, nullable=True),
            Column(
                "availability", Kind.TEXT, choices=("available", "missing", "unfetched")
            ),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("review_decision_id",), "decision", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
        ),
        checks=(
            Check(
                "publication_state != 'withdrawn' OR (withdrawal_reason IS NOT NULL AND length(trim(withdrawal_reason)) > 0)"
            ),
            Check(
                "availability != 'available' OR (content_hash IS NOT NULL AND mime IS NOT NULL AND length(mime) > 0 AND width IS NOT NULL AND width > 0 AND height IS NOT NULL AND height > 0 AND bytes IS NOT NULL)"
            ),
            Check(
                "publication_state != 'approved' OR origin != 'official' OR availability = 'available'"
            ),
            Check(
                "publication_state != 'approved' OR origin != 'third_party' OR review_decision_id IS NOT NULL"
            ),
            Check(
                "publication_state != 'approved' OR origin != 'official' OR review_decision_id IS NULL"
            ),
        ),
        query_checks=(
            QueryCheck(
                "image_confirmed_review",
                "SELECT 1 FROM image_asset AS a JOIN decision AS d ON d.id = a.review_decision_id "
                "WHERE a.origin = 'third_party' AND a.publication_state = 'approved' "
                "AND (d.state != 'confirmed' OR d.reviewed_by IS NULL OR d.reviewed_at IS NULL) LIMIT 1",
                ("image_asset", "decision"),
            ),
            QueryCheck(
                "image_review_source",
                "SELECT 1 FROM image_asset AS a WHERE a.origin = 'third_party' "
                "AND a.publication_state = 'approved' AND NOT EXISTS "
                "(SELECT 1 FROM decision_source AS s WHERE s.decision_id = a.review_decision_id "
                "AND s.source_id = a.source_id) LIMIT 1",
                ("image_asset", "decision_source"),
            ),
        ),
    ),
    Table(
        "printing_image",
        (
            Column("printing_id", Kind.ID),
            Column("face_id", Kind.ID),
            Column("image_id", Kind.ID),
        ),
        ("printing_id", "face_id"),
        foreign_keys=(
            ForeignKey(
                ("printing_id", "face_id"), "printing_face", ("printing_id", "face_id")
            ),
            ForeignKey(("image_id",), "image_asset", ("id",)),
        ),
    ),
    Table(
        "image_variant",
        (
            Column("image_id", Kind.ID),
            Column("size_key", Kind.ID, pattern=CODE),
            Column("format", Kind.ID, pattern=CODE),
            Column("path", Kind.TEXT),
            Column("width", Kind.UINT),
            Column("height", Kind.UINT),
            Column("bytes", Kind.UINT),
            Column("sha256", Kind.TEXT, pattern=HASH),
            Column("recipe_version", Kind.TEXT),
        ),
        ("image_id", "size_key", "format"),
        foreign_keys=(
            ForeignKey(("image_id",), "image_asset", ("id",)),
            ForeignKey(("size_key",), "image_size", ("key",)),
        ),
        checks=(
            Check("width > 0"),
            Check("height > 0"),
            Check("format = 'webp'"),
            Check(
                "path = 'images/sha256/' || substr(sha256, 8, 2) || '/' || substr(sha256, 8) || '.webp'"
            ),
        ),
        query_checks=(
            QueryCheck(
                "image_variant_publishable",
                "SELECT 1 FROM image_variant AS v JOIN image_asset AS a ON a.id = v.image_id "
                "WHERE a.availability != 'available' OR a.publication_state != 'approved' LIMIT 1",
                ("image_variant", "image_asset"),
            ),
            QueryCheck(
                "image_variant_not_original",
                "SELECT 1 FROM image_variant AS v JOIN image_size AS s ON s.key = v.size_key "
                "WHERE s.is_original != 0 LIMIT 1",
                ("image_variant", "image_size"),
            ),
        ),
    ),
    Table(
        "image_size",
        (
            Column("key", Kind.ID, pattern=CODE),
            Column("purpose", Kind.TEXT),
            Column("max_width", Kind.UINT, nullable=True),
            Column("max_height", Kind.UINT, nullable=True),
            Column("is_original", Kind.BOOL),
        ),
        ("key",),
    ),
)
