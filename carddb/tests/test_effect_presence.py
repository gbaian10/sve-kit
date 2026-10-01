"""Invented complete, truncated, ambiguous and double-face effect evidence."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.manifest import Kind, Region
from sve_carddb.snapshot.values import canonical, digest, parse
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources import official_en, official_jp
from sve_carddb.text_observations import FrozenTexts
from sve_carddb.text_observations.archive import verify_card
from sve_carddb.text_observations.presence import (
    PARSER,
    EffectPresence,
    detect_presence,
)
from sve_carddb.text_observations.report import observation_report

from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture
from .test_source_archive import _put, _resource, _store
from .text_observation_fixtures import make_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.records import Region as CardRegion
    from sve_carddb.registry.review import Inputs
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


def card_from_raw(tmp_path: Path, raw: bytes, region: CardRegion = "jp") -> TextCard:
    store = _store(tmp_path)
    url = (official_jp if region == "jp" else official_en).card_url("SYN-01")
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
    ).card(region, "SYN-01")
    assert card is not None
    return card


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
    assert use.source.values()["parser_version"] is None
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
