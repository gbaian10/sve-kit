"""Real-repository tests paused until they are made cheap enough for CI (#1104)."""

REASON = "Paused until the real-repository test is made cheaper (#1104)"

DEFERRED = frozenset(
    {
        "tests/domains/effects/test_author_anchors.py::test_current_anchor_grants_the_same_complete_semantic_handoff",
        "tests/domains/effects/test_author_current.py::test_real_current_author_set",
        "tests/domains/effects/test_author_current_draw.py::test_real_current_draw",
        "tests/domains/effects/test_author_current_keywords.py::test_real_printed_keywords_retain_exact_current_face",
        "tests/domains/effects/test_author_damage_current.py::test_real_lastword_current_boundary",
        "tests/domains/effects/test_author_destroy_current.py::test_real_advance_body_requires_complete_keyword_declaration",
        "tests/domains/effects/test_author_keyword_current_reconciliation.py::test_real_current_card_emits_closed_keyword_handoff",
        "tests/domains/effects/test_author_keyword_declarations.py::test_current_keyword_cannot_borrow_a_real_body_source",
        "tests/domains/effects/test_author_move_current.py::test_real_permanent_token_sources_and_whole_card_rejection",
        "tests/domains/effects/test_author_numeric_current.py::test_real_numeric_current_boundary",
        "tests/domains/effects/test_author_search_current.py::test_real_named_search_current",
        "tests/domains/effects/test_current_keyword_fixture.py::test_real_keyword_handoff_contains_every_printed_sibling",
    }
)


def is_deferred(nodeid: str) -> bool:
    """Match a collected test, ignoring its parameter suffix."""
    return nodeid.split("[", 1)[0] in DEFERRED
