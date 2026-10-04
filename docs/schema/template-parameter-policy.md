# 模板參數辨識規則

參數辨識是翻譯工具的功能，規則版本與修改交給程式／資料 PR；不另設核可收據或採納前置條件。
所有既有型別、語義角色、比對及未解參數清單保留，定義依[翻譯契約 §4](translation-contract.md#4-參數譯文與自動套用)。

## 1. 當前規則格式

`authored/template-parameter-rules/current.yaml` 完整欄位：
`{parameter_rule_format:2,kind:template_parameter_rules,rules}`。
rules 是按 rule_id 排序的 `{rule_id,enabled,origin,low_confidence,note}` 陣列，ID 唯一、enabled 為 Bool。
origin/low_confidence/note 沿翻譯 record 的意義；不用 policy_id、matcher_commit、condition_hash 或 approval。

rule_id 必須對到現行程式已登錄的具名規則；條件、適用語言／角色、優先序、型別與數值界限由該規則程式明定。
新增或改條件修改同一程式 PR 與必要反例，不允許輸入任意程式／正規式執行。
缺檔或未知規則失敗；空規則集合明示空陣列，不等於自行啟用全部規則。
規則可直接修正或停用；整體輸出清冊由 CI 重產比對，不逐規則重走核可頁。

## 2. 必須自動驗的行為

- 只在真正匹配的位置產生參數；數字相同不證明語義角色相同。
- uint／literal／reference、界限、符號、來源位置及 raw roundtrip 正確；未知角色保持未解。
- 術語、卡名、vocabulary 引用存在且不歧義；不把名字字串當 card ID。
- 全來源片段都有去向，body／reminder／token_header／layout 分開；風味不走效果正規化。
- 改規則時比較新舊匹配位置、值及未解清單，不用總筆數相同代替正確性；不要求結果永遠只能增加。

## 3. 舊格式轉換

format 1 的 policy／approval 配對、事件三元組、原八條授權橋接、samples、限制收據與歷史重播只供轉換。
將原規則實際條件及排除保留在現行程式與 current.yaml，不從「曾經核可」推論可擴大比對範圍。
純資料修改不需要歷史 main 祖先或舊環境重播；來源歸檔的保存責任不變。
