"""Adopted digital illustrations and explicit art-level comparison coverage."""

from sve_carddb.build.model import Column, ForeignKey, Kind, QueryCheck, Table, Unique
from sve_carddb.core.dates import DATE


def _fk(column: str, table: str) -> ForeignKey:
    return ForeignKey((column,), table, ("id",))


TABLES = (
    Table(
        "digital_art",
        (
            Column("id", Kind.ID),
            Column("digital_face_id", Kind.ID),
            Column("style_key", Kind.TEXT),
            Column("source_card_id", Kind.ID),
            Column("source_image_url", Kind.TEXT, nullable=True),
            Column("image_hash_raw", Kind.TEXT, nullable=True),
            Column("illustrator_raw", Kind.TEXT, nullable=True),
            Column("source_id", Kind.ID),
        ),
        ("id",),
        foreign_keys=(
            _fk("digital_face_id", "digital_face"),
            _fk("source_card_id", "digital_card"),
            _fk("source_id", "source_record"),
        ),
        unique=(Unique(("digital_face_id", "style_key")),),
        query_checks=(
            QueryCheck(
                "digital_art_source_card",
                "SELECT 1 FROM digital_art AS a JOIN digital_face AS f ON f.id=a.digital_face_id WHERE a.source_card_id != f.digital_card_id LIMIT 1",
                ("digital_art", "digital_face"),
            ),
        ),
    ),
    Table(
        "digital_art_link",
        (
            Column("art_id", Kind.ID),
            Column("digital_art_id", Kind.ID),
            Column("relation", Kind.TEXT, choices=("same_art", "adapted_art")),
            Column("decision_id", Kind.ID),
        ),
        ("art_id", "digital_art_id"),
        foreign_keys=(
            _fk("art_id", "art"),
            _fk("digital_art_id", "digital_art"),
            _fk("decision_id", "decision"),
        ),
        query_checks=(
            QueryCheck(
                "digital_art_link_adopted",
                "SELECT 1 FROM digital_art_link AS l JOIN decision AS d ON d.id=l.decision_id WHERE d.state NOT IN ('model_reviewed','sampled','confirmed') LIMIT 1",
                ("digital_art_link", "decision"),
            ),
        ),
    ),
    Table(
        "digital_art_coverage",
        (
            Column("art_id", Kind.ID),
            Column("game", Kind.TEXT, choices=("sv1", "svwb")),
            Column("state", Kind.TEXT, choices=("matched", "not_found", "unknown")),
            Column("as_of", Kind.TEXT, pattern=DATE),
            Column("decision_id", Kind.ID),
        ),
        ("art_id", "game"),
        foreign_keys=(_fk("art_id", "art"), _fk("decision_id", "decision")),
        query_checks=(
            QueryCheck(
                "digital_art_coverage_adopted",
                "SELECT 1 FROM digital_art_coverage AS c JOIN decision AS d ON d.id=c.decision_id WHERE d.state NOT IN ('model_reviewed','sampled','confirmed') LIMIT 1",
                ("digital_art_coverage", "decision"),
            ),
            QueryCheck(
                "digital_art_coverage_matches",
                "SELECT 1 FROM digital_art_coverage AS c WHERE (c.state = 'matched') != EXISTS (SELECT 1 FROM digital_art_link AS l JOIN digital_art AS a ON a.id=l.digital_art_id JOIN digital_face AS f ON f.id=a.digital_face_id JOIN digital_card AS d ON d.id=f.digital_card_id WHERE l.art_id=c.art_id AND d.game=c.game) LIMIT 1",
                (
                    "digital_art_coverage",
                    "digital_art_link",
                    "digital_art",
                    "digital_face",
                    "digital_card",
                ),
            ),
        ),
    ),
)
