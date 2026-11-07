"""Classical card-quad finder (contours, no model), used for offline crops of single-card photos.

find_card(img_rgb, tap=(x, y) or None) -> (quad float32[4,2] in image coords, score) or (None, 0)
rectify(img_rgb, quad) -> uint8 [336, 240, 3] portrait card
"""

import cv2
import numpy as np

W, H = 240, 336
CARD_RATIO = 459 / 641  # short/long


def order(pts: np.ndarray) -> np.ndarray:
    """TL, TR, BR, BL."""
    pts = pts.reshape(4, 2).astype(np.float32)
    c = pts.mean(0)
    ang = np.arctan2(pts[:, 1] - c[1], pts[:, 0] - c[0])
    pts = pts[np.argsort(ang)]  # clockwise from -pi (left/top-left side)
    # rotate so first is the one with smallest x+y
    k = int(np.argmin(pts.sum(1)))
    return np.roll(pts, -k, axis=0)


def _candidates(gray: np.ndarray) -> list[np.ndarray]:
    out = []
    blur = cv2.GaussianBlur(gray, (5, 5), 0)
    for lo, hi in ((30, 90), (60, 160), (15, 50)):
        edges = cv2.Canny(blur, lo, hi)
        edges = cv2.dilate(edges, np.ones((3, 3), np.uint8), iterations=1)
        cnts, _ = cv2.findContours(edges, cv2.RETR_LIST, cv2.CHAIN_APPROX_SIMPLE)
        for c in cnts:
            peri = cv2.arcLength(c, True)
            for eps in (0.02, 0.04):
                ap = cv2.approxPolyDP(c, eps * peri, True)
                if len(ap) == 4 and cv2.isContourConvex(ap):
                    out.append(ap.reshape(4, 2).astype(np.float32))
                    break
    # adaptive threshold route catches cards on busy mats
    th = cv2.adaptiveThreshold(blur, 255, cv2.ADAPTIVE_THRESH_GAUSSIAN_C, cv2.THRESH_BINARY, 51, 5)
    for img in (th, 255 - th):
        cnts, _ = cv2.findContours(img, cv2.RETR_LIST, cv2.CHAIN_APPROX_SIMPLE)
        for c in cnts:
            peri = cv2.arcLength(c, True)
            ap = cv2.approxPolyDP(c, 0.03 * peri, True)
            if len(ap) == 4 and cv2.isContourConvex(ap):
                out.append(ap.reshape(4, 2).astype(np.float32))
    return out


def find_card(img: np.ndarray, tap: tuple[float, float] | None = None) -> tuple[np.ndarray | None, float]:
    h0, w0 = img.shape[:2]
    s = 800 / max(h0, w0)
    small = cv2.resize(img, (int(w0 * s), int(h0 * s)), interpolation=cv2.INTER_AREA)
    gray = cv2.cvtColor(small, cv2.COLOR_RGB2GRAY)
    area_img = small.shape[0] * small.shape[1]
    tap_s = (small.shape[1] / 2, small.shape[0] / 2) if tap is None else (tap[0] * s, tap[1] * s)
    best, best_score = None, 0.0
    for q in _candidates(gray):
        q = order(q)
        area = cv2.contourArea(q)
        if area < 0.04 * area_img or area > 0.98 * area_img:
            continue
        if cv2.pointPolygonTest(q.reshape(-1, 1, 2), tap_s, False) < 0:
            continue
        e = np.linalg.norm(np.roll(q, -1, 0) - q, axis=1)  # top, right, bottom, left
        a, b = (e[0] + e[2]) / 2, (e[1] + e[3]) / 2
        ratio = min(a, b) / max(a, b)
        if not 0.5 < ratio < 0.9:
            continue
        # opposite sides should be similar (perspective allows some taper)
        taper = min(e[0], e[2]) / max(e[0], e[2]) * min(e[1], e[3]) / max(e[1], e[3])
        score = (area / area_img) ** 0.5 * (1 - abs(ratio - CARD_RATIO) * 3) * taper
        if score > best_score:
            best, best_score = q / s, score
    return best, best_score


def rectify(img: np.ndarray, quad: np.ndarray) -> np.ndarray:
    q = order(quad)
    e = np.linalg.norm(np.roll(q, -1, 0) - q, axis=1)
    if (e[0] + e[2]) > (e[1] + e[3]):  # landscape → make long side vertical
        q = np.roll(q, -1, axis=0)
    dst = np.float32([[0, 0], [W, 0], [W, H], [0, H]])
    m = cv2.getPerspectiveTransform(q.astype(np.float32), dst)
    return cv2.warpPerspective(img, m, (W, H), flags=cv2.INTER_AREA)


def squash(img: np.ndarray) -> np.ndarray:
    return cv2.resize(img, (W, H), interpolation=cv2.INTER_AREA)


def center_crop(img: np.ndarray, frac: float = 0.8) -> np.ndarray:
    h, w = img.shape[:2]
    ch = min(h * frac, w * frac / CARD_RATIO)
    cw = ch * CARD_RATIO
    y0, x0 = int((h - ch) / 2), int((w - cw) / 2)
    return squash(img[y0 : y0 + int(ch), x0 : x0 + int(cw)])
