# 公開快照候選匯出器

本文件說明 Python producer 的邊界；公開欄位與分片語意以
[snapshot-format.md](snapshot-format.md)、[snapshot-transport.md](snapshot-transport.md)
為準，機器契約以套件內 Schema 為準。

## 輸入與輸出

`sve_carddb.snapshot.export.export_snapshot(projection, ownership, batch, brotli=...)`
接受公開 `Projection`、建置端的版次歸屬與預覽批次。`Ownership.from_database`
透過已驗證 SQLite 邊界讀取永久 `printing.home_set_id`；不能從卡號、商品收錄或
`card.home_set_id` 補猜版次歸屬，也不把此建置欄加入公開列。

`Batch` 只提供 `data_version,published_at,regions`，格式與完整快照清單由 Schema
驗證。此候選匯出器只接受 `preview-` 批次。回傳 `Snapshot` 包含快照清單、按 key
索引的 `Blob`、`text_all` 及建置用壓縮 recipe。寫檔、分配批次與發布由呼叫端負責；
此函式不讀 live manifest、不抓來源、不更新版本索引。

`Blob.raw` 是 canonical-json-v1 bytes；`br` 是可空 bytes，`gzip` 是固定 level 9、
mtime=0、無 filename 的 bytes。沒有提供 Brotli 時，快照清單的 br 長度明示 null，
容量量測也不宣稱 br 閘門通過。Brotli 由呼叫端提供釘住版本、固定 quality 11 的
`Brotli(version, compress)`；版本與參數屬建置 recipe，不放進可重用的資料 payload。
呼叫端須將這份 recipe 與其他建置依賴一起釘住；啟用新的壓縮函式庫遵守專案的依賴流程。

## 固定分片與裝檔

bucket 數只讀取目前候選 Schema 的 `bucket_count.const`，不接受呼叫端任選 N。
分片鍵採 canonical 主實體／完整主鍵陣列與全 SHA-256；永久 ID 不縮短、不重配。
現行加 pending 暫顯的 `display_ref` 決定 revision 的 bootstrap/detail；其餘完整列
只進 history。text 與 translation 按名字、可用名字翻譯及啟動欄位的文字引用閉包分列。

裝檔保持同一 owner/bucket/partition 的完整 fragments 在同一檔案，避免讓可重用的
永久 ID 與 types 閉包跨檔重複。檔內不同集合仍各有固定 fragment 身分，不合併邏輯列。
每片 raw 512 KiB 的預檢由量測 API 明示結果；超過上限仍可作容量診斷的候選產物，
不能宣稱正式通過、任選 bucket 數、重配 owner 或丟棄資料。base 檔案 bytes/hash
變動時，相關詳情片必須以新 base hash 重建，即使 row_index 本身不變。

printing 與 display revision 的詳情片一對一提供每個 base row，保留 unknown/null
值。printing 用永久 `face.ordinal`，不把陣列位置當 ordinal；revision 的 name 與
非 name translations 各存一次。每片只附實際使用的 nested types 閉包。

## 完整性與容量驗證

匯出完成必須通過交付點 A 的獨立 `read_snapshot`，再與輸入 logical view 逐欄相等。
未知 descriptor、缺 required、重複 PK、錯誤 base/dependency、循環、未閉合的公開引用、
錯誤列位置與非法 pending 呈現都會失敗。`text_all` 保留原 File 分組、member bytes
與 hash，透過同一 reader join，不另出全欄複本。

`Snapshot.assert_identical` 比較兩次輸出的 raw/br/gzip、文字聯集與 recipe；批次時間
及版號只影響快照清單，未變 payload bytes 可跨版重用。

`export.measure.measure` 回傳數字：全文與啟動包 raw/br/gzip、逐片 owner/bucket/列數/
大小、超限片與各容量閘門。它把快照清單和 config 計入，排除影像、空 DSL 附件及
作為替代下載的 `text_all` 重複計算。啟動包分別列 br/gzip 的 1 MiB 檢查；全文遵守
raw 40 MiB、br 8 MiB、gzip 10 MiB。任何失敗均不能宣稱正式容量驗收通過。

`export.measure.update` 比較不可變 payload bytes，列出變更檔鍵與替換下載量（含新
快照清單）；base hash 變動即使 row_index 不變也會重建詳情片。選用 `text_all` 更新
時應另計整個聯集，不能與個別分片同時計算。

JP 量測不足以凍結正式配置；正式前還須量 EN、雙區與完整三語名字／facet 閉包，
包含零 pending、實際分布、全 pending、最大片與增量重建量。手機的 48/80 MiB
常駐／更新記憶體與解析時間仍按 [size-budget.md](size-budget.md) 獨立驗收。
