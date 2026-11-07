"""Find every card quad in a large deck-spread photo: run the 256px corner detector on overlapping tiles at
several scales, map to full-frame px, keep untruncated plausible quads, NMS by IoU (Python twin of the web
client's deck reader, used for labelling and offline checks; the web client stays the reference).

Models are SVE_SCAN_WORK/det_<tag>.w8.onnx.

usage: python -m sve_scan tiledet <tag> <photo> <overlay.jpg>"""

import sys

import cv2
import numpy as np
import onnxruntime as ort

from sve_scan import artifacts, paths

S, STRIDE = 256, 8
_sess = {}


def sess(model):
    if model not in _sess:
        o = ort.SessionOptions()
        o.intra_op_num_threads = 3
        _sess[model] = ort.InferenceSession(
            str(paths.work() / f"det_{model}.w8.onnx"), o, providers=["CPUExecutionProvider"]
        )
    return _sess[model]


order = artifacts.order_corners


def plausible(q):
    if not np.all(np.isfinite(q)) or not cv2.isContourConvex(q.reshape(-1, 1, 2).astype(np.float32)):
        return False
    e = np.linalg.norm(np.roll(q, -1, 0) - q, axis=1)
    a, b = (e[0] + e[2]) / 2, (e[1] + e[3]) / 2
    return 0.45 < min(a, b) / max(a, b) < 0.95 and min(e) > 12


def iou(a, b):
    inter, _ = cv2.intersectConvexConvex(a.astype(np.float32), b.astype(np.float32))
    u = cv2.contourArea(a.astype(np.float32)) + cv2.contourArea(b.astype(np.float32)) - inter
    return inter / max(u, 1e-6)


def detect(rgb, model, scales=(0.22, 0.32, 0.45, 0.65, 1.0), thr=0.35, area="global"):
    """area="local": compare each quad with its 6 nearest neighbours instead of the photo median, so near cards
    of an oblique shot (several times the far ones) are not dropped as merged neighbours."""
    h, w = rgb.shape[:2]
    m = min(h, w)
    dets = []
    for s in scales:
        t = int(m * s)
        st = max(1, t // 2)
        xs = list(range(0, max(1, w - t) + 1, st))
        ys = list(range(0, max(1, h - t) + 1, st))
        if xs[-1] + t < w:
            xs.append(w - t)
        if ys[-1] + t < h:
            ys.append(h - t)
        tiles, offs = [], []
        for y0 in ys:
            for x0 in xs:
                tiles.append(cv2.resize(rgb[y0 : y0 + t, x0 : x0 + t], (S, S), interpolation=cv2.INTER_AREA))
                offs.append((x0, y0))
        x = np.stack(tiles).astype(np.float32).transpose(0, 3, 1, 2) / 255
        for i in range(len(x)):
            heat, co = sess(model).run(None, {"image": x[i : i + 1]})
            hm = heat[0, 0]
            mx = cv2.dilate(hm, np.ones((3, 3), np.uint8))
            for yy, xx in zip(*np.nonzero((hm == mx) & (hm > thr))):
                q = artifacts.decode_quad(co[0], xx, yy, STRIDE)  # tile-input px
                margin = 3
                if q.min() < margin or q.max() > S - margin:
                    continue  # touches the tile edge: probably truncated
                q = q * (t / S) + offs[i]
                q = order(q)
                if plausible(q):
                    dets.append((float(hm[yy, xx]), q, s))
    dets.sort(key=lambda d: -d[0])
    keep = []
    for d in dets:
        if all(iou(d[1], k[1]) < 0.35 for k in keep):
            keep.append(d)
    # drop quads much bigger than the median card (merged neighbours)
    if len(keep) >= 5:
        ar = np.array([cv2.contourArea(k[1].astype(np.float32)) for k in keep])
        if area == "local":
            c = np.array([k[1].mean(0) for k in keep])
            d = np.linalg.norm(c[:, None] - c[None], axis=2)
            ref = np.array([np.median(ar[np.argsort(d[i])[1:7]]) for i in range(len(keep))])
        else:
            ref = np.full(len(keep), np.median(ar))
        keep = [k for k, a, m in zip(keep, ar, ref) if 0.35 * m < a < 2.2 * m]
    return keep


if __name__ == "__main__":
    cv2.setNumThreads(3)
    model, src, dst = sys.argv[1], sys.argv[2], sys.argv[3]
    bgr = cv2.imread(src)
    rgb = cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB)
    ks = detect(rgb, model)
    for _sc, q, _s in ks:
        cv2.polylines(bgr, [q.astype(np.int32)], True, (0, 255, 0), 3)
    cv2.imwrite(dst, cv2.resize(bgr, None, fx=0.6, fy=0.6))
    import collections

    print(len(ks), "cards", collections.Counter(k[2] for k in ks))
