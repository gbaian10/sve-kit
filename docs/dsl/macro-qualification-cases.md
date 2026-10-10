# 框架 DSL 巨集資格固定案例規格

本文件與 [macro-qualification-cases.json](macro-qualification-cases.json) 是 [#497](https://github.com/gbaian10/sve-kit/issues/497) 交付的固定輸入／預期，供後續 carddb 資格 consumer 驗收使用，**不是測試程式或已通過紀錄**。規範依[作者語法 §1／§10](author-syntax-1.0.md#10-巨集規範)、[ADR-0010](../adr/0010-version-meta.md)與[四層契約 §7／§8](../schema/domains/four-layer-translation.md#7-規則投影接口)。

資料僅含自撰合成識別與資格事實，不含官方卡文、不裁定個別卡片、不授予既有候選資格。frame／卡名概念／scope／模型 ID 均為合成登錄，後續測試須在隔離資料根建立。資料中的欄位是測試輸入的邏輯事實，不新增 production 表、meta 或作者引用語法。

## 1. 讀取方式與輸入

頂層恰含 `format:1,kind:macro_qualification_cases,fixtures,cases`。fixtures 是具名物件，cases 是有序陣列；每筆恰有 `{group,id,operation,input,expected}`，完整案例 ID 為 `group/id` 且唯一。operation 為 `qualification` 或 `use_review`，是測試責任名稱，不要求同名 production 函式。

input 恰含 `fixture` 與 `changes`。先深複製指定 fixture，再依 changes 的 JSON Pointer→值替換已存在的欄位／陣列元素；空 changes 表示原 fixture，不隱含其他預設。陣列可整個替換。不得改 expected、忽略輸入或只以 JSON 可載入宣稱 consumer 通過。

每個 fixture 的完整輸入如下。未標可 null 的欄位皆必填且不可 null；字串識別非空，布林不接受整數，印刷 ID 僅供重印／異圖去重反例。

| 欄位 | 型別與責任 |
| --- | --- |
| candidate | `{frame_id,semantic_variant,projection_kind,body_version,applicability_scope,written_by,version_registered,scope_registered}`；識別為字串，body_version 為可 null 字串，只有 resolved ability_body 候選有本體版本，其餘不假造本體；兩個 registered 為 Bool，表示版本／適用域登錄的已驗事實 |
| candidate.semantic_variant | 四層契約的 `{state,key,scope}`；resolved 時 key 非空且 scope=null，pending 時 key=null 且 scope 為自身精確 OccurrenceKey |
| uses | `{frame_id,semantic_variant,projection_kind,card_name_concept,evolved,printing_id}` 陣列；evolved 為 Bool。來源 binding、卡名概念與面歸屬已按四層契約驗妥，不能以此 fixture 跳過來源驗證 |
| reviews | `{model,body_version,applicability_scope,verdict,current}` 陣列；model 為模型識別，verdict=ok/revise，current 為 Bool，表示來源／接口等依賴仍有效的已驗事實，不能由作者手填 current 冒充驗證 |
| boundary_test | 可 null；非 null 恰為 `{body_version,applicability_scope,result,current,covered_boundaries}`。result=pass/fail，current 為 Bool，covered_boundaries 為唯一字串陣列 |
| application | 可 null；qualification 案例為 null；use_review 時為下節的逐用途檢查事實 |

frame_id 為完整 `frame:`＋SHA-256 形狀；合成 fixture 視為已驗且存在的 frame 登錄，不聲稱其 hash 是從官方來源算出。
frame hash 含語義變體與 projection_kind，因此同一 frame_id 在全檔只對應一組 semantic_variant 與 projection_kind。
所有 resolved key、projection_kind 與 pending scope 沿四層契約，錯來源／引用／型別由該契約拒絕，不以忽略壞列取得資格。

applicability_scope 引用合成已審範圍 `fixture.scope`；`fixture.other_scope` 是另一個不適用的範圍。這批案例只測精確登錄範圍的適用與不適用，不定義新的範圍包含算法。written_by 與 model 採測試用模型識別，沒有真實審核者或核可紀錄。

## 2. 輸出與判定責任

qualification 的 expected 恰含 `count:UInt,threshold_eligible:Bool,qualified:Bool`。

count 為同一精確 frame、同一 resolved 語義變體且 projection_kind=ability_body 的 uses 中，不同 `(card_name_concept,evolved)` 鍵的數量。候選本身不是 resolved ability_body 時 count=0；pending 必須保留自身 scope，不跨來源合計。重印、異圖及重複用途不增加 count；不同變體或不同 frame 的合法來源用途各歸自己的候選，不因表面文字相同合計。

threshold_eligible 恰為 count≥3；qualified 還須同時具備：

1. 候選版本及適用域均已登錄。
2. 至少兩個不同模型的獨立 ok 審核，模型皆不同於 written_by；每筆都對候選本體版本／適用域有效且 current=true。重複同模型不算第二模型，過期或不適用的審核不能補足數量。
3. 同本體版本／適用域、current=true 且 result=pass 的邊界測試，涵蓋 zero_cards、no_legal_targets、large_values、timing、information 五項責任。

同版本同範圍的有效審核可在 2→3 張時採計；本體或適用域變動後，舊紀錄保留但不作新資格。current=false 的案例只表達已失效的事實；每個 frame 用途依賴須包含 interface_key，建置重算不符即失效接口連接、用途審查與實跑資格，不重鍵有效來源的翻譯。鍵的存放承載及 stale 比較位置由 #498 後續實作定，本檔不指定欄位或表。

use_review 的 application 恰為 `{frame_variant_match,in_scope,typed_params_match,source_roles_match,roundtrip_match,roundtrip_from_dsl,unresolved_semantics,engine_run_passed}`，皆為 Bool。三項機械檢查依作者語法 §10.2；roundtrip_from_dsl=false 表示回放儲存的原文，不能算反譯。engine_run_passed 表示當前 exact 版本的有效實跑證據，不是 YAML 作者的宣告。

use_review 的 expected 除三個資格欄位外，還有 `reviewed_by_macro_checks:Bool,requires_independent_review:Bool,verified:Bool`。只有 qualified 且精確 frame／variant、適用域、typed 參數、來源角色、由 DSL 產生的反譯全部通過且無未解語義時，reviewed_by_macro_checks 才為 true；否則 requires_independent_review=true，不表示獨立審核已完成。verified 還需 engine_run_passed=true；本 fixture 不另提供獨立審核的通過證據。

## 3. 覆蓋矩陣

| 組 | 固定輸入／邊界 | 預期 |
| --- | --- | --- |
| MQ-001 | V1 使用 A、B；V2 使用 C，各自查詢 | V1 count=2、V2 count=1，均頻次不足 |
| MQ-002 | A 未進化的三個印刷（含異圖），加 B 未進化 | count=2，頻次不足 |
| MQ-003 | A 未進化、A 進化、B 未進化，同一有效變體 | count=3、threshold_eligible=true；其餘資格完備才 qualified |
| MQ-004 | 上例缺一模型審核、缺測試、測試失敗或漏邊界 | count=3、threshold_eligible=true、qualified=false |
| MQ-005 | 同模型重複、作者自審、revise、舊版本／異域／失效審核與測試、未登錄 | 頻次足夠仍不得 qualified |
| MQ-006 | 0／1／2 張；2→3 保留本體與有效審核；重複用途／不同 frame | 精確去重與隔離；只有補足第三卡且資格仍有效才 qualified |
| MQ-007 | ability_flag／card_field／none／pending；resolved frame 的其他 pending 來源 | 非本體及 pending 不累計；pending 與 none 分開保存 |
| MQ-008 | 合格巨集逐用途三項檢查、超域、未解語義、原文回放、未實跑 | 資格不代替用途 reviewed／verified；任一失敗退回逐用途審核 |

本體存放不受以上 count 或 qualified 阻擋；低頻本體與用途仍負獨立審核責任。後續 consumer 須透過實際 public reader／builder／資格入口讀取這些固定事實，並另驗真實來源及 DSL 反譯、邊界執行與引擎實跑；這裡不以合成 pass 取代那些測試。
