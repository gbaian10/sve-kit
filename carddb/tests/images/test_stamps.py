"""Synthetic marks exercise alignment, edge logos and faint mid-art serial text."""

import io
import json
import random
from typing import TYPE_CHECKING

import pytest
from PIL import Image, ImageDraw

from sve_carddb.images.stamps import (
    Detection,
    MarkBox,
    MarkGroups,
    bitmap,
    detect_marks,
    mark_pattern,
    match_patterns,
)
from sve_carddb.workflows.stamps import write_candidates

if TYPE_CHECKING:
    from pathlib import Path


def raw(image: Image.Image) -> bytes:
    stream = io.BytesIO()
    image.save(stream, "PNG")
    return stream.getvalue()


def texture() -> Image.Image:
    rng = random.Random(304)  # ruff: ignore[suspicious-non-cryptographic-random-usage] -- deterministic authored texture
    image = Image.new("RGB", (459, 641), (100, 100, 100))
    draw = ImageDraw.Draw(image)
    for _ in range(400):
        x, y = rng.randrange(55, 410), rng.randrange(65, 390)
        shade = rng.randrange(30, 180)
        draw.ellipse((x, y, x + 8, y + 8), fill=(shade,) * 3)
    return image


def overlaps(result: Detection, box: tuple[int, int, int, int]) -> bool:
    left, top, right, bottom = box
    return any(
        mark.x < right
        and mark.x + mark.width > left
        and mark.y < bottom
        and mark.y + mark.height > top
        for mark in result.boxes
    )


def test_faint_serial_below_an_ornament_survives_alignment() -> None:
    reference = texture()
    target = reference.copy()
    draw = ImageDraw.Draw(target)
    draw.line((240, 360, 410, 360), fill=(220, 200, 110), width=3)
    draw.text((350, 400), "/150", fill=(127, 122, 110), stroke_width=0)
    result = detect_marks(raw(target), (raw(reference),))
    assert result.reference_index == 0
    assert result.inliers >= 10
    assert overlaps(result, (350, 400, 374, 410))
    assert any(mark.low_contrast and mark.y >= 390 for mark in result.boxes)


def test_shifted_reference_preserves_tall_anniversary_and_left_mark() -> None:
    reference = texture()
    target = Image.new("RGB", reference.size, (100, 100, 100))
    target.paste(reference, (4, 3))
    draw = ImageDraw.Draw(target)
    draw.rectangle((355, 275, 417, 390), fill="orange")
    draw.rectangle((48, 290, 95, 325), fill="blue")
    result = detect_marks(raw(target), (raw(reference),))
    assert overlaps(result, (355, 275, 417, 390))
    assert overlaps(result, (48, 290, 95, 325))


def test_shared_plaque_and_blank_image_do_not_prove_absence() -> None:
    image = texture()
    assert detect_marks(raw(image), (raw(image),)).boxes == ()
    fallback = detect_marks(raw(Image.new("RGB", image.size, "black")))
    assert fallback.reference_index is None
    assert fallback.boxes == ()


def test_fallback_keeps_pale_gold_outside_right_corner() -> None:
    image = Image.new("RGB", (459, 641), "black")
    ImageDraw.Draw(image).rectangle((80, 260, 125, 275), fill=(190, 180, 150))
    result = detect_marks(raw(image))
    assert result.reference_index is None
    assert overlaps(result, (80, 260, 125, 275))


def test_invalid_bitmap_is_rejected() -> None:
    with pytest.raises(ValueError, match="Cannot decode"):
        detect_marks(b"not an image")


@pytest.mark.parametrize("colour", [(210, 180, 60), (180, 180, 180)])
def test_runtime_ornament_matches_gold_and_silver_away_from_right_corner(
    colour: tuple[int, int, int],
) -> None:
    example = Image.new("RGB", (459, 641), "black")
    ornament = [(80, 200), (100, 207), (115, 193), (130, 210), (160, 200)]
    ImageDraw.Draw(example).line(ornament, fill=(210, 180, 60), width=3)
    pattern = mark_pattern(raw(example), MarkBox(75, 188, 90, 28), label="serial")
    target = Image.new("RGB", example.size, "black")
    ImageDraw.Draw(target).line(
        [(x + 40, y + 100) for x, y in ornament], fill=colour, width=3
    )
    matches = match_patterns(raw(target), (pattern,))
    assert len(matches) == 1
    assert matches[0].label == "serial"
    assert matches[0].box.x < 140
    assert 280 < matches[0].box.y < 320


def test_shape_groups_collect_repeated_local_marks() -> None:
    image = Image.new("RGB", (459, 641), "black")
    draw = ImageDraw.Draw(image)
    draw.rectangle((90, 200, 125, 230), outline="white", width=3)
    draw.rectangle((230, 350, 265, 380), outline="white", width=3)
    groups = MarkGroups()
    normalized = bitmap(raw(image))
    assert groups.add(normalized, MarkBox(80, 190, 55, 50)) == groups.add(
        normalized, MarkBox(220, 340, 55, 50)
    )


def test_uninformative_and_outside_patterns_are_rejected() -> None:
    image = raw(Image.new("RGB", (459, 641), "black"))
    with pytest.raises(ValueError, match="outside"):
        mark_pattern(image, MarkBox(-1, 0, 10, 10), label="serial")
    with pytest.raises(ValueError, match="visible"):
        mark_pattern(image, MarkBox(10, 10, 30, 30), label="serial")


def test_candidate_json_keeps_float_similarity(tmp_path: Path) -> None:
    write_candidates(
        tmp_path, "pr", [{"pattern_matches": [{"label": "serial", "score": 0.71}]}]
    )
    saved = json.loads((tmp_path / "candidates.json").read_text(encoding="utf-8"))
    assert saved["review_required"] is True
    assert saved["cards"][0]["pattern_matches"][0]["score"] == pytest.approx(0.71)
