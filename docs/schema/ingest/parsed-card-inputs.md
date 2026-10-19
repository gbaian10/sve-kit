# 解析卡片輸入契約

`card-data/` 保存已審查的 SVE 日英官方頁面解析結果，作為 carddb 的輸入。
使用者與 sim 仍讀有版號的卡表快照；解析輸入不包含翻譯採納、跨地區配對、效果 DSL 或人工勘誤追蹤。
來源歸檔與備份依[原始來源契約](source-archive.md)保存，不能因 JSON 已入版控而刪除。

## 身分、證據與歷史

每個地區、原樣卡號下保存依來源版本排序的 `observations`。每個分片由兩個純文字檔組成：

- 卡片檔 `*.json`：檔頭是地區、包、區段及共用 `products` 表；每筆觀測只放 `source`（`id`、`url`、`fetched_at`）與 `document`。費用、名稱、主文及有序卡面直接保存在 `document.record`，不必開證據檔就能閱讀。原 record 的商品列表改為 `products_ref`，只參照同一張卡片檔的檔頭；商品名稱、日期、URL 仍在該檔內。
- 證據檔 `*.evidence.json`：同地區、包與區段；`observations` 以來源 ID 對應證據，`shared` 共用表保存重複內容。建置 reader 合併兩檔後，還原下表的完整內部觀測模型。

下表描述**合併後的語義欄位**，不是卡片檔逐筆重複的儲存格式：

| 欄位 | 契約 |
| --- | --- |
| `recipe` | `sve-parsed-card-v1`；未知 recipe 拒絕讀取 |
| `producer_revision` | 產生器完整 git commit SHA |
| `evidence.descriptor` | 既有歸檔 descriptor：來源版本、來源鍵、地區、種類、官方網址、raw SHA-256、raw bytes、最初 receipt hash |
| `evidence` 其餘欄位 | descriptor SHA-256、觀測時間、ETag、Last-Modified；不含 store 名稱或本機位置 |
| `document` | 明示 `jp`／`en`、原卡號與完整 parser record，包含有序卡面、卡文、風味、商品、Q&A、相關卡、圖片 URL、繪師及來源日期 |
| `layout` | 原 DOM 才能證明的主文存在狀態、空白主文、商品區塊與 Q&A／相關連結定位；沒有 HTML 也能還原相同下游投影 |
| `content_hash` | 除自身與批次 membership 之外全部觀測欄位的 canonical JSON SHA-256 |
| `memberships` | 原凍結批次 ID、該批次 receipt hash 與是否為 current；依批次 ID 排序、不重複 |

來源 ID 沿用歸檔演算法，不重新配號。地區與官方 URL／卡號須一致；卡面順序與各面的存在證據 source index 一致。
圖片只保留原 URL；不包含圖片、音訊 bytes 或 raw HTML。
`fetched_at` 是觀測時間，不能當成卡文生效日；本契約不推定未知的歷史或生效日。

證據檔的 `shared` 分為 `recipes`、`parsers`、`locators`、`presence`、`products`、`qa`、`related`、`errata_urls`、`batches`。
相同 parser／產生器版本、template／container locator、商品區塊（含 `resolved_urls`）及批次 ID 在單檔各表內只寫一次，再以 key 參照。
key 是類別名稱加該共用值 canonical JSON SHA-256 的前 16 個十六進位字元；新增不同值不重新編號。若短 key 碰撞，產生器拒絕輸出，不以覆蓋解決；完整內容及檔案仍以完整 SHA-256 驗證。
有序商品、Q&A、相關連結列表以完整有序值去重，不排序或合併來源差異。

可無歧義還原的重複身分不重存：presence 與 `source_block` 商品 match 的來源 ID 取所屬觀測；descriptor 的地區、種類、URL、來源鍵由卡片來源參照依原歸檔演算法還原；membership 的 `receipt_id: null` 表示與 `first_receipt_id` 相同。
其他 raw hash／bytes、receipt、descriptor hash、ETag、Last-Modified、whitespace、presence result hash 與 `content_hash` 都保存在證據檔。
合併後重驗所有原始完整 hash，再重新拆分比較 canonical 共用表；懸空參照、未使用項目、不正確去重或多餘證據均拒絕讀取。
此拆分不改變原觀測 `content_hash` 或原始批次 ID；序列化程式更新也不改寫既有 `producer_revision`。

更新時追加新的來源版本，保留舊觀測、舊批次與舊分檔位置。
驗證更新必須提供前一版完整根目錄：只驗當前自洽 hash 無法發現有人連同索引一起刪掉歷史。
同一來源與 recipe 的重跑若改變解析結果即失敗；調整 parser 語義必須提出新的 recipe 契約與顯式遷移審查，不能藉更新 hash 覆蓋舊結果。
首次匯入只能宣稱保留本次已取得的來源版本，不能補造缺失的舊文。

## 固定分檔表

一般包為 `<region>/<family>.json`，例如 `jp/BP01.json`。`family` 由明示的歸檔表指定，不從跨地區卡號猜測配對。
`index.json` 記錄固定的 `split_families`、`range_width: 100`、全部卡片檔的 `path`／`sha256`／`bytes`／卡數／觀測數與批次完整性計數。
每筆釘選另有 `evidence_sha256`／`evidence_bytes`，證據路徑固定將卡片路徑的 `.json` 換成 `.evidence.json`；例如 `jp/BP01.json` 配 `jp/BP01.evidence.json`，分段則 `jp/PR/000-099.json` 配 `jp/PR/000-099.evidence.json`。
批次釘選另保留原始建立時間及 manifest hash／bytes／schema 版號；配合每筆 membership 的 receipt hash 與執行時明示的邏輯 store 名稱，可還原原始 inventory 並驗回原批次 hash，不需保存私人位置或讀取 HTML。此驗證不宣稱已重驗當下原始 bytes，raw 歸檔仍依原契約獨立保存。
需切檔的包按卡號數字固定每 **100** 號保存：

```text
jp/PR/000-099.json
jp/PR/100-199.json
jp/PR/200-299.json
en/PR/000-099.json
```

取最後一個連字號後、類別前綴之後的連續 ASCII 十進位數字；EN 只在此步移除尾端 `EN`，原卡號不變，也不作跨地區映射。
同包內前綴各自分組，例如 `jp/CP04/P/000-099.json`、`jp/CP04/SP/000-099.json`；無前綴仍為 `jp/CP04/000-099.json`。
允許前綴 `A–Z`／`Ⓢ` 與數字後小寫後綴，例如 `CP04-U13`、`BP03-LDⓈ01EN`、`SYN-007a`。
`EP`、`SEP` 兩個沒有數字的尾段放該包的 `unnumbered.json`；其他不符合格式者拒絕產生，先審查例外規則。
區段起點為數字除以 100 後向下取整再乘以 100，檔名至少三位數；不按檔案大小自動重切、不用 hash 分桶。
新增較大號碼只進對應新區段；舊號補收與新來源觀測仍須更新原區段，不保證所有新資料都只改最新檔。

每個 JSON 檔（含索引）上限 1 MiB（1,048,576 bytes）。歷史累積或異畫同號使固定區段超限時，產生失敗並交維護者審查容量／契約調整，不自動改寬或搬舊卡。
首次採納的固定分檔表必須一併審查；後續驗證禁止既有卡片移動，即使新索引自洽也不例外。

## 序列化與讀取

UTF-8、無 BOM、LF、檔尾一個 LF、兩空格縮排、不轉義非 ASCII；物件欄位依模型宣告順序。
卡片依原樣卡號排序，觀測依來源版本與 recipe 排序。面、段落、商品、Q&A 與相關連結保留來源順序。
不可重複 JSON key、不可未知欄位、不可符號連結或穿越根目錄，不接受缺片、多出的 JSON 或錯誤 hash／bytes／數量。
嚴格 reader 完成整個閉包驗證後才交付資料，無網路、HTML、manifest 或 latest cache fallback。
hash 是變更偵測與鎖定工具，不能代替來源核對、權利確認或 PR 審查。

產生的機器 Schema：[索引](parsed-cards/index.schema.json)、[卡片](parsed-cards/cards.schema.json)、[證據](parsed-cards/evidence.schema.json)。

內部語義模型在 `carddb/src/sve_carddb/domains/parsed_cards/models.py`，卡片與證據檔模型在同目錄的 `wire.py`；`codec.schema` 產生封閉的 Draft 2020-12 JSON Schema。
`sve-carddb parsed-cards schema <全新絕對目錄>` 可輸出 `index.schema.json`、`cards.schema.json`、`evidence.schema.json`，runtime 同時驗 schema、身分關係與 canonical bytes。
卡片檔可單獨閱讀，但建置必須驗證配對證據與完整索引；缺證據不可視為可建置的輸入。

## 產生與驗收

`sve-carddb parsed-cards generate <request.json 絕對路徑> <全新輸出絕對目錄> [--previous <前版絕對目錄>]` 只產生候選資料，不推送、不覆寫來源或已存在的目錄。
私有 request 使用下列結構；來源位置只存在操作輸入，不會寫入 card-data：

```json
{
  "sources": [
    {"region": "jp", "root": "/archive", "store_id": "configured-store", "batch_id": "sha256:<64 hex>"}
  ],
  "filing": [{"region": "jp", "card_no": "SYN-001", "family": "SYN"}],
  "producer_revision": "<40 hex commit>",
  "split_families": []
}
```

來源批次須完整且限定單地區 SVE card pages；產生器讀取並驗原始 hash，套用與既有建置共用的 DOM recipe。
`sve-carddb parsed-cards validate <資料絕對目錄> [--previous <前版絕對目錄>]` 驗證全部分片與更新歷史。
CLI 錯誤只印失敗摘要，不將可能含完整卡文的驗證物件印入 log。

推送前的人工清單包含檔案數、總 bytes、最大檔、欄位、實際日英範例與排除秘密／本機路徑／媒體 bytes 的檢查。
切換建置須證明相同來源集合在沒有 SVE HTML 的隔離環境逐欄一致，且匯出仍通過公開 reader、完整 Python／Rust 覆蓋率門檻。
壓縮檔適合原始來源備份或傳輸封裝；不取代這份供 Git diff 審查的純文字輸入，也不改變備份還原驗證義務。
