# 插畫裁切覆寫契約

本文件定義來源綁定的覆寫採納輸入 **`image_crop_format: 2`**，沿用[卡圖衍生檔契約](image-variants.md)的整數框、五檔 WebP 與既有 `CropOverride`，本來源採納格式本身不新增影像表或快照欄位；公開 2.0 投影另依 snapshot-format §2.1。`jp` 與 `en` 共用 loader、產圖與建置驗證契約；覆寫是否已套用須由完整輸入閉包與建置報告核對。

## 1. 鍵與兩種 image ID

覆寫鍵為 `(source_key,source_sha256)`。`source_key` 取凍結 descriptor 的既有資源鍵（provider、kind、原樣 URL 的 canonical hash），不得從卡號組網址。`source_sha256` 是來源 raw bytes 的 SHA-256，與產圖 API 一致，使用不含 `sha256:` 前綴的 64 碼小寫 hex。不是 RGB 像素 hash、公開 WebP hash 或清單 hash。

永久 card／face 不作覆寫鍵：同卡的不同版次、異圖、地區可以有不同版型。卡片身分修復也不得把覆寫搬到別的來源。身分只用於 §3 的重印診斷。

同名的 `image_id` 有兩種用途，不能互換：

| 用途 | 形式與輸入 | 是否寫入 authored 覆寫列 |
| --- | --- | --- |
| 產圖 API 比對覆寫 | `img:v1:`，由凍結來源版本 ID 推導 | 否，由 loader 計算 |
| 建置 DB／公開 `image_asset.id` | `img:binding:`，由來源 ID 與 HTML 原樣 `<img src>` 推導 | 否，仍由頁面 binding 產生 |

以下 `canonical` 固定為物件鍵排序、UTF-8、不 ASCII escape、分隔符 `,`／`:`、無額外空白或尾端換行、不正規化 Unicode；`hex_sha256` 回傳 64 碼小寫 hex。沿用[來源歸檔契約](source-archive.md)的公式：

```text
source_version_id = "src:v1:" + hex_sha256(canonical({
  "source_key": source_key,
  "raw_sha256": "sha256:" + source_sha256
}))
conversion_image_id = "img:v1:" + hex_sha256(canonical({
  "source_id": source_version_id
}))
binding_image_id = "img:binding:" + hex_sha256(canonical({
  "source_id": image_source_version_id,
  "src": source_src_raw
}))
```

loader 由覆寫列的兩個鍵值算出 `conversion_image_id`，選中凍結來源時再核對 descriptor 的來源版本 ID 與 raw hash，組成既有 `CropOverride`。公開 binding 的 `src` 是頁面 attribute 原值，不是 resolved URL；缺少圖片來源的 binding 以頁面來源版本 ID 代替公式的 `image_source_version_id`，並依既有規則保持 pending、不產 variants。authored 不抄寫這兩種 ID。

## 2. 目錄、分片與覆寫列

覆寫分片放在 `authored/image-crops/<filing_key>/<sequence>.yaml`。`filing_key` 是歸檔標籤（例如 BP10、BP15），不是選框依據。新資料追加下一個序號，不重排既有分片。每檔遵守 [authored 共同格式](authored-layout.md#1-路徑與共同格式)的 YAML 邊界與小於 1,048,576 bytes 上限；重用既有嚴格 YAML reader，不另做寬鬆 parser。

**不設 index.yaml。** loader 在釘住的完整 authored Git revision 下，載入 `image-crops/` 中全部分片 `.yaml`，以相對路徑排序；不接受符號連結或越界路徑。分片位置與封套 kind 必須相符，未知格式／kind／欄位拒絕。全體檔案一併驗證，不因本次僅建 JP 而跳過 EN 列。完整檔案集合與 exact bytes 依 §3 進 F1，缺檔或多檔不得當成正常空集合。

目前只讀取格式 2；未知格式拒絕，現有分片直接維護為此格式。

覆寫分片封套恰有 `image_crop_format: 2`、`kind: crop_override_shard`、非空 `records` 陣列。每列恰有下列欄位：

| 欄位 | 型別與用途 |
| --- | --- |
| source_key | `sha256:<64 小寫 hex>`；descriptor 的資源鍵 |
| source_sha256 | 64 碼小寫 hex；raw bytes pin |
| left、top | 真整數，非 bool／float，≥0；套用 EXIF 方向後的像素座標 |
| width、height | 真整數，非 bool／float，>0，`3*width == 4*height` |
| reason | 非空白字串，說明為何不用標準框 |
| region | `jp` 或 `en`，不具權威的審閱標註 |
| card_no | 非空白的原樣卡號，不具權威的審閱標註 |

實際半開框為 `[left,top,left+width,top+height)`；對選中的來源須滿足 `left+width≤W`、`top+height≤H`，W/H 從已驗證 raw 與既有方向處理取得。覆寫只選位置，不新增旋轉／翻轉選項，不改既有 EXIF 方向處理。

全體分片的 `(source_key,source_sha256)` 必須唯一，重複即拒絕，即使框完全相同亦然。同一 source_key 可保留多個採納的歷史 hash；每個鍵只有一個有效框，要改同一來源版本的框時直接修改該列，舊內容仍由 Git 歷史保留。每次新增列按 `(region,card_no,source_key,source_sha256)` 排序，不重排舊檔。

region／card_no 不參與來源查找、hash 推導、跨區配對或採納判定；已驗證 binding 與標註不符時只報告，不能憑標註換用另一張來源圖。只建 JP 時仍驗 EN 列的結構、鍵及推導 ID；未選中的來源不宣稱已做 decode／框內驗證。

## 3. 解析、建置驗證與報告

選框解析是共用、不分地區的函式，輸入已驗證的凍結 descriptor（kind=image）、來源 bytes 與完整採納集合：

1. 全體沒有該 source_key：依既有直向／橫向整數公式用預設框。
2. 有該 source_key，但新 raw hash 沒有有效採納：停止本批產圖，錯誤列出 source_key、新 hash 與已有 hash；不套舊框、不退回預設。同網址換圖且新版改回標準版型時，仍須新增該 hash 的採納列，座標填依新版尺寸算出的預設框。
3. 有完全相符的鍵：核對來源版本與框內約束，推導產圖 ID 並傳入既有 `build_variants(override=…)`。任何失敗均停止，不跳過該列充作完成。

先完整驗證採納集合，再大量轉檔；未選中的列列為未使用，不是錯誤。`build_regional_assets(region=…)` 明示 `jp` 或 `en`，只接受相符地區的 image batch；`build_jp_assets` 保留 JP 限制。頁面綁定使用該地區萃取器的原始 `<img src>` 與人工 source_face_map。

日英 `snapshot export-offline` 的 Inputs.sources 分別釘兩區 card／image batch 與 parser version，成對提供 `--image-assets-dir`、`--image-cache-dir` 才納入影像。入口以 `reuse_only` 驗證完整五檔快取，缺檔即失敗；產圖由上述 API 先完成。建置須重驗各區目前批次的完整成員與全體裁切採納，影像綁定及來源使用閉包同時進入 staging／sealed DB 重播；詳細參數見[離線建置入口](../../carddb/src/sve_carddb/snapshot/OFFLINE.md)。正式發布仍依既有來源涵蓋與 readiness 門檻。

首次資料採納包含 JP 六列與 EN 六列，共 12 筆來源綁定的覆寫列。JP-only 建置會報告未用到的 EN 六列；日英建置依實際選中的來源判定各列是否套用。

F1 依[建置輸入紀錄](source-archive.md#221-建置輸入紀錄與完整使用閉包)釘完整 authored revision、所有覆寫分片的排序相對路徑和 exact bytes hash。`configuration.image_crop_overrides` 保存 `{image_crop_format,authored_revision,files}`，image_crop_format 為整數 `2`，revision 為完整 40 碼 Git SHA，files 為按 name 排序的 `{name,sha256}` 清單，name 是 checkout 相對路徑（`authored/image-crops/...`），sha256 為 `sha256:<64 小寫 hex>`；內容須與 dependencies 與該 revision 的目錄集合相符。呼叫端 pin 只作複核，不能代替 authored 覆寫列；無目錄的 revision 是明示的空集合，不能把本應存在的檔案遺失當空集合。

只有帶影像的建置納入上述裁切 dependencies／configuration，與 `image_recipe` 一致；純文字建置不因裁切資料變動而換 input fingerprint。`build()` 消費外部傳入的 `ImageBuild` 時，必須從自身釘住的採納輸入重算每個來源應用的框（覆寫或預設），與 `VariantSet.crop_box` 比對後才可填 DB／輸出公開清單。僅在 `build_regional_assets` 產圖側驗證不夠；來源、recipe、五檔 hash 都有效但仍使用舊框的結果也必須拒絕。

建置報告不含卡片原文或私人路徑，至少列出：

- 套用覆寫的來源圖張數（按覆寫鍵去重，不把同圖多個 binding 重算）。
- 未被本次選中來源用到的覆寫列：鍵與 region／card_no 標註，包含 JP-only 建置未選中的 EN 列。
- 標註與已驗證 binding 不符的列。
- **不阻擋建置的重印診斷**：同一有效永久 card／face，在其他版次的已驗證來源有覆寫，而本版次的來源沒有覆寫時，列永久 ID、版次 ID 與來源鍵。只用已採納身分及已驗證 binding，不靠標註猜對應；未建 EN 的 binding 不假裝已查。這項診斷不自動繼承框，不將其他來源升格為已核可。

## 4. 目前核可的精確框與掃描限制

**使用者 2026-10-03 核可**的倒吊版型只有以下 12 張，全部來源為 459×641。保留倒吊、不另旋轉；left=36、width=384、height=288 固定：

| JP 卡號 | EN 卡號 | top | 半開框 |
| --- | --- | --- | --- |
| BP10-001、BP10-002、BP10-SL01、BP10-SL02 | BP10-001EN、BP10-002EN、BP10-SL01EN、BP10-SL02EN | 184 | `[36,184,420,472)` |
| BP10-U01 | BP10-U01EN | 232 | `[36,232,420,520)` |
| BP15-PR01 | BP15-PR01EN | 256 | `[36,256,420,544)` |

此表是已核可框的說明，不是卡號選框表，也不推導日英身分關聯；有效資料仍須有每張凍結來源的鍵、raw hash 與框。最後兩張為主戰者圖，與十張上置效果文字框的卡圖分開計數。

調查對凍結的 JP 7,383 張與 EN 7,437 張卡圖驗 hash／decode，再以字形及水平底帶判定篩選標準框附近的文字，補掃中下段。**程式篩選後由模型看縮圖總覽**；不是使用者逐張複核全圖庫。調查支持這批來源中倒吊版型為唯一須下移的一組，使用者另看過並同意上述 12 張實際大小的最終 RGB 裁切結果。畫內標誌／簽名的 13 張已決定不處理、保持標準框，不列待決事項。

本次決定只採納上表 12 張來源的精確框；掃描僅供尋找候選，不作全圖庫無漏檢的證明，也不作自動選框規則。
未見版型、淡底帶或淺底深字仍可能漏檢；日後發現其他例外仍須按來源採納，不擴大本次同意。

## 5. 變體、快照與上傳的版本策略

既有 recipe 已支援來源綁定的整數覆寫，新增採納資料不改 recipe_version；解碼、編碼、縮放或框演算法改變才按既有規則換 recipe。快取 key 包含來源 hash、實際框與完整 recipe：新框取得新 key，`reuse_only` 若沒有新框結果必須失敗，不得借用舊框的完整五檔。

新 key 下五檔會重新產製。在來源與 recipe 不變時，card 三檔輸入不變，內容定址後仍可共用原 blob；art 兩檔改為新 WebP hash／path。不改寫內容定址的建置 blob。`img:binding:` 不依賴裁切，來源／頁面 binding 不變時保持原值。2.0 的公開 key 固定、只換 art_version 並覆寫有變動的 art bytes，card 三檔版本沿用；欄序依 snapshot-format §2.1，不公開框。

採納資料與 F1 的變動產生新的影像建置輸入指紋，須重產受影響的預覽、影像分片、清單、壓縮旁檔與 2.0 發布凍結包，不拿舊清單充作已完成覆寫。preview 只供本機使用；R2 僅透過 `r2 upload-v2` 發布 2.0 凍結包。發布須分開驗 current＋previous 的 metadata 聯集及僅 current 的圖片集合；previous 圖片引用不保留舊 WebP，不要求同 key 符合兩版輸出。新圖驗妥才切快照，再清理失去 current 引用的 key，合法在途 staging 另計；使用隔離 preview 根，不混入任意無引用檔案。

資料入庫後先完成兩檔 art 的實際 WebP 檢視，再重產 2.0 發布凍結包並執行離線 dry-run；真正上傳 R2／公開仍需使用者另行同意。來源 raw、來源歸檔證據與必要發布收據依原契約保存，公開快照／WebP 依有限保留策略，不因新框已發布而刪除。
