# ADR-0013：觀測雜湊與規則語義 bundle 的明示遷移

狀態：提案，待使用者決定（2026-10-01）。不可直接改標 recipe 或繼承 verified 的限制，已由 [build-db §14](../schema/build-db.md#14-不可變雜湊僅建置) 定義；本 ADR 提案其持久收據與遷移流程，不宣稱實作完成。

## 背景

face-bundle-v1 釘完整觀測，含 revision、原始文字、段落、數值與特性。它適合追溯當時看到的資料，但顯示表記改字也會改 hash。rule-bundle-v2 以不可變 semantics、規則數值與傳遞引用閉包追蹤規則依賴，允許已採納等義表記共用規則表示。兩者的證明對象不同，不能只換名稱。

既有身分登錄與分類模板只證明各自採納範圍，不構成 revision_semantics 或跨區等義審核。翻譯以 exact source/context 追蹤字句，不能把規則 bundle 當翻譯 freshness 的唯一鍵。

## 提案

保留 v1 觀測與其所有舊收據；新增 v2 結果與新收據，不覆寫任何歷史決定。來源 recipe、authored 封套 recipe、公開快照 format 與 DSL 語法版本分開：本遷移不改 DSL Schema，也不修改公開 format。

1. 釘住可重建的 raw、萃取器、來源更正、current/wording 採納及身分版本；缺原始來源不得以 hash 字串充當完成。
2. 建立完整 face_semantics、revision_semantics、semantic_reference 閉包。初始 exact 原文保留全部規則可走既定機械映射；移除提醒／重複 token、跨表記共用須有已核可政策或 confirmed 決定。unknown 段落、衝突定義、缺同區 token target 均不能輸出可驗 bundle。
3. 依 build-db §14 的原定 rule-bundle-v2 recipe 計算，記錄全部面、同區已採納 references 傳遞閉包及規則名稱；visited 集合處理循環，不遞迴自我 hash。保存來源觀測到規則表示的精確映射與決定，不能把新 v2 hash 寫回舊 v1 receipt。
4. 對依賴它的 DSL／題本／載入、機制與跨區核對建立各自新驗證。DSL verified 仍須既定獨立審核／巨集門檻及該引擎實跑證據；只完成轉換不是 pass。首發 missing_dsl 不因此升級，pending/disputed 也不被遷移消掉。
5. 全部引用與新證據一致後才讓本次建置選用 v2。失敗保持舊觀測及診斷，不在同一份 v2 文件混用 v1 source hash。舊快照及回放繼續釘舊內容；重新建置可選舊完整輸入，不覆寫歷史已發布事實。

持久遷移收據提案存於 repo 外、不可變的建置稽核輸出，隨 F1 建置輸入紀錄釘完整內容 hash；不新增公開資料表。每份恰含 `migration_format: 1, subject, old_recipe, old_hash, new_recipe, new_hash, input_record_hash, semantic_refs, decision_refs, checks, result`。

- subject 為 `{kind,id,version}`，kind 限 `dsl/mechanic/region_review`；version 精確指本次文件／核對版本，不能只指 current。
- old_recipe 固定 face-bundle-v1、new_recipe 固定 rule-bundle-v2；非 null hash 皆驗實際內容，blocked 且無法產生新 bundle 時 new_hash=null；passed 時兩者必非 null。沒有舊收據的初始 v2 建置不造假遷移紀錄。
- semantic_refs 為排序且去重的 `{face_id,region,semantic_id,rule_hash}` 完整閉包，decision_refs 為 `{decision_id,record_key,semantic_content_hash}`，均可從釘住輸入驗回。
- checks 為 `{kind,checker_version,report_hash,checked_at,result}` 陣列，kind 限 `closure/review/load/exam/region_alignment`，result=pass/fail；dsl 至少要求 closure/review/load/exam 全部 pass（exam 須覆蓋既定必測集合），mechanic 要 closure/review，region_review 要 closure/region_alignment；缺檢查、任何 fail 或依賴不 fresh 均為 blocked，不以某一種 pass 代替另一種。頂層 result=passed/blocked；passed 表示該 subject 的遷移要求滿足，不授予其他 subject 的權限。

收據與依賴來源須持久保存並可還原，不能只留下 console log；這是證據保存格式提案，尚未授權批量轉成 verified。

## 後果與適用邊界

只有翻譯／等義功能需要時才啟用 semantics 能力及完整驗證器；不回寫首發已取得等義證明，也不因本 ADR 建立假的 empty-pass。跨區 identity confirmed 與 region_text_review aligned 各自驗證；規則核對可重用等義表記的 bundle，卡名／效果／段落的顯示選用仍驗兩端 exact 版本。

已採納等義的純表記更新可保持 rule bundle，所以同規則 DSL 不需為顯示修字重跑；翻譯對新 exact source 仍須重建。規則數值、rules_names、normalizer、規則本文或 token 依賴改動則換 bundle 並重驗。來源正規化政策變更不能以「只是格式」跳過。

不採用「全庫更換 recipe 名」：它會製造未做過的驗證。也不以每次顯示修字全庫重跑為預設：依賴閉包可以精確圈出受影響者。獨立反例與定向突變見 [翻譯契約 §9](../schema/translation-contract.md#9-獨立反例與定向突變驗收)，尤其 V16、V18–V19、V23、V26–V28。
