# 里程碑 2：範圍與交付

本文件記錄卡表資料管線的已決範圍與待決事項；進度見 [主 issue #4](https://github.com/gbaian10/sve-kit/issues/4)。
建置資料庫語意、公開欄位與容量門檻仍分別以 [build-db.md](build-db.md)、[snapshot-format.md](snapshot-format.md)、[size-budget.md](size-budget.md) 為準。
下列交付與驗收是實作要求，不代表匯出器或發布閘門已完成。

## 前端交付點

| 交付 | 負責項目 | 內容與完成界線 |
| --- | --- | --- |
| A：契約與手寫樣本 | [M2-01c／#20](https://github.com/gbaian10/sve-kit/issues/20) | 公開 JSON Schema、`tests/fixtures/snapshot-contract/v1/` 手寫合成 golden、Python 獨立 reader 與 TS 應驗清單；不依賴建置資料庫，不寫永久版本索引 |
| B：日版預覽快照 | [M2-13p／#33](https://github.com/gbaian10/sve-kit/issues/33) | 用正式匯出器產 JP-only 預覽快照；驗已啟用能力、Schema、公開引用閉包、分片 join、hash／counts 與容量，列出未完成的正式閘門 |
| 預覽卡圖 | [M2-20／#35](https://github.com/gbaian10/sve-kit/issues/35) | 將 WebP 與影像清單接入預覽快照，驗來源、尺寸、內容 hash 與圖片發布狀態；先於 CI 引擎接線交付 |
| 正式文字與卡圖快照 | [M2-15／#42](https://github.com/gbaian10/sve-kit/issues/42) | 對選定地區完成首發能力、發布閘門與容量驗收；文字、影像及其資產閉包納入同次本機發布交易，最後原子更新永久版本索引 |

A 的 golden 包含 tuple、巢狀型別、`row_index`／`face_ordinal` 及手寫的 join 後 logical view。
里程碑 2 交付 Python reader 與 TS 應驗項目；**TS reader 由里程碑 3 自行開 issue 實作**，讀同一份 Schema 與 golden，不另留副本。
卡片列表、單卡頁、篩選、更正徽章與手機效能由里程碑 3 驗收，不作里程碑 2 的跨專案完成條件。

## 預覽與正式發布的隔離

[預覽快照（preview）](../terminology.md) 僅供非公開開發；固定先交 JP，正式首發地區仍待量測後決定。
預覽使用 `SVE_PREVIEW_DIR`，正式本機發布使用 `SVE_CDN_DIR`；兩個根目錄不得相同或互相包含。
預覽 `data_version` 使用 `preview-` 前綴，不寫 `snapshots/versions/index.json` 或其 pages，不改正式 active。
預覽不供公開 URL、永久分享碼或回放 pin 使用；正式發布器拒收預覽版號，須重新建置並驗證正式發布範圍，不能直接將預覽升為正式版。
版號完整語法與傳輸形狀由 [M2-01b／#19](https://github.com/gbaian10/sve-kit/issues/19) 補齊。

協調者須在 A／B 交付前同步里程碑 3：前端資料根設定要能明確切換預覽與正式根目錄，IndexedDB／Cache namespace 也必須隔離。
B 的交接須附 manifest 入口、輸入 hash、地區範圍、卡數與排除清單、coverage、容量報告及尚未完成的正式閘門。
缺少 Q&A、勘誤、CR 或禁限來源時維持未知語意；`source_windows` 只用既有 complete／partial 或空窗口，不新增 unknown 列舉，也不因集合為空就宣稱 absent 或合法。
已知且適用的更正須套用；無法解決的 current 衝突排除受影響預覽閉包並列清單。

卡圖在預覽階段交付，先於 [M2-16／#43](https://github.com/gbaian10/sve-kit/issues/43) 與 [M2-17／#44](https://github.com/gbaian10/sve-kit/issues/44) 的 CI 引擎接線。
CI 使用測試 adapter 從正式最小快照推導原型需要的 legacy shape，不綁 DSL 1.0 遷移；移除對應 ignore 並實測行覆蓋率達 90% 是接線的驗收要求。
本里程碑先輸出本機；上傳 R2 仍須另行明確授權。

## 首發能力與引擎狀態

正式首發最低為 [implementation-tiers.md](implementation-tiers.md) 的 T0 40 表與必要 T1 17 表，共 57 表；各表的匯入器與驗證器都須完成。
`identity_change` 必須在第一次公開 `int_id` 前啟用，即使沒有修復事件也不能省略實作。
若納入現有 EN 登錄，另啟用 `region_mapping_review`、`art`、`region_text_review`、`region_divergence`，合計 61 表；**61 是含 EN 條件組的結果，不是無條件必建的數量**。
其他真實非空引用仍須完成依賴閉包；JP 預覽不為湊表數偽造 EN 資料，也不代表正式 57 表已驗收。
首發不啟用 `face_semantics`／`revision_semantics`／`semantic_reference`；現有身分分組不視為規則等義證明。

**R1 所有卡的引擎支援狀態都是 `missing_dsl`，包括已有原型程式的卡。**
依 [authored-layout.md §7](authored-layout.md#7-dsl-與拒絕輸入) 的正式候選入口及 `dsl/` 的 DSL 1.0 Schema，現有 `astra/1` 原型不屬正式候選。
因此依 [build-db.md §10](build-db.md#10-dsl驗證與未實作卡片頁) 的「沒有選定候選」規則，`dsl_id`／`dsl_version` 與引擎目標皆為 null，`automatic=false`，公開支援狀態附未實作理由。
公開 `automatic` 沿快照契約由 effective status 推導，不新增傳輸欄位。查卡介面顯示「未實作」，卡文仍可使用；此結果已定，不再要求確認是否接受。

## 待量測與待決定

- **正式首發地區**：由 [M2-05b0／#25](https://github.com/gbaian10/sve-kit/issues/25) 先交 EN 既有觀測的全體／可比對吻合率、缺來源與逐欄差異，再由使用者決定 JP-only 或 JP＋EN。EN 未完成不阻 A／B；若正式先 JP，EN 仍在里程碑內補齊。
- **G08：CI 真卡測試輸入方案待定**：不提交／私有 R2 bucket／私有 testdata repo／進 git 四種方案，於 M2-16 前由使用者決定，沒有預選方案。進 git 的提案上限為 500 張、UTF-8 input 合計 1 MiB、單檔 <512 KiB；上限不是准入批准。
- **current 真實衝突**：由 [M2-09a／#29](https://github.com/gbaian10/sve-kit/issues/29) 先列出來源、差異與可機械判定的結果；只有無法依既有政策判定的個案才交使用者選擇，不預先要求逐卡決定。
- **翻譯首輪批次與抽查投入**：到翻譯階段有候選與工作量時再決定；首版缺繁中翻譯時回退原文，不擋文字快照交付。

G08 決策前須核完整引用閉包（含 token、題本、AI、回放、輔助與 rejected 原文定位）、精確卡數、bytes 與來源，說明各方案對離線重建、fork PR CI 及權利說明的影響。
未決前不得將這批官方卡文放進 git，亦不得建立或上傳私有遠端資料庫作為既定方案；待決只影響相關測試資料交付與 CI 收尾，不阻 A／B／卡圖。
選「不提交」時須同步決定 CI 的資料取得與驗收安排，不能默認取消原測試或降低 90% 門檻。
