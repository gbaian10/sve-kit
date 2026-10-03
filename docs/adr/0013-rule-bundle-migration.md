# ADR-0013：觀測雜湊與規則語義 bundle 的明示遷移

狀態：已採用技術方向（2026-10-01），尚未實作。沿 [build-db §14](../schema/build-db.md#14-不可變雜湊僅建置)，不改 DSL Schema 或公開快照 format。

## 背景

face-bundle-v1 釘完整觀測；rule-bundle-v2 釘不可變規則表示、數值／特性與傳遞規則引用。證明對象不同，不能換 recipe 名稱就沿用 verified。既有身分登錄與模板分類也不構成語義等義證明。

目前 face-bundle-v1 沒有已實作的驗證實例或 production 收據，因此不先建立專用遷移收據格式、儲存區或假紀錄。初次 v2 建置直接遵循完整來源與驗證契約，不稱為已完成 v1 遷移。

## 決定

需要語義能力時，先從釘住的凍結原文、來源更正、身分與 wording 採納建立 face_semantics/revision_semantics/semantic_reference。初始 exact 原文可作機械映射；跨表記等義、移除提示或重複 token 須既定核可政策或 confirmed 決定。unknown 段落、缺同區目標、漏面或矛盾規則不產可驗 bundle。

依 build-db §14 計 rule-bundle-v2，記完整面、規則名稱與同區 references 傳遞閉包；循環用 visited 集合處理，不遞迴自我 hash。DSL／題本／載入、機制與跨區對照各自通過所需驗證，不能借一項 pass 提升其他功能；missing_dsl 不因新 hash 變 verified。

若未來遇到真正的 v1 驗證物件，保留舊觀測／決定與歷史快照，建立新語義對應並重驗一次。以該實例的來源、舊新 hash 與驗證需求先補收據保存契約及反例，才啟用遷移；不能在尚無可驗收據時保留舊 verified。無需為未發生的遷移另外設永久物件庫。

## 後果

純已採納表記更新可維持規則 bundle，翻譯仍依 exact source/context 重算；規則本文、數值、特性、rules_names、normalizer 或 token 規則依賴改動須重驗。identity confirmed 與 region_text_review aligned 獨立，規則等義也不代表新版顯示字句已核。

來源與人工採納依原歸檔／authored 契約保存，推導結果由對應 F1 輸入重現。只在翻譯／等義功能需要時啟用完整語義能力，不回寫首發已有等義證明。反例見 [翻譯契約 §9](../schema/translation-contract.md#9-獨立反例與定向突變驗收)，尤其 V16、V18–V19、V23、V26–V28。

公開 CDN 的快照／圖片保留依 [ADR-0016](0016-snapshot-retention.md)；本 ADR 的本機重播／建置證據要求不構成永久歷史下載承諾。
