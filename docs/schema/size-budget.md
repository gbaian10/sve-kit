# 卡表快照容量與記憶體預算

卡表快照的容量限制、啟動目標、量測方法與目前結論。快照的分片與欄位分割規則見 [snapshot-format.md](snapshot-format.md) §3。

## 預算

完整文字容量門檻於 2026-09-28 定案；範圍是卡表快照的完整文字分片合計，不含卡圖、語音與 DSL 程式包（這些另外下載，見 `docs/terminology.md`）。啟動包目標於 2026-10-03 更正為依使用者所選日版／英版，各自以 Brotli 計算的約略目標。

| 項目                                | 門檻             | 說明                                          |
| ----------------------------------- | ---------------- | --------------------------------------------- |
| 卡表快照，Brotli 壓縮後             | ≤ 8 MiB          | 主要門檻，Brotli 品質 11                      |
| 卡表快照，gzip 壓縮後               | ≤ 10 MiB         | 伺服端只能提供 gzip 時的對應門檻，gzip 等級 9 |
| 卡表快照，解壓後                    | ≤ 40 MiB         | 未壓縮 canonical payload 合計                 |
| 啟動包，Brotli 壓縮後（依所選版本） | 約 1 MiB（盡量） | 約略目標，非硬門檻；2 MiB 也可接受            |

記憶體門檻仍待手機實測驗收（中階 Android 與 iPad），不含 WebGL、圖片與 app 本身：

| 項目                         | 目標      |
| ---------------------------- | --------- |
| 建好索引後的卡表常駐 JS heap | ≤ 48 MiB  |
| 更新期間峰值                 | ≤ 80 MiB  |
| 單一分片解壓後               | ≤ 512 KiB |
| 背景解析時單段主執行緒工作   | ≤ 50 ms   |
| 完整可搜尋資料的解析時間累計 | ≤ 1 秒    |

完整文字或單個資料 File 超過限制就停止發布效能驗收，調整投影或固定分片配置；不能只因 gzip 小就宣稱手機順暢。單片 512 KiB 計整個實際下載容器（含 types 與 fragment metadata），不只計其中一個 fragment；manifest 與替代下載的 text_all 另計，不能漏算其總量或記憶體。

啟動包約 1 MiB 是盡量達成的目標，不是 br／gzip 的 CI 閘門。JP／EN 依所選版本各自計 manifest＋config＋首屏實際必載 File 與依賴，按 key 去重。共用／混區檔整檔計入每個需要它的版本，不能按列數或語言比例分攤；仍載全部 bootstrap 時兩區數字會相同。raw／gzip 另報實測，gzip 不另套 1 MiB。

任何所選版本的啟動 Brotli 超過 2 MiB（2,097,152 bytes）時，停止配置定版與發布推進，交維護者看數字及改善方案，取得該配置的明示同意後才繼續。這是決定停點，不是新增 CI 硬門檻；不能只報超標便自行放行，也不能排除卡片、名字、翻譯或 pending 狀態來壓容量。

## 量測方法

- 以 7,369 個真實 JP 版次、約 3,710 張卡實測日文部分；EN 結構假設同量，三語文字長度與壓縮率由各語言的獨立語料推估。
- 全包公式：`(主表小計 + 欄位分割的額外開銷 × 2 區 + QA/勘誤摘要 × 2 區 + 稀疏機制 coverage + 版次 metadata + Decklog 欄位增量 × 2 區) × 1.20 裕度 × 分片倍率`。raw、Brotli、gzip 各自計算，最後才取整。
- 其他 metadata 以 2.5 MiB raw／0.6 MiB 壓縮後全額預留，不扣除推測的重疊。
- Decklog 四欄（`decklog_available`、`decklog_verification`、`decklog_source_url`、`decklog_checked_on`）只存在啟動包的 `printing` 頂層；來源網址以「唯一 URL 陣列＋逐列整數引用」的字典計入，兩區各自計算，不跨區共用證據。
- 來源網址長度（96／192 bytes）、查證比例與日期是容量用的 fixture，不是真實 Decklog 資料；主要情境取每版次一個不同來源、全部未查證。
- MiB = 1,048,576 bytes；Brotli 品質 11，gzip 等級 9。
- 分片倍率沿用既有分片量測；正式匯出器完成後要以實際最大分片重新驗收。

## 早期原型與推估

主要情境（每版次各自的 96-byte 來源、全部未查證、稀疏機制標籤）的全三語估算：

| 項目   | 估算值    | 門檻     | 結果 |
| ------ | --------- | -------- | ---- |
| 解壓後 | 26.06 MiB | ≤ 40 MiB | 通過 |
| Brotli | 5.09 MiB  | ≤ 8 MiB  | 通過 |
| gzip   | 5.23 MiB  | ≤ 10 MiB | 通過 |

敏感度情境（共用來源、10% 或全部已查證、192-byte 長網址、機制標籤密度加倍或 partial coverage）的估算落在 raw 24.3～27.7 MiB、Brotli 4.70～5.12 MiB、gzip 4.84～5.28 MiB，全部在預算內。

表記未定的公開呈現須量測省略 settled wording 項後的日版／英版啟動包，包含當次實際可用的名稱翻譯；manifest、config、必載 bootstrap 與 pending wording 引用依上述各版本 Brotli 目標計算。bootstrap 須計入每個 pending face-region 最多一筆 display revision 的輕量投影、名稱及可用名稱翻譯文字閉包，讓首屏可讀暫顯卡名、搜尋名稱與建立 facet。display_ref 以外的候選 revision 與文字閉包按需載入，另報容量，不算成已完成候選索引；不能讓其餘候選引用形成必載依賴後仍漏算啟動成本。

早期啟動包原型只量了日文部分：Brotli 約 0.30 MiB（raw 3.75 MiB）。這不能代表日英全庫、當次名稱翻譯或新分片配置；每次定版須以完整新 manifest、types、依賴與實際裝檔重測 startup_by_region，不沿用舊 manifest 大小估算。

以上是早期原型實測加推估，不是正式匯出器或手機驗收。之後資料量增加（新卡包、EN 完整資料、繁中翻譯）要以實際輸出重新對帳；沒有名稱翻譯的批次不能代表完整三語規模。

## 手機記憶體

reader 建好 typed 索引後必須釋放啟動包的原始 tuple 陣列與解碼字串，只保留 TypedArray、唯一字串池、索引與當頁 view。驗收時量「建索引後常駐」的 JS heap＋ArrayBuffer、詳情分片的 LRU 與更新期間峰值。桌面瀏覽器的 tuple heap 量測（約 11 MiB）不能代替 typed store 在手機上的實測。

影像 metadata 的 CacheStorage bytes、解析暫存、當頁 image 列、衍生 Map／view 及 pin 工作集均須量測。format 1.1 的調度預算是 12 MiB raw 對應量與最多 64 檔，並不等於 JS heap；解析整個 bucket 後只保留當頁需要的 image 列，其他列即釋放，不建立全庫物件索引。PR 的桌面 heap 數字仍不能代替手機 48／80 MiB 驗收。
