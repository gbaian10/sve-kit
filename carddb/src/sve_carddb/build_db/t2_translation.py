"""Complete translation-use/selection shapes with opt-in name-only validation."""

from sve_carddb.build_db.domains import LANG
from sve_carddb.build_db.model import (
    Check,
    Column,
    ForeignKey,
    Kind,
    QueryCheck,
    Table,
    Unique,
)

OWNERS = (
    ("face_revision_id",),
    ("printing_id", "face_id"),
    ("qa_version_id",),
    ("cr_clause_id",),
    ("vocabulary_kind", "vocabulary_code"),
    ("keyword_id",),
    ("product_family_id",),
    ("product_id",),
)


def _fk(column: str, table: str) -> ForeignKey:
    return ForeignKey((column,), table, ("id",))


TABLES = (
    Table(
        "translation_use",
        (
            Column("id", Kind.ID),
            Column("context_id", Kind.ID),
            Column("field", Kind.ID),
            Column("ordinal", Kind.UINT, nullable=True),
            *(
                Column(name, Kind.ID, nullable=True)
                for group in OWNERS
                for name in group
            ),
        ),
        ("id",),
        foreign_keys=(
            _fk("context_id", "translation_context"),
            _fk("face_revision_id", "face_revision"),
            ForeignKey(
                ("printing_id", "face_id"), "printing_face", ("printing_id", "face_id")
            ),
            _fk("qa_version_id", "qa_version"),
            _fk("cr_clause_id", "cr_clause"),
            ForeignKey(
                ("vocabulary_kind", "vocabulary_code"), "vocabulary", ("kind", "code")
            ),
            _fk("keyword_id", "keyword"),
            _fk("product_family_id", "product_family"),
            _fk("product_id", "product"),
        ),
        checks=(
            Check("(printing_id IS NULL) = (face_id IS NULL)"),
            Check("(vocabulary_kind IS NULL) = (vocabulary_code IS NULL)"),
            Check(
                " + ".join("(" + group[0] + " IS NOT NULL)" for group in OWNERS)
                + " = 1"
            ),
        ),
        unique=tuple(
            unique
            for group in OWNERS
            for unique in (
                Unique(
                    (*group, "field"),
                    where=group[0] + " IS NOT NULL AND ordinal IS NULL",
                ),
                Unique(
                    (*group, "field", "ordinal"),
                    where=group[0] + " IS NOT NULL AND ordinal IS NOT NULL",
                ),
            )
        ),
        query_checks=(
            QueryCheck(
                "name_use_supported_scope",
                "SELECT 1 FROM translation_use WHERE field != 'name' OR ordinal IS NOT NULL "
                "OR (face_revision_id IS NULL AND printing_id IS NULL) LIMIT 1",
                ("translation_use",),
            ),
            QueryCheck(
                "name_use_adopted_variant",
                "SELECT 1 FROM translation_use AS u JOIN translation_context AS c ON c.id=u.context_id "
                "LEFT JOIN decision AS d ON d.id=c.decision_id "
                "WHERE c.semantic_variant != 'default' AND (d.id IS NULL "
                "OR d.category!='context_assignment' OR NOT sve_is_maintainer(d.reviewed_by) "
                "OR d.state NOT IN ('sampled','confirmed') OR NOT EXISTS "
                "(SELECT 1 FROM decision_source AS s WHERE s.decision_id=d.id "
                "AND s.role LIKE 'name_identity:%')) LIMIT 1",
                (
                    "translation_use",
                    "translation_context",
                    "decision",
                    "decision_source",
                ),
            ),
            QueryCheck(
                "name_use_confirmed_identity",
                "SELECT 1 FROM translation_use AS u JOIN face_revision AS r ON r.id=u.face_revision_id "
                "JOIN face AS f ON f.id=r.face_id JOIN card AS c ON c.id=f.card_id WHERE c.identity_state!='confirmed' "
                "UNION ALL SELECT 1 FROM translation_use AS u JOIN printing AS p ON p.id=u.printing_id "
                "JOIN card AS c ON c.id=p.card_id WHERE c.identity_state!='confirmed' LIMIT 1",
                ("translation_use", "face_revision", "face", "printing", "card"),
            ),
            QueryCheck(
                "name_use_revision_source",
                "SELECT 1 FROM translation_use AS u JOIN translation_context AS c ON c.id=u.context_id "
                "JOIN face_revision AS r ON r.id=u.face_revision_id "
                "WHERE c.source_unit_id != r.name_unit_id LIMIT 1",
                ("translation_use", "translation_context", "face_revision"),
            ),
            QueryCheck(
                "name_use_printed_source",
                "SELECT 1 FROM translation_use AS u JOIN translation_context AS c ON c.id=u.context_id "
                "JOIN printing_face AS p ON p.printing_id=u.printing_id AND p.face_id=u.face_id "
                "WHERE p.printed_text_state IN ('unknown','omitted') OR p.printed_name_unit_id IS NULL "
                "OR c.source_unit_id != p.printed_name_unit_id LIMIT 1",
                ("translation_use", "translation_context", "printing_face"),
            ),
        ),
    ),
    Table(
        "translation_selection",
        (
            Column("context_id", Kind.ID),
            Column("target_lang", Kind.ID, pattern=LANG),
            Column("translation_id", Kind.ID),
        ),
        ("context_id", "target_lang"),
        foreign_keys=(
            _fk("context_id", "translation_context"),
            _fk("translation_id", "translation"),
        ),
        query_checks=(
            QueryCheck(
                "translation_selection_exact",
                "SELECT 1 FROM translation_selection AS s JOIN translation AS t ON t.id=s.translation_id "
                "WHERE s.context_id != t.context_id OR s.target_lang != t.target_lang "
                "OR t.status != 'reviewed' LIMIT 1",
                ("translation_selection", "translation"),
            ),
            QueryCheck(
                "name_selection_owner_eligibility",
                "SELECT 1 FROM translation_selection AS s JOIN translation AS t ON t.id=s.translation_id "
                "WHERE t.authority != 'unofficial' LIMIT 1",
                ("translation_selection", "translation"),
            ),
        ),
    ),
)
