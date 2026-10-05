# 風味文字的直接對照表

風味文字不走句型模板、不設參數。譯文是整段文字，直接以原文的 hash 對照；譯錯代價低，
標 `origin: machine` 即可入庫，不設審核或採納流程。

## 1. 檔案

`authored/flavor-translations/<hash 首位 16 進位字>.yaml`，每檔單一 `kind: flavor_translation_shard`，
`entries` 依 `(source_hash, lang)` 排序且唯一，同一組合不得出現在兩個檔案。單檔小於 1 MiB。

| 欄位 | 含義 |
| --- | --- |
| source_hash | 該段日文風味原文 exact UTF-8 的 SHA-256；不存原文 |
| lang | 目前只有 `zh-Hant` |
| text | 整段譯文，直接顯示，沒有任何參數或跳脫 |
| origin | `machine` 或 `project`；不使用 `official` |
| low_confidence | 為真時前端顯示「待校對」，且可切回原文 |

譯文使用 LF 換行，不得為空、不得有首尾空白或行尾空白。

角色名稱只需在翻譯時與 glossary 的卡名或角色譯名一致；沒有模板、替換或參數機制。

## 2. 套用

離線建置對每個已知日文 printed 風味的 printing_face，以該風味文字單元的 `content_hash` 查表：

- 命中即寫入 `translation_context`（來源文字單元）、`translation_use`（printing_face，field=flavor）、
  `translation` 與 `translation_selection`，authority 為 unofficial；
- 原文相同的版次共用同一譯文，同卡異版只要原文不同，hash 就不同，是預期行為；
- 英文風味保留原文，不套用日文對照表；
- 未命中（缺譯、官網改字使 hash 不符）、printed 文字 unknown／omitted、卡片身分未確認時，
  不寫譯文，前端退回原文；
- 兩個語言不同的條目是不同的鍵；一段原文不能有兩個互相衝突的譯文。

建置報告的 `flavor_translations` 列出 `entries`（對照表筆數）、`applied`（實際套用的面數）與
`unused`（沒有任何卡面使用的筆數，多半表示官網改字）。

## 3. 測試

自動測試涵蓋對照表載入與格式拒絕、缺譯與英文風味退回原文、匯出到快照的結構（translation 列與 own_source 綁定）。
