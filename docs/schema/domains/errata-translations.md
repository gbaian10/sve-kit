# 勘誤片段譯文

`authored/translations/errata/` 的 YAML 以卡包分檔，保存公告中的舊日文與修正後日文的繁中譯稿。
`source_hash` 是 `SHA-256(canonical JSON([before_value, after_value]))`，並同時綁定
`source_url` 與 `source_date`。這些欄位來自 `card-data/errata/jp.json` 的已審查公告。
英文公告不作中文翻譯來源。

舊片段的錯誤或歧義須如實保留，不能在翻譯時先修正。`origin=machine` 不因審讀而改成 project；
`low_confidence` 標出卡名選詞、原文誤植或風味修辭仍需確認的譯稿。
本批依現行模板的用詞及翻譯風格 v1.14、term sheet DSL；歷史公告不具有現行模板要求的卡頁 owner 綁定，
因此不借用現行卡文的來源位置或把公告片段拼成完整卡文。

離線建置載入並驗證當次譯稿一次，在 configuration 釘住檔案 hash，寫入 authored source_record，
再以公告 URL 與前後原文的共同 hash 配對。日期不同則拒絕套用；未命中的片段維持原文。
`errata_change_translation` 引用已匯入的變更、語言與譯稿來源，保存前後譯文、origin 與 low_confidence。
公開投影內嵌 `wording_translations`，不新增實體印刷卡文或 current 裁定。
