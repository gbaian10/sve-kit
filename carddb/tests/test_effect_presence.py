"""Invented complete, truncated, ambiguous and double-face effect evidence."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.build_db.source_rows import source_values
from sve_carddb.core.json import canonical, digest, parse
from sve_carddb.html import parse as parse_html
from sve_carddb.html import select_all
from sve_carddb.manifest import Kind, Region
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.source_archive import seal_batch
from sve_carddb.source_corrections.plan import corrected_observations
from sve_carddb.sources import official_en, official_jp
from sve_carddb.text_observations import FrozenTexts, presence
from sve_carddb.text_observations import plan as planning
from sve_carddb.text_observations.archive import verify_card
from sve_carddb.text_observations.presence import (
    PARSER,
    EffectPresence,
    PresenceResult,
    detect_presence,
)
from sve_carddb.text_observations.report import observation_report

from .registry_snapshot_fixtures import edit_record
from .source_correction_fixtures import make_correction_case
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture
from .test_source_archive import _put, _resource, _store
from .text_observation_fixtures import make_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.records import Region as CardRegion
    from sve_carddb.registry.review import Inputs
    from sve_carddb.registry.storage import Entry
    from sve_carddb.text_observations.models import TextCard


def page(region: CardRegion, effect: str = "", *, double: bool = False) -> bytes:
    labels = (
        ["クラス", "カード種類", "タイプ", "レアリティ"]
        if region == "jp"
        else ["Class", "Card Type", "Trait", "Rarity"]
    )
    values = ["-", "Synthetic type", "-", "LG"]
    info = "".join(
        f"<dl><dt>{key}</dt><dd>{value}</dd></dl>"
        for key, value in zip(labels, values, strict=True)
    )
    stats = "".join(
        f'<span class="status-Item status-Item-{kind}"><span class="heading">{kind}</span>1</span>'
        for kind in ("Cost", "Power", "Hp")
    )
    face = f'<div class="cardlist-Detail_Box_Inner"><div class="img w100"><img src="/synthetic.png"></div><div class="txt"><h1 class="ttl">Synthetic name</h1><div class="txt-Inner"><div class="info">{info}</div><div class="status">{stats}</div>{effect}<div class="illustrator"><span class="heading">Synthetic artist</span><span class="name">SYN-01</span></div></div></div></div>'
    return (
        '<html><body><div class="cardlist-Detail">'
        + face
        + (face if double else "")
        + "</div><!--"
        + "x" * 1100
        + "--></body></html>"
    ).encode()


def card_from_raw(
    tmp_path: Path, raw: bytes, region: CardRegion = "jp", *, number: str = "SYN-01"
) -> TextCard:
    store = _store(tmp_path)
    url = (official_jp if region == "jp" else official_en).card_url(number)
    resource = replace(
        _resource(url, "raw/synthetic.html", raw, Kind.CARD),
        region=Region.JP if region == "jp" else Region.EN,
    )
    _put(store, resource, raw)
    sealed = seal_batch(store)
    card = FrozenTexts(
        store.root,
        store.store_id,
        sealed.batch_id,
        region=region,
        parser_version="synthetic-extractor-pin",
    ).card(region, number)
    assert card is not None
    return card


@pytest.mark.parametrize(
    "change",
    [
        "none",
        "text",
        "href",
        "markup",
        "duplicate",
        "before_credit",
        "wrong_credit",
        "en",
    ],
)
def test_notice_template_accepts_only_pinned_terminal_block_after_valid_credit(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, change: str
) -> None:
    notice = '<div class="illustrator"><span><font color="red">Synthetic publication notice <a href="https://example.invalid/notice">Synthetic link</a></font></span></div>'
    html = parse_html(notice)
    notice_hash = digest((select_all(html, ".illustrator")[0].html or "").encode())
    monkeypatch.setattr(presence, "_JP_NOTICE_HASHES", frozenset({notice_hash}))
    region: CardRegion = "en" if change == "en" else "jp"
    raw = page(region)
    if change in {"text", "href", "markup"}:
        replacements = {
            "text": ("publication notice", "rule with a different meaning"),
            "href": ("/notice", "/different"),
            "markup": ("<span>", '<span class="extra">'),
        }
        before, after = replacements[change]
        notice = notice.replace(before, after)
    elif change == "duplicate":
        notice += notice
    if change == "before_credit":
        raw = raw.replace(
            b'<div class="illustrator">',
            notice.encode() + b'<div class="illustrator">',
            1,
        )
    else:
        raw = raw.replace(
            b"</span></div></div></div></div>",
            b"</span></div>" + notice.encode() + b"</div></div></div>",
        )
    if change == "wrong_credit":
        raw = raw.replace(b"SYN-01", b"SYN-02")
    card = card_from_raw(tmp_path, raw, region)
    proof = card.effect_presence[0].result
    if change == "none":
        assert proof.state == "absent"
        assert proof.template_id == "jp-card-detail-notice-v1"
        assert card.projected(0).effect is not None
        assert not card.projected(0).effect
        verify_card(card)
    else:
        assert proof.state == "unknown"
        assert card.projected(0).effect is None


@pytest.mark.parametrize("region", ["jp", "en"])
@pytest.mark.parametrize(
    ("container", "state", "reason", "effect"),
    [
        ("", "absent", "template_omits_empty_effect", ""),
        ('<div class="detail"></div>', "absent", "empty_container", ""),
        ('<div class="detail"><br><p></p></div>', "absent", "empty_container", ""),
        (
            '<div class="detail">Synthetic rule</div>',
            "present",
            "nonempty_container",
            "Synthetic rule",
        ),
        ('<div class="detail"> \t </div>', "present", "nonempty_container", " \t "),
        (
            '<div class="detail"><span class="unrecognized-icon"></span></div>',
            "unknown",
            "ambiguous_container",
            None,
        ),
        (
            '<div class="detail"><svg></svg></div>',
            "unknown",
            "ambiguous_container",
            None,
        ),
        (
            '<div class="detail"></div><div class="detail"></div>',
            "unknown",
            "ambiguous_container",
            None,
        ),
        ('<div class="mystery"></div>', "unknown", "ambiguous_container", None),
    ],
)
def test_projection_preserves_raw_and_requires_complete_evidence(
    tmp_path: Path,
    region: CardRegion,
    container: str,
    state: str,
    reason: str,
    effect: str | None,
) -> None:
    card = card_from_raw(tmp_path, page(region, container), region)
    result = card.effect_presence[0]
    assert result.result.state == state
    assert result.result.reason_code == reason
    assert card.projected(0).effect == effect
    assert card.raw is not None
    assert result.result_hash == digest(
        canonical(result.result.model_dump(mode="json"))
    )
    assert result.result.source_version_id == card.source.id
    assert result.result.source_index == 0
    verify_card(card)


@pytest.mark.parametrize(
    "mutation",
    [
        "footer",
        "status",
        "identity",
        "face_index",
        "second_face",
        "detail_root",
        "wrapper",
    ],
)
def test_no_absence_from_truncation_wrong_face_or_unknown_template(
    tmp_path: Path, mutation: str
) -> None:
    raw = page("jp", double=True)
    card = card_from_raw(tmp_path, raw)
    index = 0
    if mutation == "footer":
        raw = raw.replace(b"</body></html>", b"")
    elif mutation == "status":
        raw = raw.replace(b'class="status"', b'class="lost-status"', 1)
    elif mutation == "identity":
        raw = raw.replace(b"SYN-01", b"OTHER-01", 1)
    elif mutation == "face_index":
        index = 2
    elif mutation == "second_face":
        raw = b"".join(raw.rsplit(b"Synthetic name", 1))
    elif mutation == "detail_root":
        raw = raw.replace(b'class="cardlist-Detail"', b'class="new-template"')
    elif mutation == "wrapper":
        raw = raw.replace(b'class="txt-Inner"', b'class="new-wrapper"')
    result = detect_presence(
        raw, card.source, region="jp", number="SYN-01", source_index=index
    )
    assert result.result.state == "unknown"


@pytest.mark.parametrize(
    "field",
    [
        "state",
        "source_index",
        "parser_version",
        "template_id",
        "container_locator",
        "source_version_id",
        "reason_code",
    ],
)
def test_each_result_field_is_hash_bound(tmp_path: Path, field: str) -> None:
    card = card_from_raw(tmp_path, page("jp"))
    value = card.effect_presence[0].model_dump(mode="json")
    value["result"][field] = 1 if field == "source_index" else "tampered"
    with pytest.raises(ValidationError):
        EffectPresence.model_validate(value)


def test_self_consistent_forgery_cannot_bypass_source_reproduction(
    tmp_path: Path,
) -> None:
    card = card_from_raw(tmp_path, page("jp", '<div class="detail"><svg></svg></div>'))
    result = card.effect_presence[0].result.model_copy(
        update={"state": "absent", "reason_code": "empty_container"}
    )
    proof = EffectPresence(
        result=result, result_hash=digest(canonical(result.model_dump(mode="json")))
    )
    with pytest.raises(ValueError, match="reproduced"):
        verify_card(card.model_copy(update={"effect_presence": (proof,)}))
    with pytest.raises(ValueError, match="hash"):
        verify_card(card.model_copy(update={"raw": b"other source"}))


def test_two_faces_keep_independent_presence_and_sections(tmp_path: Path) -> None:
    raw = page("en", double=True).replace(
        b'</div><div class="illustrator">', b'</div><div class="illustrator">', 1
    )
    raw = raw.replace(
        b'<div class="status">',
        b'<div class="detail">-----<br>Synthetic auxiliary</div><div class="status">',
        1,
    )
    card = card_from_raw(tmp_path, raw, "en")
    assert card.projected(0).sections == ("Synthetic auxiliary",)
    assert card.projected(0).effect is not None
    assert not card.projected(0).effect
    assert card.effect_presence[0].result.state == "present"
    assert card.effect_presence[1].result.state == "absent"
    assert card.projected(1).effect is not None
    assert not card.projected(1).effect


@pytest.mark.parametrize("unknown", [False, True])
def test_presence_use_and_report_bind_full_reference_evidence(
    tmp_path: Path, inputs: Inputs, unknown: bool
) -> None:
    case = make_case(tmp_path / "authored", inputs)
    card = card_from_raw(
        tmp_path / "sealed",
        page("jp", '<div class="detail"><svg></svg></div>' if unknown else ""),
    )
    original = case.plan.observations[0]
    item = original.model_copy(update={"card": card, "content": card.projected(0)})
    plan = replace(case.plan, observations=(item,))
    uses = plan.source_uses()
    assert len(uses) == 3
    use = next(use for use in uses if use.usage == "effect_presence")
    locator = parse(use.locator.encode())
    assert isinstance(locator, dict)
    assert locator["printing_id"] == item.printing_id
    assert locator["face_id"] == item.face_id
    assert locator["source_index"] == 0
    assert locator["result_hash"] == card.effect_presence[0].result_hash
    assert use.source.parser_version == PARSER
    assert source_values(use.source)["parser_version"] is None
    assert use.source.archive == card.source.archive
    report = observation_report(item)
    assert report["effect_presence"] == card.effect_presence[0].value()
    assert report["raw_face_hash"] == card.faces[0].fingerprint()
    if not unknown:
        assert report["content_hash"] != report["raw_face_hash"]
    assert "Synthetic name" not in canonical(report).decode()
    assert plan.configuration() != case.plan.configuration()


def test_required_status_is_checked_even_for_existing_empty_container(
    tmp_path: Path,
) -> None:
    card = card_from_raw(tmp_path, page("jp", '<div class="detail"></div>'))
    assert card.raw is not None
    raw = card.raw.replace(b'class="status"', b'class="lost-status"')
    proof = detect_presence(
        raw, card.source, region="jp", number="SYN-01", source_index=0
    )
    assert proof.result.state == "unknown"
    assert proof.result.reason_code == "incomplete_source"


@pytest.mark.parametrize("effect", ["Synthetic rule", " "])
def test_absent_cannot_erase_nonempty_extractor_effect(
    tmp_path: Path, effect: str
) -> None:
    card = card_from_raw(tmp_path, page("jp"))
    changed = card.faces[0].model_copy(update={"effect": effect})
    with pytest.raises(ValueError, match="contradicts extracted effect"):
        card.model_copy(update={"faces": (changed,)}).projected(0)


@pytest.mark.parametrize("effect", [None, ""])
def test_absent_accepts_only_null_or_empty_effect(
    tmp_path: Path, effect: str | None
) -> None:
    card = card_from_raw(tmp_path, page("jp"))
    changed = card.faces[0].model_copy(update={"effect": effect})
    projected = card.model_copy(update={"faces": (changed,)}).projected(0)
    assert projected.effect is not None
    assert not projected.effect


@pytest.mark.parametrize("region", ["jp", "en"])
def test_textless_icon_container_is_present(tmp_path: Path, region: CardRegion) -> None:
    card = card_from_raw(
        tmp_path,
        page(
            region,
            '<div class="detail"><img src="/synthetic-icon.png" alt="Synthetic icon"></div>',
        ),
        region,
    )
    assert card.effect_presence[0].result.state == "present"
    assert card.effect_presence[0].result.reason_code == "nonempty_container"
    assert card.projected(0).effect is not None


@pytest.mark.parametrize("change", ["stray_text", "terminal_credit"])
def test_omission_requires_clean_direct_text_and_terminal_credit(
    tmp_path: Path, change: str
) -> None:
    card = card_from_raw(tmp_path, page("jp"))
    assert card.raw is not None
    if change == "stray_text":
        raw = card.raw.replace(
            b'<div class="txt-Inner">', b'<div class="txt-Inner">Synthetic stray rule'
        )
    else:
        raw = card.raw.replace(
            b"</span></div></div></div></div>",
            b'</span></div><div class="speech"></div></div></div></div>',
        )
    assert raw != card.raw
    proof = detect_presence(
        raw, card.source, region="jp", number="SYN-01", source_index=0
    )
    assert proof.result.state == "unknown"
    assert proof.result.reason_code == "ambiguous_container"


@pytest.mark.parametrize("change", ["labels", "Cost", "Power", "Hp", "faces", "detail"])
def test_each_completeness_constraint_independently_blocks_absence(
    tmp_path: Path, change: str
) -> None:
    card = card_from_raw(tmp_path, page("en", '<div class="detail"></div>'), "en")
    assert card.raw is not None
    if change == "labels":
        raw = card.raw.replace(b"<dt>Rarity</dt>", b"<dt>Synthetic label</dt>")
    elif change in {"Cost", "Power", "Hp"}:
        raw = card.raw.replace(
            f"status-Item-{change}".encode(), b"status-Item-Synthetic"
        )
    elif change == "faces":
        face = card.raw.split(b'<div class="cardlist-Detail">', 1)[1].split(
            b"</div><!--", 1
        )[0]
        raw = card.raw.replace(face, face * 3, 1)
    else:
        raw = card.raw.replace(
            b"</div><!--", b'</div><div class="cardlist-Detail"></div><!--'
        )
    assert raw != card.raw
    proof = detect_presence(
        raw, card.source, region="en", number="SYN-01", source_index=0
    )
    assert proof.result.state == "unknown"


@pytest.mark.parametrize(
    ("state", "reason"),
    [
        ("absent", "nonempty_container"),
        ("present", "empty_container"),
        ("unknown", "empty_container"),
    ],
)
def test_state_reason_pairs_reject_even_self_consistent_hashes(
    tmp_path: Path, state: str, reason: str
) -> None:
    value = (
        card_from_raw(tmp_path, page("jp")).effect_presence[0].model_dump(mode="json")
    )
    value["result"].update(state=state, reason_code=reason)
    value["result_hash"] = digest(canonical(value["result"]))
    with pytest.raises(ValidationError, match="state/reason pair"):
        EffectPresence.model_validate(value)


def test_known_state_requires_template_even_with_matching_hash(tmp_path: Path) -> None:
    value = (
        card_from_raw(tmp_path, page("jp"))
        .effect_presence[0]
        .result.model_dump(mode="json")
    )
    value["template_id"] = None
    with pytest.raises(ValidationError, match="recognized template"):
        PresenceResult.model_validate(value)


@pytest.mark.parametrize("field", ["effect_presence", "raw_face_hash"])
def test_configuration_pins_evidence_and_original_face_independently(
    tmp_path: Path, inputs: Inputs, field: str
) -> None:
    case = make_case(tmp_path / "authored", inputs)
    card = card_from_raw(tmp_path / "sealed", page("jp"))
    item = case.plan.observations[0].model_copy(
        update={"card": card, "content": card.projected(0)}
    )
    plan = replace(case.plan, observations=(item,))
    if field == "effect_presence":
        result = card.effect_presence[0].result.model_copy(
            update={"reason_code": "empty_container"}
        )
        proof = EffectPresence(
            result=result, result_hash=digest(canonical(result.model_dump(mode="json")))
        )
        altered = card.model_copy(update={"effect_presence": (proof,)})
    else:
        altered = card.model_copy(
            update={"faces": (card.faces[0].model_copy(update={"effect": ""}),)}
        )
    changed = replace(plan, observations=(item.model_copy(update={"card": altered}),))
    assert changed.observations[0].content == plan.observations[0].content
    assert changed.configuration() != plan.configuration()


def test_presence_without_frozen_bytes_is_rejected(tmp_path: Path) -> None:
    card = card_from_raw(tmp_path, page("jp"))
    with pytest.raises(ValueError, match="no frozen source bytes"):
        verify_card(card.model_copy(update={"raw": None}))


def test_planning_verifies_card_before_processing_evidence(
    tmp_path: Path, inputs: Inputs, monkeypatch: pytest.MonkeyPatch
) -> None:
    case = make_case(tmp_path, inputs)

    def rejected(card: TextCard) -> None:
        assert card in case.provider.cards.values()
        raise RuntimeError("first-line verification")

    monkeypatch.setattr(planning, "verify_card", rejected)
    with pytest.raises(RuntimeError, match="first-line verification"):
        planning.plan_text_observations(case.identity, case.provider)


def test_absent_projection_is_preserved_when_source_correction_changes_type(
    tmp_path: Path, inputs: Inputs
) -> None:
    fixture = make_correction_case(
        tmp_path / "authored", inputs, region="en", field="card_type"
    )
    assert fixture.texts.plan.corrections is not None
    case = fixture.texts
    number = "BP02-070EN"
    card = card_from_raw(
        tmp_path / "sealed",
        page("en")
        .replace(b"Synthetic type", b"Spell")
        .replace(b"SYN-01", number.encode()),
        "en",
        number=number,
    )

    def edit(entry: Entry) -> None:
        entry.data["expected_source_hash"] = card.observation.observation_hash

    edit_record(case.root, "source_correction", edit)
    evidence = dict(case.identity.evidence)
    evidence["en", number] = replace(
        evidence["en", number], source=card.source, observation=card.observation
    )
    identity = replace(
        case.identity, snapshot=load_registry(case.root), evidence=evidence
    )
    case.provider.cards["en", number] = card
    plan = planning.plan_text_observations(
        identity, case.provider, images=fixture.images
    )
    assert plan.corrections is not None
    application = plan.corrections[0]
    assert application.status == "applied"
    raw = application.observation
    candidate = next(
        item for item in plan.candidates() if item.printing_id == raw.printing_id
    )
    assert candidate == corrected_observations((raw,), (application,))[0]
    assert card.faces[0].effect is None
    assert card.effect_presence[0].result.state == "absent"
    assert raw.content.effect is not None
    assert not raw.content.effect
    assert candidate.content.effect is not None
    assert not candidate.content.effect
    assert raw.content.type_raw == "Spell"
    assert candidate.content.type_raw == "Follower"
    assert candidate.correction_keys == (application.key(),)
    assert (
        candidate.content.fingerprint()
        == raw.content.model_copy(update={"type_raw": "Follower"}).fingerprint()
    )
    assert candidate.content.fingerprint() != raw.content.fingerprint()
    report = application.report()
    assert report["raw_face_hash"] == card.faces[0].fingerprint()
    assert report["projected_face_hash"] == raw.content.fingerprint()
    assert report["raw_face_hash"] != report["projected_face_hash"]
    assert {use.usage for use in plan.source_uses()} == {
        "face_text_observation",
        "face_current_comparison",
        "effect_presence",
        "source_correction_comparison",
        "source_correction_evidence",
    }


def test_raw_observation_cannot_inherit_corrected_candidate_keys(
    tmp_path: Path, inputs: Inputs
) -> None:
    plan = make_correction_case(tmp_path, inputs).texts.plan
    item = plan.observations[0].model_copy(update={"correction_keys": ("tampered",)})
    with pytest.raises(ValueError, match="content/source face mismatch"):
        planning.verify_plan(replace(plan, observations=(item, *plan.observations[1:])))
