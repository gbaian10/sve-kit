# 插畫裁切覆寫契約

本文件定義來源綁定的插畫裁切覆寫 `authored/images/crops.yaml`，沿用[卡圖衍生檔契約](image-variants.md)的整數框與五檔 WebP，不新增影像表或快照欄位；公開 2.0 投影另依 snapshot-format §2.1。`jp` 與 `en` 共用 loader、產圖與建置驗證。

## 1. 鍵

覆寫鍵為 `(source_key,source_sha256)`。`source_key` 取凍結 descriptor 的既有資源鍵（provider、kind、原樣 URL 的 canonical hash），不得從卡號組網址。`source_sha256` 是來源 raw bytes 的 SHA-256，使用不含 `sha256:` 前綴的 64 碼小寫 hex。不是 RGB 像素 hash、公開 WebP hash 或清單 hash。

永久 card／face 不作覆寫鍵：同卡的不同版次、異圖、地區可以有不同版型。卡片身分修復也不得把覆寫搬到別的來源。身分只用於 §3 的重印診斷。覆寫列不存任何 image ID，產圖時直接以覆寫鍵查框。

## 2. 檔案與欄位

覆寫只有 `authored/images/crops.yaml` 一個檔案，頂層恰為 `format: 1, kind: image_crop_overrides, records`，原裁切列放在 records 陣列，沒有 index。檔案遵守 [authored 共同格式](../domains/authored-layout.md#1-路徑與共同格式)的 YAML 邊界與小於 1,048,576 bytes 上限，重用既有嚴格 YAML reader。

loader 直接讀工作目錄裡的檔案，不查 Git：未 commit 的修改在下一次建置就生效。檔案不存在時報錯，不當成空集合；沒有任何覆寫時仍保留 format／kind，寫 `records: []`。全檔一併驗證，不因本次只建 JP 而跳過 EN 列。

每列的欄位：

| 欄位 | 型別與用途 |
| --- | --- |
| source_key | 必填；`sha256:<64 小寫 hex>`，descriptor 的資源鍵 |
| source_sha256 | 必填；64 碼小寫 hex，raw bytes pin |
| top | 必填；真整數（非 bool／float），≥0 |
| left | 選填，預設 36；真整數，≥0 |
| width、height | 選填，預設 384、288；真整數，>0，`3*width == 4*height` |
| reason | 必填；非空白字串，說明為何不用標準框 |
| region | 必填；`jp` 或 `en`，不具權威的審閱標註 |
| card_no | 必填；非空白的原樣卡號，不具權威的審閱標註 |

預設值是目前所有採納列共用的倒吊版型框（459×641 來源，見 §4）；其他尺寸或位置的來源明寫 left、width、height。座標是套用 EXIF 方向後的像素座標，未知欄位拒絕。

實際半開框為 `[left,top,left+width,top+height)`；對選中的來源須滿足 `left+width≤W`、`top+height≤H`，W/H 從已驗證 raw 與既有方向處理取得。覆寫只選位置，不新增旋轉／翻轉選項，不改既有 EXIF 方向處理。

全檔的 `(source_key,source_sha256)` 必須唯一，重複即拒絕，即使框完全相同亦然。同一 source_key 可保留多個 hash 的列；每個鍵只有一個有效框，要改同一來源版本的框時直接修改該列，舊內容由 Git 歷史保留。列的順序不影響結果。

region／card_no 不參與來源查找、跨區配對或採納判定；已驗證 binding 與標註不符時只報告，不能憑標註換用另一張來源圖。

## 3. 解析、建置驗證與報告

選框以凍結 descriptor（kind=image）的覆寫鍵查詢，不分地區：

1. 檔案沒有該 source_key：依既有直向／橫向整數公式用預設框。
2. 有該 source_key，但新 raw hash 沒有對應列：停止本批產圖，錯誤列出 source_key、新 hash 與已有 hash；不套舊框、不退回預設。同網址換圖且新版改回標準版型時，仍須新增該 hash 的列，座標填依新版尺寸算出的預設框。
3. 有完全相符的鍵：把框傳入既有 `build_variants(override=…)`，由它核對框在圖內且精確 4:3。任何失敗均停止，不跳過該列充作完成。

先完整驗證整個檔案，再大量轉檔；未被選中的列列為未使用，不是錯誤。`build_regional_assets(region=…)` 明示 `jp` 或 `en`，只接受相符地區的 image batch。頁面綁定使用該地區萃取器的原始 `<img src>` 與人工 source_face_map。

`images build-index` 的 Inputs.sources 分別釘日英兩區 card／image batch 與 parser version，圖片庫、快取、固定 key 輸出根與圖片私有根彼此隔離。入口對每區呼叫上述 API：完整五檔快取命中時核對存在與大小，缺檔或過時（來源、覆寫或 recipe 改變）的來源當場產圖並完整驗回，寫入圖片庫與快取；`--verify-all` 另完整驗回 PNG 與 WebP。頁面綁定由 `media_projection` 的文字觀察、採納身分及 source_face_map 產生，不查文字 DB。詳細參數見[圖片索引契約](image-index.md)。正式發布仍依既有來源涵蓋與 readiness 門檻。

文字 `snapshot export-offline --image-index` 只讀已釘選的圖片索引，不讀原圖、WebP、裁切檔、快取或圖片版本 state，不再負責產圖或來源驗回。

裁切檔不寫入建置 context 的 configuration 或 dependencies：框改變時 art WebP 的 bytes 跟著改變，已由影像結果與 media 版本反映。純文字建置不讀裁切檔。

`images build-index` 把完整診斷寫到圖片私有根的 `image-report.json`（含本次索引的相對路徑與 `crops` 診斷），stdout 的單行 JSON 摘要另在 `crops` 列出下列四項的張數或列數。報告不含卡片原文或私人路徑，至少列出：

- 套用覆寫的來源圖張數（按覆寫鍵去重，不把同圖多個 binding 重算）。
- 未被本次選中來源用到的覆寫列：鍵與 region／card_no 標註。
- 標註與已驗證 binding 不符的列。
- **不阻擋建置的重印診斷**：同一有效永久 card／face，在其他版次的已驗證來源有覆寫，而本版次的來源沒有覆寫時，列永久 ID、版次 ID 與來源鍵，提醒是否要補框。只用已採納身分及已驗證 binding，不靠標註猜對應；這項診斷不自動繼承框。

## 4. 目前採納的精確框

現有 12 列（JP 六列、EN 六列）。倒吊版型的 12 張來源全部為 459×641，保留倒吊、不另旋轉；left=36、width=384、height=288 固定，即 §2 的預設值：

| JP 卡號 | EN 卡號 | top | 半開框 |
| --- | --- | --- | --- |
| BP10-001、BP10-002、BP10-SL01、BP10-SL02 | BP10-001EN、BP10-002EN、BP10-SL01EN、BP10-SL02EN | 184 | `[36,184,420,472)` |
| BP10-U01 | BP10-U01EN | 232 | `[36,232,420,520)` |
| BP15-PR01 | BP15-PR01EN | 256 | `[36,256,420,544)` |

此表說明已採納的框，不是卡號選框表，也不推導日英身分關聯；有效資料仍須有每張凍結來源的鍵、raw hash 與 top。最後兩張為主戰者圖，與十張上置效果文字框的卡圖分開計數。畫內標誌／簽名的卡圖維持標準框。

這 12 張不是全圖庫無漏檢的結果，也不是自動選框規則；日後發現其他例外，仍按來源另行採納。

## 5. 變體、快照與上傳的版本策略

既有 recipe 已支援來源綁定的整數覆寫，新增採納資料不改 recipe_version；解碼、編碼、縮放或框演算法改變才按既有規則換 recipe。快取 key 包含來源 hash、覆寫設定（無覆寫時為 null）與完整 recipe，命中時仍核對依尺寸算出的實際框：新框取得新 key，沒有新框結果時重新產製，不得借用舊框的完整五檔。

新 key 下五檔會重新產製。在來源與 recipe 不變時，card 三檔輸入不變，內容定址後仍可共用原 blob；art 兩檔改為新 WebP hash／path。不改寫內容定址的建置 blob。`img:binding:` 不依賴裁切，來源／頁面 binding 不變時保持原值。2.0 的公開 key 固定、只換 art_version 並覆寫有變動的 art bytes，card 三檔版本沿用；欄序依 snapshot-format §2.1，不公開框。

裁切檔變動後，須重產受影響的預覽、影像分片、清單與壓縮旁檔，不拿舊清單充作已完成覆寫。`sve-publish images upload` 先按成功圖片描述同步變動卡圖，`sve-publish snapshot upload` 再上傳 preview 根目前清單的 JSON 引用閉包，並核對遠端卡圖上傳清單的精確版本。保留窗口分開看 current＋previous 的 metadata 聯集及僅 current 的圖片集合；previous 圖片引用不保留舊 WebP，不要求同 key 符合兩版輸出。新圖驗妥才切索引，之後由另行執行的回收清理失去 current 引用的 key；無引用的舊檔不會被上傳。

資料入庫後先完成兩檔 art 的實際 WebP 檢視，再重新匯出並執行上傳的離線 dry-run；真正上傳 R2／公開仍需使用者另行同意。來源 raw 與來源歸檔證據依原契約保存，公開快照／WebP 依有限保留策略，不因新框已發布而刪除。
