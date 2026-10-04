# 模板翻譯的維護流程

本文件取代舊的首輪抽查及政策／收據採納門檻；格式以[翻譯契約](translation-contract.md)為準。

## 1. 一個資料 PR、一張清單

新卡包增量匯入來源，重產清冊並套用既有模板、參數、術語和譯文。
工具列低信心、新句型、未匹配、歧義與缺譯；撰寫者在同一 PR 修資料。
CI 自動驗格式、key、參數、引用及清冊重產結果；審核者審該 PR 的差異。
沒有每包必抽幾張、首輪高頻樣本門檻、不同模型逐筆 agreed 或合併前人工命令。
專案 AI 貢獻與本機審查流程仍依 AGENTS.md／CONTRIBUTING.md。

## 2. 信心與缺項

每筆來源類別只有 official/project/machine，另記 low_confidence Bool。
低信心且機械檢查通過的譯文直接顯示「待校對」，可切原文；不自動改成 project 或官方。
結構壞掉、參數無法對應與來源錯配不能渲染；缺譯及待補句型回原文，列同一張清單。
清單只需資料 ID、原因、影響範圍與建議，不保存私人提示詞、對話、點擊或核可頁 hash。

## 3. Legacy

`translation-policies/*.policy.yaml`、`.approval.yaml`、`.review-queue.yaml`、五欄 pin、
initial_sample_decisions、sample_ids 與 model_review 不再是新格式的輸入。
舊檔只供轉換取有效文字與來源類別；完成轉換後從當前 tree 移除，Git 歷史保留。
不把以前的審閱偽裝成針對新格式 bytes 的人員核可，也不另做替代收據。
