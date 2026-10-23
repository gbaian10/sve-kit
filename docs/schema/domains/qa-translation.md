# 官方 Q&A 的直接譯文

日文官方 Q&A 的問與答各自整段直譯，不走模板，不從英文轉譯。存放比照[風味文字](flavor-translation.md)，
但另綁 Q&A 身分及欄位，避免一個簡短答案的試譯擴散到其他未翻譯的 Q&A。

## 1. 分片與鍵

`authored/translations/qa/<卡包>.yaml` 使用 `format: 1`、`kind: qa_translation_shard`、非空 `entries`。
每檔小於 1 MiB；讀取目錄內全部普通 YAML 檔案，不使用索引。不接受 symlink、其他檔案、重複 YAML 鍵、未知欄位及錯誤型別。

每筆欄位如下，依 `(qa_id, field, source_hash, lang)` 排序且唯一；跨檔不得重複同一鍵。

| 欄位 | 定義 |
| --- | --- |
| qa_id | 建置資料的日文 qa.id；不以卡號或官網 Q 編號推算 |
| field | question 或 answer |
| source_hash | 該版本、該欄日文原文 exact UTF-8 的 SHA-256，包含 sha256: 前綴；不 trim 或正規化 |
| lang | zh-Hant |
| text | 整段譯文，非空、LF 換行，不得有首尾或行尾空白 |
| origin | machine 或 project，不得宣稱 official |
| low_confidence | 必填布林值；本波機器試譯填 true |

卡包僅決定檔案歸屬，不決定 owner。相同 Q&A 被多張卡片引用時只保存一次；不同 Q&A 的相同答案須各有一筆，
建置時 context 的 semantic_variant 使用 `qa_id:field`，讓相同短答可以依問句翻成「不行」「不會」或「不是」，不會互相衝突。
同一則 Q&A、同欄位、同原文的鍵仍只能有一筆譯文。

## 2. 建置與匯出

離線建置讀取分片後，以 `qa_id`、欄位及該 `qa_version` 原文單元的 hash 比對，僅處理日文 Q&A 與日文文字單元。
命中才透過既有 direct writer 建立 context、use、translation 與 selection：

- owner 為 qa_version；field 為 question／answer；ordinal 為 null。
- origin 與 low_confidence 原樣保存，authority 為 unofficial，公開 binding 為 own_source。
- 缺譯或 hash 改變不寫該欄譯文，消費端退回日文原文。不因問句有譯文而假定答句也已翻好。
- 若歷史版本仍在建置中，舊 hash 只可套用到實際符合的歷史版本，不可搬到新版本。
- 英文 Q&A 即使文字碰巧相同也不套用。

建置報告 `qa_translations` 提供 `entries`（分片條目數）、`applied`（命中的版本欄位數）、
`stale`（沒有任何日文版本欄位命中的條目數）。stale 包含已改版、已移除及不在本次建置範圍的來源，
不是缺譯數。缺目錄等同零條目。公開格式、capabilities 及分片規則沿用既有契約。

## 3. 可重複執行的檢查

對已關閉的隔離建置 DB 執行；不讀 live manifest，不連網，不修改 DB：

```bash
uv --directory carddb run sve-carddb qa-translations check --authored /absolute/authored --database /absolute/bundle/build.sqlite
```

程式入口為 `domains.translations.qa_checks.check`，命令介面為 `qa-translations check`。檢查使用目前 authored 譯文，
對照指定建置的日文來源、已套用名稱優先順序的 name owner 譯文，以及已解開來源引用的 glossary 選詞。
變更 glossary／名稱政策後，先重建 DB 再檢查。

檢查項目：

- `『…』` 完整卡名使用建置中採用的中文名；未知名稱回報 unknown_card_name，不能默默放過。
- 最長匹配的 glossary 用語須出現在譯文；詞條中的「…」代表任意物件，「召喚到戰場上」允許物件夾在動詞與目的地之間（風格 R44）；卡名內容不當作規則用語。排除「場合」中的「場」及「エボルヴフォロワー」中的獨立卡種詞「エボルヴ」，避免複合詞誤報。
- Q 編號及阿拉伯數字按出現次數保留；檢查時統一全半形，完整卡名中的數字隨採用卡名處理。
- 「はい。」固定為「是的。」；「いいえ。」按問句語義為「不行。」「不會。」「不是。」或「不需要。」（必要性問句）。工具檢查允許開頭，語義選擇由翻譯與互審確認。
- 無匹配原文的條目回報 stale；失敗以非零結束碼回傳，只列來源 hash 與錯誤代碼，不印問答全文。

工具是字詞一致性檢查，不判定語義等價。引號中的能力引文、名稱片段、原文誤字及 EVOLVE 補註目前需要接收端先核對，
未知引文仍回報待處理，不視為已通過。逐則翻譯與另一模型互審仍負責條件、否定、時點與數量作用域。

## 4. 驗證範圍

既有離線建置測試加入兩個代表例：正例涵蓋 reader、指定日文 owner、問答匯出及品質旗標；
反例涵蓋舊來源 hash 不套用、計入 stale 及嚴格格式拒絕。實際試譯由相同命令檢查。
