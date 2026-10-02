"""Invented bounded credit variants and independent v1 compatibility guards."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.html import parse, select_all
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.text_observations import presence, presence_v2

from .test_effect_presence import card_from_raw, page

if TYPE_CHECKING:
    from sve_carddb.build_inputs import Source
    from sve_carddb.registry.records import Region

CREDIT = '<div class="illustrator"><span class="heading">Synthetic artist</span><span class="name">SYN-01</span></div>'
ARTIST = '<div class="illustrator"><span class="heading">Synthetic artist</span></div>'
NOTICE = '<div class="illustrator"><span><font color="red">Synthetic notice <a href="https://example.invalid/notice">Synthetic link</a></font></span></div>'


@pytest.fixture(scope="module")
def source(tmp_path_factory: pytest.TempPathFactory) -> Source:
    """Share a sealed synthetic baseline; individual checks change only local bytes."""
    return card_from_raw(tmp_path_factory.mktemp("presence-v2"), page("jp")).source


def back_page(
    credit: str, *, effect: str = '<div class="detail">Synthetic rule</div>'
) -> bytes:
    """Only the second face uses the proposed credit variant."""
    raw = page("jp", effect, double=True)
    before, after = raw.rsplit(CREDIT.encode(), 1)
    return before + credit.encode() + after


@pytest.mark.parametrize("region", ["jp", "en"])
@pytest.mark.parametrize(
    "effect",
    ["", '<div class="detail"></div>', '<div class="detail">Synthetic rule</div>'],
)
def test_v2_preserves_every_baseline_state_and_separates_recipe(
    source: Source, region: Region, effect: str
) -> None:
    raw = page(region, effect)
    previous = presence.detect_presence(
        raw, source, region=region, number="SYN-01", source_index=0
    )
    result = presence_v2.detect_presence(
        raw, source, region=region, number="SYN-01", source_index=0
    )
    assert (result.result.state, result.result.reason_code) == (
        previous.result.state,
        previous.result.reason_code,
    )
    assert result.result.recipe == "effect-presence-v2"
    assert result.result.parser_version == "effect-presence-v2/detail-v2"
    assert (
        result.result.template_id
        == {
            "jp": "jp-card-detail-v2",
            "en": "en-card-detail-v2",
        }[region]
    )
    assert previous.result.recipe == "effect-presence-v1"
    assert previous.result.parser_version == "effect-presence-v1/detail-v1"
    assert result.result_hash == digest(
        canonical(result.result.model_dump(mode="json"))
    )
    with pytest.raises(ValidationError) as old_schema:
        presence.EffectPresence.model_validate_json(canonical(result.value()))
    assert [error["loc"] for error in old_schema.value.errors()] == [
        ("result", "recipe"),
        ("result", "parser_version"),
    ]
    with pytest.raises(ValidationError) as new_schema:
        presence_v2.EffectPresence.model_validate_json(canonical(previous.value()))
    assert [error["loc"] for error in new_schema.value.errors()] == [
        ("result", "recipe"),
        ("result", "parser_version"),
    ]


@pytest.mark.parametrize("credit", [ARTIST, ""])
@pytest.mark.parametrize("index", [0, 1])
def test_back_artist_or_omitted_credit_requires_a_valid_front_and_nonempty_back(
    source: Source, credit: str, index: int
) -> None:
    raw = back_page(credit)
    old = presence.detect_presence(
        raw, source, region="jp", number="SYN-01", source_index=index
    )
    proof = presence_v2.detect_presence(
        raw, source, region="jp", number="SYN-01", source_index=index
    )
    assert old.result.state == "unknown"
    assert proof.result.state == "present"
    assert proof.result.template_id == "jp-card-detail-credit-v2"


@pytest.mark.parametrize("credit", [ARTIST, ""])
@pytest.mark.parametrize("effect", ["", '<div class="detail"></div>'])
@pytest.mark.parametrize("front_present", [False, True])
def test_new_back_variants_do_not_prove_additional_absence(
    source: Source, credit: str, effect: str, front_present: bool
) -> None:
    if front_present:
        raw = back_page(credit)
        before, after = raw.rsplit(b'<div class="detail">Synthetic rule</div>', 1)
        raw = before + effect.encode() + after
    else:
        raw = back_page(credit, effect=effect)
    for index in (0, 1):
        assert (
            presence_v2.detect_presence(
                raw, source, region="jp", number="SYN-01", source_index=index
            ).result.state
            == "unknown"
        )


@pytest.mark.parametrize("variant", ["notice", "artist_back", "omitted_back"])
@pytest.mark.parametrize(
    "effect",
    [
        "",
        '<div class="detail"></div>',
        '<div class="detail"><span class="unknown">Synthetic rule</span></div>',
    ],
)
def test_new_variants_cannot_prove_absence_or_change_an_unknown_front_reason(
    source: Source, monkeypatch: pytest.MonkeyPatch, variant: str, effect: str
) -> None:
    if variant == "notice":
        node = select_all(parse(NOTICE), ".illustrator")[0]
        monkeypatch.setattr(
            presence_v2,
            "NOTICE_HASHES",
            frozenset({digest((node.html or "").encode())}),
        )
        raw = page("jp", effect).replace(CREDIT.encode(), (CREDIT + NOTICE).encode())
    else:
        raw = back_page(ARTIST if variant == "artist_back" else "")
        raw = raw.replace(
            b'<div class="detail">Synthetic rule</div>', effect.encode(), 1
        )
        back = presence_v2.detect_presence(
            raw, source, region="jp", number="SYN-01", source_index=1
        )
        assert (back.result.state, back.result.reason_code) == (
            "present",
            "nonempty_container",
        )
    previous = presence.detect_presence(
        raw, source, region="jp", number="SYN-01", source_index=0
    )
    proof = presence_v2.detect_presence(
        raw, source, region="jp", number="SYN-01", source_index=0
    )
    assert (previous.result.state, previous.result.reason_code) == (
        "unknown",
        "incomplete_source",
    )
    assert (proof.result.state, proof.result.reason_code) == (
        "unknown",
        "incomplete_source",
    )
    assert proof.result.template_id == "jp-card-detail-v2"


@pytest.mark.parametrize("credit", [ARTIST, ""])
@pytest.mark.parametrize(
    "change", ["missing_label", "missing_stat", "empty_title", "duplicate_title"]
)
def test_only_front_damage_blocks_a_complete_new_back_variant(
    source: Source, credit: str, change: str
) -> None:
    raw = back_page(credit)
    title = b'<h1 class="ttl">Synthetic name</h1>'
    changes = {
        "missing_label": ("<dt>クラス</dt>".encode(), b""),
        "missing_stat": (
            b'<span class="status-Item status-Item-Cost"><span class="heading">Cost</span>1</span>',
            b"",
        ),
        "empty_title": (b"Synthetic name", b""),
        "duplicate_title": (title, title * 2),
    }
    before, after = changes[change]
    raw = raw.replace(before, after, 1)
    for index in (0, 1):
        result = presence_v2.detect_presence(
            raw, source, region="jp", number="SYN-01", source_index=index
        ).result
        assert (result.state, result.reason_code) == (
            "unknown",
            "incomplete_source",
        )


@pytest.mark.parametrize(
    "credit",
    [
        ARTIST.replace("<span", "<p").replace("</span>", "</p>"),
        ARTIST.replace('class="heading"', 'class="other"'),
        ARTIST.replace('class="heading"', 'class="heading other"'),
    ],
)
def test_back_artist_requires_a_span_with_exact_heading_class(
    source: Source, credit: str
) -> None:
    raw = back_page(credit)
    for index in (0, 1):
        result = presence_v2.detect_presence(
            raw, source, region="jp", number="SYN-01", source_index=index
        ).result
        assert (result.state, result.reason_code) == (
            "unknown",
            "incomplete_source",
        )


@pytest.mark.parametrize(
    "effect",
    ["", '<div class="detail"></div>', '<div class="detail">Synthetic rule</div>'],
)
def test_known_v1_notice_preserves_its_conclusion_and_versions_template_id(
    source: Source, monkeypatch: pytest.MonkeyPatch, effect: str
) -> None:
    node = select_all(parse(NOTICE), ".illustrator")[0]
    monkeypatch.setattr(
        presence, "_JP_NOTICE_HASHES", frozenset({digest((node.html or "").encode())})
    )
    raw = page("jp", effect).replace(CREDIT.encode(), (CREDIT + NOTICE).encode())
    previous = presence.detect_presence(
        raw, source, region="jp", number="SYN-01", source_index=0
    )
    proof = presence_v2.detect_presence(
        raw, source, region="jp", number="SYN-01", source_index=0
    )
    assert previous.result.template_id == "jp-card-detail-notice-v1"
    assert proof.result.template_id == "jp-card-detail-notice-v2"
    assert (proof.result.state, proof.result.reason_code) == (
        previous.result.state,
        previous.result.reason_code,
    )


@pytest.mark.parametrize(
    "change",
    [
        "front_artist",
        "front_missing",
        "wrong_front_number",
        "wrong_back_number",
        "artist_number",
        "empty_artist",
        "nested_artist",
        "duplicate_artist",
        "artist_before_detail",
        "stray_text",
        "unknown_sibling",
        "missing_wrapper",
        "missing_label",
        "duplicate_label",
        "missing_stat",
        "duplicate_stat",
        "empty_title",
        "empty_image",
        "missing_tail",
        "duplicate_detail",
        "three_faces",
        "en",
    ],
)
def test_credit_variants_never_waive_page_or_identity_guards(
    source: Source, change: str
) -> None:
    raw = back_page(ARTIST)
    label = "<dt>クラス</dt>".encode()
    stat = b'<span class="status-Item status-Item-Cost"><span class="heading">Cost</span>1</span>'
    effect = b'<div class="detail">Synthetic rule</div>'
    edits = {
        "front_artist": (CREDIT.encode(), ARTIST.encode(), 1),
        "front_missing": (CREDIT.encode(), b"", 1),
        "wrong_front_number": (
            CREDIT.encode(),
            CREDIT.replace("SYN-01", "SYN-02").encode(),
            1,
        ),
        "wrong_back_number": (
            ARTIST.encode(),
            CREDIT.replace("SYN-01", "SYN-02").encode(),
            1,
        ),
        "artist_number": (
            ARTIST.encode(),
            ARTIST.replace("Synthetic artist", "SYN-02").encode(),
            1,
        ),
        "empty_artist": (
            ARTIST.encode(),
            ARTIST.replace("Synthetic artist", "").encode(),
            1,
        ),
        "nested_artist": (
            ARTIST.encode(),
            ARTIST.replace("Synthetic artist", "<b>Synthetic artist</b>").encode(),
            1,
        ),
        "duplicate_artist": (ARTIST.encode(), ARTIST.encode() * 2, 1),
        "artist_before_detail": (effect + ARTIST.encode(), ARTIST.encode() + effect, 1),
        "stray_text": (ARTIST.encode(), b"Synthetic stray" + ARTIST.encode(), 1),
        "unknown_sibling": (
            ARTIST.encode(),
            b'<div class="unknown">Synthetic</div>' + ARTIST.encode(),
            1,
        ),
        "missing_wrapper": (b'class="txt-Inner"', b'class="unknown"', -1),
        "missing_label": (label, b"", -1),
        "duplicate_label": (label, label * 2, -1),
        "missing_stat": (stat, b"", -1),
        "duplicate_stat": (stat, stat * 2, -1),
        "empty_title": (b"Synthetic name", b"", -1),
        "empty_image": (b"/synthetic.png", b"", -1),
        "missing_tail": (b"</body></html>", b"", 1),
        "duplicate_detail": (
            b'<div class="cardlist-Detail">',
            b'<div class="cardlist-Detail"></div><div class="cardlist-Detail">',
            1,
        ),
    }
    if change in edits:
        before, after, count = edits[change]
        assert before in raw
        raw = raw.replace(before, after, count)
    elif change == "three_faces":
        face = select_all(parse(raw.decode()), ".cardlist-Detail_Box_Inner")[0]
        raw = raw.replace(b"</body>", (face.html or "").encode() + b"</body>")
    else:
        raw = page("en", '<div class="detail">Synthetic rule</div>', double=True)
        before, after = raw.rsplit(CREDIT.encode(), 1)
        raw = before + ARTIST.encode() + after
    region: Region = "en" if change == "en" else "jp"
    for index in (0, 1):
        proof = presence_v2.detect_presence(
            raw, source, region=region, number="SYN-01", source_index=index
        )
        assert proof.result.state == "unknown"


@pytest.mark.parametrize(
    "change",
    [
        "none",
        "text",
        "href",
        "markup",
        "unknown",
        "duplicate",
        "before_credit",
        "after_other_sibling",
        "has_name",
        "has_heading",
        "wrong_number",
    ],
)
def test_new_notice_requires_exact_hash_position_and_real_credit(
    source: Source, monkeypatch: pytest.MonkeyPatch, change: str
) -> None:
    node = select_all(parse(NOTICE), ".illustrator")[0]
    monkeypatch.setattr(
        presence_v2, "NOTICE_HASHES", frozenset({digest((node.html or "").encode())})
    )
    notice = NOTICE
    if change == "text":
        notice = notice.replace("Synthetic notice", "Synthetic different notice")
    elif change == "href":
        notice = notice.replace("/notice", "/different")
    elif change == "markup":
        notice = notice.replace("<span>", '<span class="unknown">')
    elif change == "unknown":
        monkeypatch.setattr(presence_v2, "NOTICE_HASHES", frozenset())
    elif change == "duplicate":
        notice *= 2
    elif change in {"has_name", "has_heading"}:
        marker = "name" if change == "has_name" else "heading"
        notice = notice.replace("<span>", f'<span class="{marker}">')
        node = select_all(parse(notice), ".illustrator")[0]
        monkeypatch.setattr(
            presence_v2,
            "NOTICE_HASHES",
            frozenset({digest((node.html or "").encode())}),
        )
    raw = page("jp", '<div class="detail">Synthetic rule</div>')
    raw = raw.replace(
        CREDIT.encode(),
        (notice + CREDIT if change == "before_credit" else CREDIT + notice).encode(),
    )
    if change == "after_other_sibling":
        raw = raw.replace(
            notice.encode(),
            b'<div class="speech">Synthetic flavor</div>' + notice.encode(),
        )
    elif change == "wrong_number":
        raw = raw.replace(b"SYN-01", b"SYN-02")
    assert (
        presence.detect_presence(
            raw, source, region="jp", number="SYN-01", source_index=0
        ).result.state
        == "unknown"
    )
    proof = presence_v2.detect_presence(
        raw, source, region="jp", number="SYN-01", source_index=0
    )
    assert proof.result.state == ("present" if change == "none" else "unknown")


@pytest.mark.parametrize(
    "field", ["hash", "state_reason", "template", "parser", "recipe"]
)
def test_v2_evidence_rejects_tampered_versions_and_results(
    source: Source, field: str
) -> None:
    proof = presence_v2.detect_presence(
        page("jp"), source, region="jp", number="SYN-01", source_index=0
    )
    value = proof.value()
    result = value["result"]
    assert isinstance(result, dict)
    if field == "hash":
        value["result_hash"] = "sha256:" + "0" * 64
    else:
        updates: dict[str, dict[str, JsonValue]] = {
            "state_reason": {"state": "present"},
            "template": {"template_id": None},
            "parser": {"parser_version": presence.PARSER},
            "recipe": {"recipe": "effect-presence-v1"},
        }
        result.update(updates[field])
        value["result_hash"] = digest(canonical(result))
    with pytest.raises(ValidationError) as outcome:
        presence_v2.EffectPresence.model_validate_json(canonical(value))
    messages = {
        "hash": "Value error, V2 effect presence result hash mismatch",
        "state_reason": "Value error, Invalid v2 effect presence state/reason pair",
        "template": "Value error, V2 effect presence requires a recognized template",
        "parser": "Input should be 'effect-presence-v2/detail-v2'",
        "recipe": "Input should be 'effect-presence-v2'",
    }
    assert [error["msg"] for error in outcome.value.errors()] == [messages[field]]
