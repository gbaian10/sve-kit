"""Image source metadata and public derivatives from build-db sections 11 and 17."""

from sve_carddb.build.model import Check, Column, ForeignKey, Kind, QueryCheck, Table
from sve_carddb.build.scalars import CODE, HASH

TABLES = (
    Table(
        "image_asset",
        (
            Column("id", Kind.ID),
            Column("publication_state", Kind.TEXT, choices=("pending", "approved")),
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
        foreign_keys=(ForeignKey(("source_id",), "source_record", ("id",)),),
        checks=(
            Check(
                "availability != 'available' OR (content_hash IS NOT NULL AND mime IS NOT NULL AND length(mime) > 0 AND width IS NOT NULL AND width > 0 AND height IS NOT NULL AND height > 0 AND bytes IS NOT NULL)"
            ),
            Check("publication_state != 'approved' OR availability = 'available'"),
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
