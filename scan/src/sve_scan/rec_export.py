"""Recogniser checkpoint -> the phone model: fp32 ONNX, then weights-only int8 (compute stays float).

Static int8 (activations quantised too) dropped real-photo top-1 from 91% to 69%, so only weights are quantised.
Input float32 [N,3,312,224] RGB in 0..1 (normalisation is inside the graph); output [N,256], L2-normalised.

usage: python -m sve_scan rec-export <arch> <tag>   ->  SVE_SCAN_WORK/<tag>.fp32.onnx, <tag>.w8.onnx
"""

import sys
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
import torch
import torch.nn as nn
import torch.nn.functional as F
from onnx import helper, numpy_helper

from sve_scan import paths
from sve_scan import rec_train as T


class Wrapped(nn.Module):
    """The embedder with ImageNet normalisation inside, so the client feeds plain 0..1 RGB."""

    def __init__(self, m: nn.Module):
        super().__init__()
        self.m = m
        self.register_buffer("mean", torch.tensor((0.485, 0.456, 0.406)).view(1, 3, 1, 1))
        self.register_buffer("std", torch.tensor((0.229, 0.224, 0.225)).view(1, 3, 1, 1))

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return self.m((x - self.mean) / self.std)


def prep(arr: np.ndarray) -> np.ndarray:
    """uint8 [N,H,W,3] crops -> the model input (same resize as training)."""
    x = torch.from_numpy(np.ascontiguousarray(arr)).permute(0, 3, 1, 2).float() / 255
    x = F.interpolate(x, size=(T.IN_H, T.IN_W), mode="bilinear", antialias=True, align_corners=False)
    return x.numpy()


def export_fp32(arch: str, tag: str) -> Path:
    out = paths.work() / f"{tag}.fp32.onnx"
    m = T.Embedder(arch)
    m.load_state_dict(torch.load(paths.work() / f"model_{tag}.pt", map_location="cpu"))
    torch.onnx.export(
        Wrapped(m).eval(),
        torch.rand(1, 3, T.IN_H, T.IN_W),
        str(out),
        input_names=["image"],
        output_names=["embedding"],
        dynamic_axes={"image": {0: "n"}, "embedding": {0: "n"}},
        opset_version=17,
        dynamo=False,
    )
    return out


def weight_only_int8(src: Path, dst: Path) -> None:
    """Store Conv/Gemm/MatMul weights as per-output-channel int8 + DequantizeLinear; compute stays float."""
    model = onnx.load(str(src))
    g = model.graph
    inits = {i.name: i for i in g.initializer}
    targets = set()
    for node in g.node:
        if node.op_type in ("Conv", "Gemm", "MatMul") and len(node.input) > 1 and node.input[1] in inits:
            targets.add(node.input[1])
    new_nodes = []
    for name in sorted(targets):
        w = numpy_helper.to_array(inits[name]).astype(np.float32)
        if w.ndim < 2 or w.size < 1024:
            continue
        flat = np.abs(w.reshape(w.shape[0], -1)).max(1)
        scale = np.where(flat > 0, flat / 127.0, 1.0).astype(np.float32)
        shape = [-1] + [1] * (w.ndim - 1)
        q = np.clip(np.round(w / scale.reshape(shape)), -127, 127).astype(np.int8)
        g.initializer.remove(inits[name])
        g.initializer.extend([numpy_helper.from_array(q, name + "_q"), numpy_helper.from_array(scale, name + "_s")])
        new_nodes.append(
            helper.make_node("DequantizeLinear", [name + "_q", name + "_s"], [name], axis=0, name=name + "_dq")
        )
    for n in reversed(new_nodes):
        g.node.insert(0, n)
    onnx.save(model, str(dst))


def main() -> None:
    arch, tag = sys.argv[1], sys.argv[2]
    f32 = export_fp32(arch, tag)
    w8 = paths.work() / f"{tag}.w8.onnx"
    weight_only_int8(f32, w8)
    # parity: the int8 model must embed like the float one
    gallery = np.load(paths.work() / "gallery.npy", mmap_mode="r")
    x = prep(gallery[:: max(1, len(gallery) // 64)][:64])
    a, b = (
        ort.InferenceSession(str(p), providers=["CPUExecutionProvider"]).run(None, {"image": x})[0] for p in (f32, w8)
    )
    cos = (a * b).sum(1)
    print(f"{w8.name}: {w8.stat().st_size / 2**20:.2f} MiB, cosine to fp32 min {cos.min():.4f} mean {cos.mean():.4f}")


if __name__ == "__main__":
    main()
