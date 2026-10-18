"""Illustration candidates use self-authored textures and explicit face identity."""

import io
import random
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from PIL import Image, ImageDraw

from sve_carddb.core.json import digest
from sve_carddb.images.art_matching import (
    ArtCandidates,
    ArtGroup,
    ArtInput,
    ArtMember,
    ArtReview,
    ArtScore,
    compare_art,
)

if TYPE_CHECKING:
    from pathlib import Path


def texture(seed: int) -> Image.Image:
    rng = random.Random(seed)  # ruff: ignore[suspicious-non-cryptographic-random-usage] -- reproducible synthetic image texture
    image = Image.new("RGB", (384, 288), "white")
    draw = ImageDraw.Draw(image)
    for _ in range(300):
        x, y = rng.randrange(370), rng.randrange(274)
        radius = rng.randrange(2, 16)
        shade = rng.randrange(256)
        draw.ellipse((x, y, x + radius, y + radius), fill=(shade,) * 3)
    return image


def bitmap(
    root: Path,
    number: int,
    image: Image.Image,
    *,
    face: str = "f:one",
    region: str = "jp",
) -> ArtInput:
    data = io.BytesIO()
    image.save(data, "WEBP", lossless=True)
    raw = data.getvalue()
    path = root / f"{number}.webp"
    path.write_bytes(raw)
    return ArtInput(
        ArtMember.model_validate(
            {
                "printing_id": f"p:{number}",
                "face_id": face,
                "card_id": "c:one",
                "region": region,
                "card_no": f"SYN-{number}",
                "int_id": number,
                "ordinal": 0,
                "image_sha256": digest(raw),
            }
        ),
        path,
    )


@pytest.fixture
def art_inputs(tmp_path: Path) -> tuple[ArtInput, ...]:
    original = texture(1)
    framed = original.copy()
    draw = ImageDraw.Draw(framed)
    draw.rectangle((0, 0, 383, 287), outline="black", width=12)
    draw.text((90, 130), "SYNTHETIC SIGNATURE", fill="red", stroke_width=1)
    return (
        bitmap(tmp_path, 1, original),
        bitmap(tmp_path, 2, original, region="en"),
        bitmap(tmp_path, 3, framed),
        bitmap(tmp_path, 4, texture(2)),
        bitmap(tmp_path, 5, original, face="f:other"),
    )


def test_same_art_survives_frame_and_text_but_not_another_face(
    art_inputs: tuple[ArtInput, ...], tmp_path: Path
) -> None:
    result = compare_art(art_inputs, image_index_sha256="sha256:" + "0" * 64)
    grouped = {frozenset(group.members) for group in result.candidates.groups}
    assert grouped == {
        frozenset(item.member.key for item in art_inputs[:3]),
        frozenset({art_inputs[3].member.key}),
        frozenset({art_inputs[4].member.key}),
    }
    assert result.compared_pairs == 6
    assert not result.candidates.conflicts
    path = tmp_path / "candidates.json"
    path.write_bytes(result.candidates.content())
    assert ArtCandidates.read(path) == result.candidates


def test_incremental_scores_equal_full_run_and_reuse_old_pairs(
    art_inputs: tuple[ArtInput, ...],
) -> None:
    pin = "sha256:" + "0" * 64
    first = compare_art(art_inputs[:3], image_index_sha256=pin)
    incremental = compare_art(
        art_inputs, image_index_sha256=pin, previous=first.candidates
    )
    full = compare_art(art_inputs, image_index_sha256=pin)
    assert incremental.candidates == full.candidates
    assert incremental.reused_pairs == 3
    assert incremental.compared_pairs == 3
    repeated = compare_art(art_inputs, image_index_sha256=pin, previous=full.candidates)
    assert repeated.compared_pairs == 0
    assert repeated.candidates.content() == full.candidates.content()


def test_incremental_member_inserted_before_old_members(
    art_inputs: tuple[ArtInput, ...],
) -> None:
    pin = "sha256:" + "0" * 64
    previous = compare_art(art_inputs[1:], image_index_sha256=pin)
    updated = compare_art(
        art_inputs, image_index_sha256=pin, previous=previous.candidates
    )
    assert (
        updated.candidates == compare_art(art_inputs, image_index_sha256=pin).candidates
    )
    assert updated.reused_pairs == 3
    assert updated.compared_pairs == 3


@pytest.mark.parametrize("change", ["hash", "removed", "version", "incomplete"])
def test_stale_or_incomplete_incremental_input_requires_full_run(
    art_inputs: tuple[ArtInput, ...], change: str
) -> None:
    pin = "sha256:" + "0" * 64
    first = compare_art(art_inputs, image_index_sha256=pin).candidates
    current = art_inputs
    if change == "hash":
        member = art_inputs[0].member.model_copy(update={"image_sha256": pin})
        current = (replace(art_inputs[0], member=member), *art_inputs[1:])
    elif change == "removed":
        current = art_inputs[1:]
    elif change == "version":
        first = first.model_copy(update={"opencv_version": "another-version"})
    else:
        first = first.model_copy(update={"scores": first.scores[1:]})
    with pytest.raises(ValueError, match=r"Previous art|every same-face"):
        compare_art(current, image_index_sha256=pin, previous=first)


def test_blank_art_has_no_features_and_corrupt_art_fails(tmp_path: Path) -> None:
    first = bitmap(tmp_path, 1, Image.new("RGB", (384, 288)))
    second = bitmap(tmp_path, 2, Image.new("RGB", (384, 288)))
    pin = "sha256:" + "0" * 64
    result = compare_art((first, second), image_index_sha256=pin)
    assert result.candidates.scores[0].inliers == 0
    second.path.write_bytes(b"invalid")
    with pytest.raises(ValueError, match="changed during comparison"):
        compare_art((first, second), image_index_sha256=pin)
    second = replace(
        second,
        member=second.member.model_copy(update={"image_sha256": digest(b"invalid")}),
    )
    with pytest.raises(ValueError, match="Cannot decode"):
        compare_art((first, second), image_index_sha256=pin)


def test_candidate_reader_checks_review_scopes_and_transitivity(
    art_inputs: tuple[ArtInput, ...], tmp_path: Path
) -> None:
    a, b, c = (item.member for item in art_inputs[:3])
    ab = ArtScore(left=a.key, right=b.key, inliers=100)
    ac = ArtScore(left=a.key, right=c.key, inliers=0)
    bc = ArtScore(left=b.key, right=c.key, inliers=40)
    candidate = ArtCandidates(
        opencv_version="synthetic",
        image_index_sha256="sha256:" + "0" * 64,
        members=(a, b, c),
        unavailable=(),
        scores=(ab, ac, bc),
        groups=(ArtGroup(face_id=a.face_id, members=(a.key, b.key, c.key)),),
        review=(
            ArtReview(left=b.key, right=c.key, inliers=40, scope="cross_language"),
        ),
        conflicts=(ac,),
    )
    path = tmp_path / "review.json"
    path.write_bytes(candidate.content())
    assert ArtCandidates.read(path).conflicts == (ac,)
    damaged = candidate.model_copy(update={"conflicts": ()})
    path.write_bytes(damaged.content())
    with pytest.raises(ValueError, match="projections disagree"):
        ArtCandidates.read(path)


@pytest.mark.parametrize(
    ("score", "reviewed"),
    [(9, False), (10, True), (39, True), (40, True), (99, True), (100, False)],
)
def test_candidate_reader_requires_review_at_threshold_boundaries(
    art_inputs: tuple[ArtInput, ...], tmp_path: Path, score: int, *, reviewed: bool
) -> None:
    a, b = (item.member for item in art_inputs[::2][:2])
    pair = ArtScore(left=a.key, right=b.key, inliers=score)
    groups = (
        (ArtGroup(face_id=a.face_id, members=(a.key, b.key)),)
        if score >= 40
        else (
            ArtGroup(face_id=a.face_id, members=(a.key,)),
            ArtGroup(face_id=b.face_id, members=(b.key,)),
        )
    )
    candidate = ArtCandidates(
        opencv_version="synthetic",
        image_index_sha256="sha256:" + "0" * 64,
        members=(a, b),
        unavailable=(),
        scores=(pair,),
        groups=groups,
        review=(
            ArtReview(left=a.key, right=b.key, inliers=score, scope="same_language"),
        )
        if reviewed
        else (),
        conflicts=(),
    )
    path = tmp_path / "review.json"
    path.write_bytes(candidate.content())
    assert bool(ArtCandidates.read(path).review) is reviewed
