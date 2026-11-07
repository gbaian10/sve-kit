# 拍卡產物格式（`format: 1`）

拍卡用的兩個模型與卡片索引，由 `scan/` 產生（`python -m sve_scan release`），發佈到 R2 的 `scan/`，
`sim/web` 只讀這份約定。讀寫實作在 `scan/src/sve_scan/artifacts.py`，測試在 `scan/tests/test_artifacts.py`。

## 版本與身分

- **找卡模型**與**辨識模型**各自有一個整數版本，從 1 起；畫面上顯示 `det-1`、`rec-1`。已發佈的版本內容不再變，換模型就加 1
- **卡片索引**的身分＝辨識模型版本＋卡面版本（`jp`／`en`）＋卡表的 `data_version`。換辨識模型一定重建索引；同一個辨識模型遇到新卡表，只重建索引
- 實驗時的內部代號不出現在產物、路徑或畫面

## 發佈位置

| 路徑 | 內容 | 快取 |
| --- | --- | --- |
| `scan/manifest.json` | 清單檔（下節） | 短 |
| `scan/models/<sha256>.onnx` | 模型 | 永久（檔名就是內容雜湊） |
| `scan/index/<sha256>.{i8,scale,hub,json}` | 索引的四個檔 | 永久 |

## 清單檔

```json
{
  "format": 1,
  "det": {
    "version": 1, "file": "models/<sha256>.onnx", "bytes": 1130099, "sha256": "<sha256>",
    "input": 256, "stride": 8, "grow": 1.0
  },
  "rec": {
    "version": 1, "file": "models/<sha256>.onnx", "bytes": 4707342, "sha256": "<sha256>",
    "input": [312, 224], "dim": 256,
    "scoring": { "hubWeight": 0.75, "accept": 0.5, "margin": 0.03 }
  },
  "index": {
    "jp": {
      "rec": 1, "data_version": "<卡表 data_version>", "count": 7600,
      "vectors": { "file": "index/<sha256>.i8", "bytes": 0, "sha256": "<sha256>" },
      "scale": { "file": "index/<sha256>.scale", "bytes": 0, "sha256": "<sha256>" },
      "hub": { "file": "index/<sha256>.hub", "bytes": 0, "sha256": "<sha256>" },
      "cards": { "file": "index/<sha256>.json", "bytes": 0, "sha256": "<sha256>" }
    },
    "en": { "…": "同上" }
  }
}
```

- `format` 代表下表的約定（輸入大小、正規化、維度、int8 存法、四角的解碼與排序、卡片欄位）；約定一改就加 1。讀不懂的 `format` 要顯示「需要更新」，不能照猜的格式讀
- `bytes` 是存放大小；傳輸壓縮交給 CDN
- `grow`：找卡框的放大倍數，跟著找卡模型走
- `scoring`：辨識分數的校準值，跟著辨識模型走
  - 分數 = `dot(q_i, e) × scale[i] − hubWeight × hub[i]`
  - 第一名的原始相似度 ≥ `accept`，而且分數比第二張卡多 ≥ `margin`，才算認出
- 執行引擎（ONNX Runtime 的 wasm）不屬於這裡：不發佈到 R2。前端建置時從 node_modules 算出 WebGPU／CPU 兩個 wasm 的大小與 SHA-256，下載時從公開 CDN（jsDelivr，失敗改 unpkg）取得，並與建置時的雜湊比對，不符就拒用
- 索引依卡面版本分開，只下載使用者正在用的那一份

## 檔案內容

| 項目 | 內容 |
| --- | --- |
| 找卡模型 | 權重 int8 的 ONNX（計算仍是浮點）。輸入 `image` float32 `[1,3,256,256]`，RGB 0–1。輸出 `heat` `[1,1,32,32]`，以及 `corners` `[1,8,32,32]`＝四角相對格中心的偏移（單位：格，stride 8）。四角是順時針，**起點不固定**，解碼後一律排序（下節） |
| 辨識模型 | 權重 int8 的 ONNX。輸入 `image` float32 `[N,3,312,224]`，RGB 0–1（正規化在計算圖內）。輸出 `embedding` `[N,256]`，已 L2 正規化 |
| `.i8` | int8，`count × 256`，列優先。第 i 列 × `scale[i]` ≈ 第 i 張卡的單位向量 |
| `.scale` | float32 little-endian，`count` 個，`scale[i] = max(abs(v_i)) / 127` |
| `.hub` | float32 little-endian，`count` 個，hub 校正值 |
| `.json` | 長度 `count` 的陣列，順序與向量列相同 |

### 找卡框的解碼與排序

heat 上大於門檻、且是 3×3 鄰域最大值的格子 `(gx, gy)` 是一張卡。它的四角（輸入的像素座標）：

```text
corner k = (corners[2k, gy, gx] + gx + 0.5, corners[2k+1, gy, gx] + gy + 0.5) × stride
```

模型訓練時不固定起點，四個角只保證順時針。用之前一律這樣排序：

1. 以四角的平均點為中心，依 `atan2(y − cy, x − cx)` 由小到大排（畫面 y 向下，所以是順時針）
2. 從 `x + y` 最小的那個角（左上）開始

排完就是畫面上的左上、右上、右下、左下。`grow` 不是 1 時，以平均點為中心放大四角。
參考實作是 `artifacts.decode_quad` 與 `artifacts.order_corners`。

`.json` 每列的欄位（固定這幾個，依序）：

| 欄位 | 內容 |
| --- | --- |
| `no` | 卡號（照官網原樣） |
| `c` | card_id（日英版次共用；同一張卡的不同版次分數接近，以 card_id 判定認對） |
| `n` | 版次的永久整數編號（卡圖 `images/<size>/<n>.webp`） |
| `f` | 卡面序號；0 是正面，其他面的卡圖是 `<n>-f<f>.webp` |
| `nm` | 卡名原文 |
| `a` | 異圖群組（art_id） |
| `k` | 職業代碼（中立是 `neutral`，沒有職業是空字串） |
| `y` | 卡片種類代碼 |
