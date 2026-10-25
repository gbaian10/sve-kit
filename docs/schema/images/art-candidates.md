# 插畫同圖候選

`images art-candidates` 只產生可供人工審閱的同圖候選，不寫入 authored、
建置資料庫、圖片 producer 狀態或公開快照。採納後仍依
[身分登錄契約](../domains/authored-layout.md)記錄精確成員與 observation；
同圖不能推導換框、CV 簽名、活動標誌或 base／alternate。

## 指令與輸入

安裝 carddb 的 `art-matching` extra 後執行：

```bash
sve-carddb images art-candidates \
  --image-index /absolute/input/image-index/<sha256>.json \
  --image-root /absolute/input/public \
  --authored /absolute/input/authored \
  --output /absolute/output/art-candidates.json
```

所有路徑必須明示為絕對路徑。輸出不能位於圖片或 authored 輸入內，
也不能覆寫 image index 或 `--previous` 輸入。

- image index 必須是[圖片 producer 的不可變索引](image-index.md)，驗 canonical
  bytes、內容位址檔名及表間引用。
- authored 提供完整已驗證的 printing／face／整數編號與來源面對應。
  卡號、語言及 card_id 從登錄取得，不以日英卡號猜配對。
- `image-root` 下讀取 `images/art_s/<int_id>[-f<ordinal>].webp`，核對索引內的
  byte 數，並將實際 bytes 的 SHA-256 寫入候選。缺檔或 byte 數不符即失敗；
  索引中本來未公開或不可用的圖片列入 `unavailable`。
- 全程不開 live manifest、不抓網路。OpenCV 延遲載入，其他 CLI 指令不需要它。

## 比對與候選格式

recipe 固定為 `sift800-ratio075-ransac5-v1`：灰階解碼、SIFT `nfeatures=800`，
BFMatcher 兩個最近鄰、ratio 0.75，至少八個關鍵點及八個通過 ratio 的配對，
再以 RANSAC homography 的 5 px 門檻計算 inlier 數。條件不足或無 homography
時分數為零。OpenCV 每次只用一個內部執行緒。

只比較同一 `face_id` 的不同版次，依 `(int_id, ordinal)` 固定方向與順序。
inlier 至少 40 的邊以 union-find 合組；同組內任何直接分數小於 10 的邊列為
傳遞衝突，不能因間接連通就忽略不同圖證據。

候選為 canonical JSON，`format: sve-art-candidates-v1`，包含：

| 欄位 | 內容 |
| --- | --- |
| `recipe`、`opencv_version` | 演算法與實作版本 |
| `image_index_sha256` | 本次 producer 索引 bytes 的 hash |
| `members` | printing／face／card、地區、原樣卡號、整數編號、面 ordinal、圖片 hash |
| `unavailable` | 索引中未公開或不可用的 printing＋face 鍵 |
| `scores` | 同面完整兩兩分數，`left/right/inliers` |
| `groups` | 每組的 face_id 與精確成員，包含單成員組 |
| `review` | 10～99 點的配對；`scope` 區分 `same_language` 與 `cross_language` |
| `conflicts` | 同組但直接分數小於 10 的配對 |

成員鍵是 `printing_id/face_id`，不是候選組號或卡號；輸出不含本機路徑、
官方卡文或圖片 bytes。`cross_language` 讓審閱者區分只跨語言的不確定配對，
不改動分數或分組。人工審閱範圍由採納批次明示，候選本身沒有人工確認狀態。

## 增量

對包含舊成員與新卡包的完整 image index 加上
`--previous /absolute/input/previous-art-candidates.json`。工具先重驗舊候選的
完整同面配對、重複成員、分組、待確認清單與衝突投影，再核對所有舊成員
的身分、實際圖片 hash 與 OpenCV 版本。

未變動的舊對舊分數直接沿用；只新增同面內含新成員的配對，包括同批新對新。
新面只有一個成員時不計分。重新以完整分數合組，不直接拼接舊組號，因此與
同一完整輸入的全量結果一致。stdout 分別回報 `compared_pairs` 與 `reused_pairs`。

舊成員被移除、換圖、變更身分或 OpenCV 版本不同時拒絕沿用，移除
`--previous` 後全量重跑。這不是來源更新或人工決定續版工具；人工決定應另外
釘住受審候選及精確成員，不改寫候選分數。
