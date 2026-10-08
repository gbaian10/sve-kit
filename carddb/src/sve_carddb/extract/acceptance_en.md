# EN 整合驗收與差異重審

`acceptance_en.acceptance_report(text_plan, official_products, stores)` 組合
既有 06 身分、08 商品、09a 文字觀測與 09b 來源更正計畫。呼叫者必須先以
明確的封存批次與 parser pin 建立計畫，並選入 `en` 地區；JP＋EN 首發候選
使用 `regions=("jp", "en")`。這個函式不寫 authored、不配號、不建立人工決定。

報告對所有已登錄 EN printing 使用原樣卡號與全部 `source_face_map`，逐筆列出
永久 printing/card/face ID、兩個歷史／實際觀測 hash、來源完整 pin、
商品來源吻合與收錄、文字面與更正結果。只有兩個 hash、recipe、地區與卡號都
相同才列 `exact`；規則近似、規則 hash 單獨相同或缺欄位都不能通過。
完整 extractor 保留的 speech、auxiliary sections、數值與圖片仍透過原觀測
recipe 核對，不能為吻合率丟欄位。

| 分類 | 處理 |
| --- | --- |
| `exact` | 保留歷史身分；獨立執行文字對照、current、勘誤與發布閘門 |
| `mismatch` | 列出所有依賴該觀測的登錄記錄與原始／新 hash、來源 pin，待重審；不改舊 ID |
| `missing_raw` | 保留分母、永久 ID 與所有面對應；補明確封存來源，不退回 latest 或線上 |
| 解析／封存驗證失敗 | 既有 provider 直接失敗，不產生成功驗收報告；修 parser 或補正凍結輸入後重跑 |

本報告直接與 authored 的歷史觀測比對，不需要舊 JSONL 作來源；分母不排除
缺來源項目，可比對分母僅含 exact＋mismatch。

真來源更新時，先封存新版本、釘新 parser／批次，重新計算觀測，再交
`review_queue`。清單包含所有受影響的身分、圖片分組、跨區與 related 決定，
不限 printing 自己的決定；JP 端來源更新也會使 EN 對應待重審。人工確認後依
既有身分修復與決定續版契約追加記錄，再重跑相關計畫與建置。
缺歷史來源、parser 不支援與真來源更新必須分開診斷；hash 差異本身不證明
是哪一種，不自動撤回、更正或採納任何歷史決定。

本次實際來源用途保存於 inputs 摘要；原始 bytes 與來源 metadata 在讀取時驗證，
面索引與 owner 適用性在 plan 檢查。`populate_text_preview` 在同一交易填 DB 一次，
完成後直接保存該 DB，不再執行 build seal 或 expected 使用閉包重播。
只有身分／商品建置輸出不表示文字／更正 DB 匯入通過；plan 的 `applied`
也不表示更正已 materialize。`raw_effect_present` 保留 extractor 原始值；`effect_present` 是既有 presence
投影後的值，完整 `effect_presence` 證據與 raw／projected hash 分開列出。只有
已核可的 `effect-presence-v1` 證明 absence 才能投影空字串；raw 主文仍為 null
且 presence unknown 時，保留來源與觀測、暫不產生 revision，不自行補值。

目前 API 沒有接收 fresh `region_text_review` 的採納輸入，因此每筆 EN 都明列
`blocked_not_supplied`，報告永遠 `publication_gate=false`、
`snapshot_output_authorized=false`、`release_status=blocked`。`source_closure` 為
`planned_inputs_without_independent_closure_replay`，表示計畫用途摘要，沒有獨立閉包重播。
建置輸出存在與 exact 100% 都不替代該閘門；正式發布集合仍須由公開投影及
發布驗證器逐筆檢查可驗來源與引用閉包。此 API 也不將待決的文字排除提案
`plan.eligible` 當成發布白名單。
