# 解析卡片輸入契約

`card-data/` 的格式用來保存已審查的 SVE 日英官方頁面解析結果。
目前已匯入日英基線並實作產生器、reader 與 validator；建置入口與 CI 接線另行實作。
使用者與 sim 仍讀有版號的卡表快照；解析輸入不包含翻譯採納、跨地區配對、效果 DSL 或人工勘誤追蹤。
來源歸檔與備份依[原始來源契約](source-archive.md)保存，不能因 JSON 已入版控而刪除。

## 卡片內容與來源證據

每個地區、包、卡號區段形成一個**卡片檔組**，包含兩個純文字檔：

- 卡片檔 `*.json`：檔頭是地區、包、區段與共用 `products` 商品提示表。每張卡先列 `card_no` 及 `documents`，其中每份文件的 `record` 直接保存費用、名稱、主文、有序卡面、風味、Q&A、繪師等完整卡片內容。其後 `observations` 只列 `source`（`id`、`url`、`fetched_at`）及 `document_ref`，參照**同一張卡**的文件。不同 HTML 版本若解析出完全相同的文件，只保存一份文件。原 record 的商品列表以 `products_ref` 參照同檔檔頭，商品名稱、日期與 URL 仍在卡片檔內；閱讀卡文不必開證據檔。
- 證據檔 `*.evidence.json`：`observations` 以來源 ID 對應證據，`shared` 保存重複內容。reader 合併兩檔後還原下表的完整內部觀測模型。

| 語義欄位 | 契約 |
| --- | --- |
| `recipe` | `sve-parsed-card-v1`；未知 recipe 拒絕讀取 |
| `producer_revision` | 產生者宣告的完整 git commit SHA；格式檢查不證明實際執行的是該 commit，審查者須核對產生紀錄 |
| `evidence.descriptor` | 歸檔來源版本、來源鍵、地區、種類、官方網址、raw SHA-256、raw bytes、最初 receipt hash |
| `evidence` 其餘欄位 | descriptor SHA-256、觀測時間、ETag、Last-Modified；不含 store 名稱或本機位置 |
| `document` | 明示 `jp`／`en`、原卡號與完整 parser record，包含有序卡面、卡文、風味、商品、Q&A、相關卡、圖片 URL、繪師及來源日期 |
| `layout` | 原 DOM 才能證明的主文存在狀態、空白主文、商品區塊與 Q&A／相關連結定位；沒有 HTML 也能還原相同下游投影 |
| `content_hash` | 除自身外全部語義觀測欄位的 canonical JSON SHA-256；批次選取獨立保存，不參與此 hash |

來源 ID 沿用歸檔演算法，不重新配號。地區與官方 URL／卡號須一致；卡面順序與各面的存在證據 source index 一致。
圖片只保留原 URL；不包含圖片、音訊 bytes 或 raw HTML。
`fetched_at` 是觀測時間，不能當成卡文生效日；本契約不推定未知的歷史或生效日。
每個來源版本在整棵資料樹只能有一筆觀測。換 recipe 必須提出新格式與顯式遷移審查，不能為同一來源新增第二個 recipe 或覆蓋舊結果。

證據檔的 `shared` 分為 `recipes`、`parsers`、`locators`、`presence`、`products`、`qa`、`related`、`errata_urls`。
相同 parser／產生者版本、template／container locator 及商品區塊（含 `resolved_urls`）在單檔各表內只寫一次，以 key 參照。
共用 key 是表名加 canonical JSON SHA-256 的前 16 個十六進位字元，例外是 `errata_urls` 使用 `errata-` 前綴；卡片文件使用 `documents-` 前綴。
卡片檔的 `products-` 是商品提示，證據檔的 `products-` 是商品區塊證據，屬於各自檔案的命名空間，不能跨檔互換。
新增不同值不重新編號；短 key 碰撞時拒絕輸出。完整內容與檔案仍以完整 SHA-256 驗證。
有序商品、Q&A、相關連結列表以完整有序值去重，不排序或合併來源差異。

可還原的身分不重存：presence 與 `source_block` 商品 match 的來源 ID 取所屬觀測；descriptor 的地區、種類、URL、來源鍵由卡片來源參照依原歸檔演算法還原。
其餘 raw hash／bytes、最初 receipt hash、descriptor hash、ETag、Last-Modified、whitespace、presence result hash 與 `content_hash` 都保存在證據檔。
合併後重驗所有語義 hash，再重新拆分比較**兩檔完整 canonical bytes**；懸空參照、未使用項目、不正確去重、多餘證據及 map 排序不同均拒絕。
重排儲存格式不改變語義觀測 `content_hash`，也不改寫既有 `producer_revision`。

## 批次選取與歷史保護

每個已審查批次另存於 `batches/<原批次 SHA-256 的 64 位十六進位>/0000.json` 等成員檔。
每個檔案保存 `format`、原批次 `id`、`region`、`versions`（來源 ID → 是否 current 的布林值）及從零開始的 `part`。
按來源 ID 排序後**固定每 1,000 筆**切一個成員檔；除最後一檔外必須滿 1,000 筆，檔名為至少四位的 part 編號。
這是新批次的來源選取成員，不是卡片的百號區段；新批次有自己的目錄，永不重新分配舊批次。
`index.batches` 記錄原批次 `id` 與按 part 排序的 `parts`，逐檔釘選獨立 `sha256` 與 `bytes`。
reader 重組後重切，檢查連續 part、排序、容量、地區及來源 ID 不重複。
批次選取檔的 hash **不是**原始 inventory hash；原批次 ID 只標明產生時已驗證的來源批次。
產生器從完整、已驗證的凍結來源批次建立選取表；reader 驗證每筆來源存在、地區一致、同批每 URL 至多一個 current，且全部觀測至少被一批選取。

新增批次只新增獨立選取成員檔並更新索引。未變的卡片檔與證據檔 bytes 保持不變。
選取表保留重建指定批次需要的全部來源版本與 current 狀態，供沒有 HTML 的建置使用。
原始 inventory 的建立時間、manifest 指紋與逐批 receipt 對照僅留在私有來源歸檔，不複製到公開樹；公開資料不足以重建只差 store 名稱的 inventory。
此格式不驗證執行時 store 名稱是否為原始名稱；store 對應仍由私有建置設定與來源歸檔驗證。也不宣稱讀取 JSON 等於重驗原始 raw bytes。

更新保留舊觀測、完整舊批次選取與舊分檔位置。同一來源重跑若改變解析結果即失敗。
`load` 只保證目前資料自洽，無法偵測連同 hash、索引一起改寫的歷史。
每個修改 `card-data/**` 的 PR，CI **必須**以 PR base commit 的完整 `card-data/` 執行
`parsed-cards validate <候選根> --previous <PR base 資料根>`；不以同一棵候選樹充當前版。
CI 接線尚待建置整合實作，完成前由本機執行相同前版驗證並在交付紀錄列明基底。
首次匯入沒有前版，只能由原始來源本機產生、核對來源與人工審查，不能宣稱有歷史保護或補造缺失的舊文。

## 固定分檔規則

**全部卡包**一律採固定每 **100** 號區段，不按大小決定是否切檔。
`family` 由明示歸檔表指定，不從跨地區卡號猜測配對。
增量產生自動沿用前版所有卡片的 family；請求只須列新卡，重列舊卡若衝突即失敗。
`index.json` 記錄 `range_width: 100`、`files` 中每個卡片檔的 `path`／`sha256`／`bytes`／卡數／觀測數，以及 `evidence_sha256`／`evidence_bytes`。
證據路徑固定將卡片路徑的 `.json` 換成 `.evidence.json`。

```text
jp/PR/000-099.json
jp/PR/100-199.json
jp/CP04/P/000-099.json
jp/CP04/SP/000-099.json
en/PR/000-099.json
```

取最後一個連字號後、類別前綴之後的連續 ASCII 十進位數字；EN 只在此步移除尾端 `EN`，原卡號不變，也不作跨地區映射。
同包不同字母前綴各自分組；無前綴直接使用區段檔名。
允許前綴 `A–Z`／`Ⓢ` 與數字後小寫後綴，例如 `CP04-U13`、`BP03-LDⓈ01EN`、`SYN-007a`。
路徑中每個 `Ⓢ` 固定編碼為 `_circled_s`，例如 `LDⓈ` → `LD_circled_s`；原前綴不允許底線，此映射不會與 `LDS` 或其他合法前綴碰撞。卡號內容保持原樣。
`EP`、`SEP` 兩個沒有數字的尾段放該包的 `unnumbered.json`；其他不符格式者拒絕產生，先審查例外規則。
區段起點為數字除以 100 後向下取整再乘以 100，檔名至少三位數；不按檔案大小自動重切、不用 hash 分桶。
新增較大號碼只進對應新區段；舊號補收與新來源觀測仍須更新原區段。

每個 JSON 檔（含索引與批次選取檔）上限 1 MiB（1,048,576 bytes）。
歷史或來源集合累積超限時，產生失敗並交維護者審查容量／契約調整，不自動改寬、搬卡或壓縮縮排。
相同文件去重只減少重複卡文；來源證據及每批選取仍會累積，不能保證無限歷史容量。

## 序列化與讀取

UTF-8、無 BOM、LF、檔尾一個 LF、兩空格縮排、不轉義非 ASCII；物件欄位依模型宣告順序，map 依 key 排序。
卡片依原樣卡號排序，觀測依來源 ID（hash）排序，**不是時間順序**；觀測時間須看 `fetched_at`。
面、段落、商品、Q&A 與相關連結保留來源順序。
不可重複 JSON key、不可未知欄位、不可符號連結或穿越根目錄，不接受缺檔、多出的 JSON 或錯誤 hash／bytes／數量。
嚴格 reader 完成整個閉包驗證後才交付資料，無網路、HTML、manifest 或 latest cache fallback。
hash 是變更偵測與鎖定工具，不能代替來源核對、權利確認或 PR 審查。

機器 Schema：[索引](parsed-cards/index.schema.json)、[卡片](parsed-cards/cards.schema.json)、[證據](parsed-cards/evidence.schema.json)、[批次選取](parsed-cards/batch.schema.json)。
`codec.schema` 產生封閉的 Draft 2020-12 JSON Schema；測試逐 byte 比對提交的四份 Schema，避免漂移。
`sve-carddb parsed-cards schema <全新絕對目錄>` 輸出上述四份檔案；runtime 同時驗 schema、身分關係與 canonical bytes。
卡片檔可單獨閱讀，完整輸入 reader 仍須驗證配對證據、批次選取與索引。

## 產生與驗收

`sve-carddb parsed-cards generate <request.json 絕對路徑> <全新輸出絕對目錄> [--previous <前版絕對目錄>]` 只產生候選資料，不推送、不覆寫來源或已存在的目錄。
私有 request 使用下列結構；來源位置只存在操作輸入，不會寫入 card-data：

```json
{
  "sources": [
    {"region": "jp", "root": "/archive", "store_id": "configured-store", "batch_id": "sha256:<64 hex>"}
  ],
  "filing": [{"region": "jp", "card_no": "SYN-001", "family": "SYN"}],
  "producer_revision": "<40 hex commit>"
}
```

來源批次須完整且限定單地區 SVE card pages；產生器讀取並驗原始 hash，套用與既有建置共用的 DOM recipe。
`sve-carddb parsed-cards validate <資料絕對目錄> [--previous <前版絕對目錄>]` 驗證全部檔案與更新歷史。
CLI 錯誤只印失敗摘要，不將可能含完整卡文的驗證物件印入 log。

推送前人工清單包含各類檔案數、總 bytes、最大檔、欄位、實際日英範例及排除秘密／本機路徑／媒體 bytes 的檢查。
切換建置須證明相同來源集合在沒有 SVE HTML 的隔離環境逐欄一致，匯出通過公開 reader 與完整 Python／Rust 覆蓋率門檻。
壓縮檔適合原始來源備份或傳輸封裝；不取代供 Git diff 審查的純文字輸入，也不改變備份還原驗證義務。
