"""Optional OpenCV boundary for the fixed illustration comparison recipe."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, cast

import cv2
import numpy as np

if TYPE_CHECKING:
    from numpy.typing import NDArray

MIN_MATCHES = 8
FEATURES = 800
RATIO = 0.75
REPROJECTION_PX = 5.0
NEIGHBOURS = 2


@dataclass(frozen=True)
class Features:
    points: NDArray[np.float32]
    descriptors: NDArray[np.float32] | None


def opencv_version() -> str:
    """Pin cache reuse to the implementation that produced the scores."""
    return cv2.__version__


def features(raw: bytes) -> Features:
    """Extract the session's SIFT recipe from a checked art_s bitmap."""
    cv2.setNumThreads(1)
    image = cv2.imdecode(np.frombuffer(raw, dtype=np.uint8), cv2.IMREAD_GRAYSCALE)
    if image is None:
        raise ValueError("Cannot decode art bitmap")
    detector = cv2.SIFT.create(nfeatures=FEATURES)
    points, raw_descriptors = detector.detectAndCompute(image, None)
    descriptors = cast("NDArray[np.float32] | None", raw_descriptors)
    return Features(
        np.asarray([point.pt for point in points], dtype=np.float32),
        descriptors,
    )


def inliers(left: Features, right: Features) -> int:
    """Count RANSAC support after directional nearest-neighbour ratio filtering."""
    if (
        left.descriptors is None
        or right.descriptors is None
        or len(left.points) < MIN_MATCHES
        or len(right.points) < MIN_MATCHES
    ):
        return 0
    matches = cv2.BFMatcher().knnMatch(
        left.descriptors, right.descriptors, k=NEIGHBOURS
    )
    good = [
        pair[0]
        for pair in matches
        if len(pair) == NEIGHBOURS and pair[0].distance < RATIO * pair[1].distance
    ]
    if len(good) < MIN_MATCHES:
        return 0
    source = np.asarray(
        [left.points[match.queryIdx] for match in good], dtype=np.float32
    ).reshape(-1, 1, 2)
    target = np.asarray(
        [right.points[match.trainIdx] for match in good], dtype=np.float32
    ).reshape(-1, 1, 2)
    _, raw_mask = cv2.findHomography(source, target, cv2.RANSAC, REPROJECTION_PX)
    mask = cast("NDArray[np.uint8] | None", raw_mask)
    return int(mask.sum()) if mask is not None else 0
