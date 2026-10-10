# 純英文卡的整欄顯示特例

本入口由 #500 實作，沿 [翻譯契約](translation-contract.md) 的 JP 優先政策；
只有已確認無同卡 JP 來源的 EN 卡可使用。換皮關係 `same_rules_reskin` 不自動變成同卡來源。
此處的 target 是顯示譯文，不建立 EN normalizer、Frame、SourceBinding 或 DSL 規則資格。

## authored 入口

沿用 `format: 3, kind: translation_shard, records: [...]` 封套，放在
`authored/translations/overrides/english/<sequence>.yaml`。
兩種 record 都使用既有 `origin`、`low_confidence`、`note` 品質欄；origin 限 project／machine。
reader 拒絕多餘欄位、重複鍵、錯型別與不存在的 target／概念引用。

| kind | 選擇鍵 | data |
| --- | --- | --- |
| english_exception_target | id,lang | `{id,source_hash,lang,references,nodes}` |
| english_exception_use | owner,field,ordinal | `{source,card_id,face_id,target_id,identity,reason}` |

target 的 id 是 Code，lang 固定 zh-Hant，source_hash 是不帶前綴的完整 SHA-256，
釘 EN 整欄原文字串的 UTF-8 bytes。相同來源與相同語義使用同一個 target；
不同 EN 寫法保留各自來源，不以同卡或近似文字互相借用。
同字串不同語義仍須分 target，由身分與引用審查確認，不能按 hash 自動合併。

`references` 是 slot Code → [具名概念引用](four-layer-translation.md#4-有型別葉槽與值) 的物件。
卡名引用用 card_name；其他 glossary 引用不能冒充卡名；vocabulary 必須是本次建置的 active 成員。
必要葉引用由人工在完整來源中確認後列入，不得改成 Literal 迴避概念選詞。
這不是一般可填任意參數的 EN 模板。

nodes 是非空陣列，只接受既有 `{kind: Literal,text}` 與 `{kind: LeafRef,slot}`。
每個已宣告引用都必須被使用，LeafRef 不可指不存在的 slot；可重複使用。
Literal 保留原樣。LeafRef 使用當前具名中文選詞；沒有可用選詞時整欄退回 EN。
target 位置由渲染節點依 Unicode code point 產生，沿既有 AnnotationSet 檢查與公開 reader；
不接受人工填入譯文 offsets。

use 的 source 使用 [SourceDescriptor](four-layer-translation.md#6-sourcebindingtrace-與逐-occurrence-位置)：
owner、field、ordinal、source_unit_id、source_hash 與完整 source_ref。
source_ref.parser 固定 translation-en-v1，指向凍結頁面的整欄；持有者只可為 face_revision／printing_face，
欄位只可為 name／effect／section／flavor，並遵守原 OwnerField 的欄位與 ordinal 規則。
card_id 與 face_id 明示本次身分結論的卡與面，不能以另一個文字相同的持有者代替。
identity 是 confirmed_no_jp／unresolved，reason 必須非空白，記錄目前來源的人工結論依據。
每個 printing 依自己的來源記錄選用；目前版次與印刷欄位分開，不借用彼此的來源版本。

## 選用與失敗語義

正常離線建置從同一份四層 authored closure 讀取；不需要另外的建置命令。
結構、必要引用或 target 來源宣告彼此矛盾時拒絕載入／建置。
合法但不再適用的 use 保留在報告，不啟用譯文：

| 情況 | fallback reason |
| --- | --- |
| 未定，或最新 region_mapping_review 不是 confirmed_none／缺紀錄 | unresolved_english_identity |
| card 的身分尚未 confirmed | unconfirmed_identity |
| 同卡已有 JP printing／目前 JP 面 | has_jp_source |
| 凍結來源、hash、語言、來源版本、locator 或持有者整欄不符／不可取得 | english_source_mismatch |
| 明示 card／face 與來源持有者不同 | english_owner_face_mismatch |
| 版次不是 EN 的目前版次 | english_source_not_current |
| 必要引用沒有可用中文選詞 | missing_english_reference_translation |

只有 use 的目前人工無 JP 結論、既有永久身分與來源檢查都成立時才選用。
歷史 absence 紀錄本身、unmapped 或只有 EN printing 均不足以啟用特例。
有 JP 的案例繼續由正常 JP 管線提供 jp_source；不以 EN 特例覆蓋其個別段落。

譯文以既有 translation／translation_use／translation_selection 輸出，basis 為 own_source，
authority 為 unofficial；品質合併 use、target 與必要引用的選詞，任何一項 low_confidence 都保留待校對狀態。
缺譯仍保留整欄 EN 原文。此入口不提供 EN 原文概念位置，所以不產 EN 原文 annotation，
不搜尋中文反推位置，也不套 JP offsets；中文 LeafRef 的範圍則可正常顯示與開關加粗。
報告只列數量與原因，不含官方卡文。

移除 use 即撤回特例，重建後回到 EN 原文；原始來源與永久身分不變。
