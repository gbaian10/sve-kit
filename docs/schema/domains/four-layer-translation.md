# 四層翻譯共用資料契約

本文件是 [ADR-0016](../../adr/0016-four-layer-translation.md) 的共用契約；#498 已實作 N0 的 reader、DDL、來源 binding 與渲染。
區域／階段葉、可選範圍、省略持有者與 NC 全名等價等 N1 行為由 #380／#499 承接；本文不是 DSL 語法。
用語依[術語表](../../../GLOSSARY.md#四層翻譯)；來源選用依[翻譯契約](translation-contract.md)。
本契約取代固定整行模板的參數／target 模型，保留整行粒度與必要的來源完整性檢查。
公開欄序、版本與 reader 拒絕規則由 [#496](https://github.com/gbaian10/sve-kit/issues/496) 定義；
N0 實作及測試已由 [#498](https://github.com/gbaian10/sve-kit/issues/498) 完成，整庫語義與中文重建由 [#499](https://github.com/gbaian10/sve-kit/issues/499) 承接。

## 1. 型別、驗證責任與失敗語義

下文物件均為封閉結構：未列欄位、重複鍵、錯型別拒絕；未標 `?` 的欄位必填且不可 null。
`T?` 表示**必填、可 null**，不是可省略；僅明寫「可省略」才有預設值。空集合使用 `[]`／`{}`。
`Code` 是非空 ASCII `[a-z][a-z0-9_.-]*`，`ID` 是對應登錄的非空識別，`Hash` 是 64 位小寫 SHA-256。
`TypeName` 是封閉型別名稱，ASCII `[A-Z][A-Za-z0-9]*`，只接受該 normalizer 版本登錄的名稱。
它不是 `Code`；兩者不互相轉換，也不自動映射大小寫。
`UInt` 是 0..9007199254740991 的整數且不接受 Bool，`Ordinal` 是 UInt，`Lang` 沿建置 DB 的語言登錄。
`Span={start:UInt,end:UInt}` 以 **Unicode code point** 計數，半開區間，要求 start < end；空字串不造零長 span。
陣列的排序、唯一與非空條件在各欄明定，不能用 JSON 的物件順序代替來源順序。

| 驗證者 | 責任與拒絕時點 |
| --- | --- |
| A：authored reader | 讀當前輸入一次，驗版本、封閉欄位、型別、鍵唯一、target 語法、可在輸入閉包判定的引用；裁定明示 revision／兩種 level 的封閉形狀；壞資料拒絕載入 |
| B：建置 validator | 從固定來源驗 owner／face、hash、角色、trace、葉值、適用域、完整覆蓋及使用閉包；裁定原引用、ordinal、已知舊域完整性及未知舊域資格；已知錯配拒絕建置交易，不任選或靜默忽略 |
| C：DB 邊界 | 寫入及讀回時驗 SQL 的 PK／FK、nullable、enum、Json 的具名型別及跨列一致性；原始 Any 不得流出邊界；裁定沿 SQL PK／FK 與逐列封閉型別；邏輯唯一／互斥及完整用途集合由 #499 接上；不重跑舊 normalizer；FL-027 |

每張表的「失敗／案例」列給出可判定原因；案例 ID 指向[固定案例](four-layer-cases.md)。
尚未辨識的合法來源用 pending／未匹配清單保存；缺譯用整欄原文退回。
這兩者可完成建置，但**壞引用、錯來源或無效 target 是錯誤**，不能以 pending 或 low_confidence 放行。
§9 的引用層級 unknown_legacy_scope 是保存固定歷史裁定原始引用的明示分支。它沒有有效模板／frame FK 的宣告，不構成壞引用可以啟用的例外。其裁定版本、檔案來源、hash、ordinal 與原始引用必須可驗；已宣告的來源／命名空間錯配仍拒絕。只有舊適用域證據不足時可保留，且不得產生 active 目標或進入可執行閉包。這個 nullable 分支只屬於 RulingResolution，不放寬 SourceBinding、Mapping 或 Frame.semantic_variant 的 OccurrenceKey。

NP 沒覆蓋的修飾可以用已寫好的 Literal＋LeafRef 翻譯；必要葉值未解時仍整欄退回，不能把它改成 Literal 躲檢查。

## 2. authored 入口與替換界線

authored frame 未在本次建置的完整來源欄位被使用時，不使整批建置失敗。
建置報告列出 frame ID 與 no_fully_verified_source_owner_field；不啟用該 frame／target，受影響欄位照常整欄退回原文。
壞結構、引用、明示 manual match 的過期來源與無效 target 仍拒絕。

四層翻譯分片採 `format:3,kind:translation_shard,records:[Record]`。
三欄皆必填且不可 null：format 是固定整數 3、kind 是固定字串 translation_shard、records 是可空陣列；
A 驗封閉形狀與全入口唯一鍵，缺欄／錯版／重複鍵拒絕（FL-009／FL-010）。
正式翻譯入口只讀此 format 3；reader、authored 資料與建置端同步切換，不建立 format 2／3 永久雙軌。
parameter-rules 的 format 2 與風味的獨立封套不隨之升版。
沿[翻譯契約 §2](translation-contract.md#2-當前資料入口)的安全路徑、分片大小與排序規則；不新增 includes/checksum index。

| Record 欄位 | 型別／必填與合法域、引用 | 驗證者；失敗／案例 |
| --- | --- | --- |
| kind | Code；限下表及既有非模板 kind 白名單 | A；未知 kind，含 source_exception，拒絕；FL-010 |
| data | 對應 kind 的封閉物件 | A；錯欄位／型別；FL-010 |
| origin | 可省略，official/project/machine，預設 project | A；非法來源類別；FL-010 |
| low_confidence | 可省略，Bool，預設 false | A；非 Bool；FL-010 |
| note | 可省略，Text，預設空字串，不入語義 hash | A；非文字；FL-001 |

`record_key` 仍是 `[kind,...選擇鍵]` 的 canonical JSON 字串，由 reader 算出、不存檔；全入口唯一。
glossary_term／choice／emphasis、symbol_localization_choice、context_assignment、card_name_concept、
glossary_choice_variant 與 translation_override 沿原欄位與來源規則；後者只能選可重用的 target／選詞。
`sentence_template` 的 kind 與 DB 名保留，本文稱其中的新資料為 Frame，避免另造同義身分系統。

| kind／路徑（相對 authored/translations） | 選擇鍵 | data 的完整形狀／差異 |
| --- | --- | --- |
| sentence_template；templates/definitions | id | §3 Frame authored 欄位；移除舊 normalized_hash、parameter_schema，改 source、leaf_schema |
| template_translation；templates/values | template_id,lang | `{template_id:FrameID,lang:Lang,target:Target,expression_targets:{Code:Target}}`；map 可省略為空；variant 另以 variant_key 選完整稿件，A/B/C 驗引用與完整使用，FL-005 |
| template_translation_variant；templates/values | template_id,lang,variant_key | 上列加 `variant_key:Code`，不可 default；不是 semantic_variant，FL-001／FL-005 |
| template_translation_candidate；templates/candidates | source_kind,candidate_id,lang | 保留未啟用的七欄草稿，依翻譯契約 §2.1；不能被 LeafRef／pin 引用，FL-029 |
| translation_form；forms | id,lang | §5 FormDefinition；封閉文法表，不保存官方全文，FL-012 |
| template_match；overrides | context_key | `{context_key,source_hash,matches}`；matches 為 null 或 `{frame_id,source_span,values}` 陣列；values 依 §4，A/B 驗唯一匹配、來源與全覆蓋，FL-007／FL-008 |

新增 `forms/<sequence>.yaml`；definitions／values／candidates 及既有 glossary／overrides 路徑不改。
template_match 不容手填未驗來源值；null 僅撤回人工指定，回本次自動匹配。
context_key.variant 指 context_assignment 的具名 variant（缺省 default），不是 §8 建置後的 context 聚合鍵。
source_exception 整個 kind 移除，divergence/default_jp 都沒有新入口；切換必須列出舊記錄及處置，遇遺留記錄拒絕，不靜默跳過。
純英文卡另用 [EN 整欄顯示特例](english-exceptions.md) 的 english_exception_target／english_exception_use，
不屬於 Frame 或一般每卡覆寫，不復活 source_exception。
舊模板與裁定重鍵依 §9；無法綁定的原稿仍是 candidate。永久 card／face／printing／int_id、配號游標、人工日英對應與歸檔 bytes 均不改。

## 3. Frame 與語義身分

Frame 是整行的語義定義，句中 reminder 可拆出並以 anchor 接回；不是按每個句號自由拼接的翻譯片段。
同形固定字若有選取時點、費用／效果、能力作用域等差異，必須分 semantic_variant。

| authored 欄位 | 型別／合法域與引用 | 驗證者；失敗／案例 |
| --- | --- | --- |
| id | FrameID，`frame:`＋完整 content_hash | A/B/C；格式或重算不符；FL-001／FL-002 |
| source | `{source_lang,canonical_hash,normalizer_version}`，如下三欄 | A/B；來源描述不完整；FL-007 |
| source.source_lang | 固定 ja | A/B；EN 冒充 JP 拒絕；FL-023 |
| source.canonical_hash | Hash；建置重建 canonical_source 的 UTF-8 hash | B/C；模式錯配；FL-007 |
| source.normalizer_version | Code；本次具名、釘版且受支援的 normalizer | A/B；不支援拒絕，版本變更重鍵；FL-002 |
| role | body/reminder/token_header/layout/name/label | A/B/C；角色錯配；FL-006 |
| semantic_variant | `{state,key,scope}`，依下段 | A/B/C；未解卻共用、分類錯配；FL-002／FL-006 |
| leaf_schema | `{format:2,slots:[LeafSlot]}`，§4，依首次 canonical 位置排序 | A/B/C；重名、域或角色錯配；FL-004／FL-005 |
| projection | §7 的 Projection | A/B/C；未知當 none、介面越 scope；FL-006／FL-022 |
| content_hash | Hash；完整語義 payload hash | B/C；同 hash 異 bytes 必失敗；FL-002 |

`semantic_variant.state` 是 resolved/pending；`key:Code?` 在 resolved 時必填非空，pending 時 null；
`scope:OccurrenceKey?` 在 resolved 時 null，pending 時必指自身精確來源 occurrence（§6）。
resolved key 引用具名且釘在 normalizer_version 的分類規則；該規則須明列來源語法、必要上下文與語義判別，不能是任意註解。
projection_kind=pending 時 semantic_variant 也必為 pending；分類原因列本次 build_issue，不假造 resolved key。
例如 play_targets_effect 與 resolution_choose_effect 分開，cost_material 與 effect_material 分開。
未知角色可以用保守精確 frame 翻譯，但 pending scope 限本 occurrence，不跨來源湊巨集數量。
新來源能確認變體後重鍵；不以顯示譯文相同推論分類。

此承接只限 JP `source_correction`：來源更正 revision 的 ID 含 `correction_keys`，
更正計畫改動即會重鍵，且可能恢復較早版本的原文；JP 是四層效果翻譯的來源語言。
一般 wording／errata 版本及 EN 官方原文不因文字相同而擴大 pending scope 的適用域。
這類 revision 可承接同卡面、同地區較早版本的譯文：先驗完整欄位原文、
卡片資料與分類結果相同，再把 pending scope 改為更正版本自身的 occurrence 並重算 frame ID。
依 revision 編號由近到遠選第一個可完整驗證的來源版本；來源綁定仍各自核對凍結來源與更正證據，
不共用舊 scope。預設與具名譯文、人工匹配、選用及停用規則沿用原上下文；
衍生 frame 與譯文保留原 authored 檔案及 record_key 的來源紀錄，不改 current 選擇。

建置才持有 `canonical_source:Text`，由完整 trace 重建，不抄官方字串進 authored。
ID 使用 build-db §14 的 canonical-json-v1，payload 恰為：

```text
{recipe:"frame-v1",source_lang,canonical_source,normalizer_version,
 role,leaf_schema,semantic_variant,
 projection:{projection_kind,discriminator}}
```

projection 只有 `projection_kind` 與 `discriminator` 入 frame hash；`scopes/imports/exports` 由 §7 的 interface_key 追蹤。
來源語義相同時，補足或調整 DSL 接口描述不應使譯文、binding 與裁定映射重鍵；相依 DSL 必須重驗接口（FL-001／FL-016）。
這不容許用接口改動偷換來源語義：選取／付款時點、能力或分支作用域、跨句關係改變，
仍須反映到 canonical source、葉角色／域、semantic_variant 或投影分類，重鍵後依 §9 處理引用（FL-002／FL-022）。
未知來源語義轉為已確認也須按 semantic_variant 重鍵；只新增已知語義的 adapter 支援則不必。
DSL 本體／body version 不入 frame hash。
變體 key 的含義改變須改分類器版本，不能原 key 偷換語義。
normalizer_version 是全庫單一版本，同時釘住分段、Unicode／canonical、葉角色、域、單位、alias、省略及 resolved 規則。
不按規則族另釘版本；封版後改任一項語義均須新版本及全庫重鍵。N0 的 reminder／token_header 保留 raw Unicode，
不套 body 的 NFKC；所有角色共用同一來源 units／trace，分類器不能重新用另一套正規化索取位置。
葉的實值、來源卡片／頁面（resolved 時）、譯文、form、NP 分組、加粗與 note 不入 frame hash。
新增語義槽、改合法域／canonical pattern／正規化規則須新 ID；只開關 NP、修中文形式或改選詞不換 ID。
同 hash 必比完整 payload，碰撞即錯誤，不截短或覆蓋已有 frame。

## 4. 有型別葉槽與值

| LeafSlot 欄位 | 型別／合法域與引用 | 驗證者；失敗／案例 |
| --- | --- | --- |
| name | Code；同 frame 唯一 | A/C；重名或未知引用；FL-005 |
| type | TypeName；下表封閉型別名稱 | A/B/C；不可泛用 term 吞所有角色；FL-004 |
| role | Code；type 對應的具名語義角色 | A/B；同字不同角色不合併；FL-004／FL-021 |
| domain | `{values:[Scalar],min:UInt?,max:UInt?}` | A/B/C；引用值域或數值界限不符；FL-004 |
| required | Bool；true 必須綁值且被 target 使用 | A/B；遺失、Literal 偽裝引用；FL-005 |
| occurrences | 排序且不重疊的 canonical_source Span 陣列；一般非空，只有具名來源規則推導的省略槽可空 | A/B；越界、空槽沒有推導規則或角色不符；FL-008／FL-020 |

Scalar 只可為 UInt、Bool、Code 或下列具名引用。domain.values 是排序唯一的來源合法值描述；
數值型別用空 values 與非 null min/max，其他型別 min/max=null、values 非空；排序依元素的 canonical JSON bytes。
Concept、CardName、CardKind 及複合型別可用具名域代碼，引用該 normalizer 版本的封閉 descriptor，不能執行任意 predicate。
N0 引用型別每槽恰用一個代碼，不將本次 catalog 的整個類別展開到 Frame；代碼固定引用種類、category／vocabulary kind、
有效性及來源構造限制。catalog 新增合格成員不改域含義；實際值仍須通過本次 FK、category、採用狀態及來源限制檢查。
合法域是來源限制，不能從 target 推導。target 可不顯示的槽 required=false；沒有來源用途的 optional 槽可以不綁值。
由具名來源規則實際產生的純省略槽仍須有值及 omitted occurrence，不用 null 造值。

| type／值 | role／合法域與語義界線 | 案例 |
| --- | --- | --- |
| Nat／UInt；Ordinal／1..安全整數上限 | count、damage_amount、counter_amount、repeat_count、threshold、choice_index 分角色；負號不是幅度值 | FL-004／FL-021 |
| Player／具名 Code | self/opponent/each 等由構造的 domain 明列；source omission 不能全域默認 self | FL-020 |
| ZoneSet／排序唯一 ZoneKind 陣列 | counted_zone/source_zone/destination_zone 分開；各元素引用型別登錄及其詞彙對照；deck top/bottom 是 ordered position，非 ZoneKind | FL-003／FL-017 |
| CardKind／vocabulary 引用 | counted_kind/filter_kind；vocabulary 的 kind=type，code 必存在 | FL-003／FL-004 |
| Concept／TermReference | keyword/trait/ability/rule_term 各自有 category 與角色限制；`{kind:glossary,key:ID}` 或 `{kind:vocabulary,key:[kind,code]}` | FL-005／FL-018 |
| CardName／`{kind:card_name,term_id:ID}` | 名稱概念 FK；整個名字是葉，不抽名字內的術語，不因同名猜 card／token 身分 | FL-013 |
| DeckPosition／具名 Code | source_position／destination_position；容器固定為 deck，完整牌堆＋端點片語由一葉持有，不與 deck ZoneSet 重疊 | N1 正式登錄 |
| Phase、Stat、TokenStatus／具名 Code | phase_trigger/phase_duration、stat_filter、token_filter 各自引用封閉登錄；token 不是 trait | FL-018／FL-021 |
| QuantitySpec／下述物件 | selection_count、existence_count、set_extent、repeat_count 等依構造限制 | FL-004／FL-019 |
| QuantityExpr／下述有限描述 | count_expression；符號 X、計數與算式不是一律 Nat，也不提早凍結 Live 值 | FL-019 |
| LiteralLayout／Text | 只允許 layout 來源的 exact 空白，不得吞規則文字 | FL-008 |

`QuantitySpec={mode,expr}`：mode=exact/up_to/at_least/all/any，expr 為 QuantityExpr?；
exact/up_to/at_least 必有 expr，all/any 必為 null。at_least 可表存在數量比較，不能因此新增 choose.at_least；
may 是 frame 控制語義，不等於 up_to 或選零個。random 屬 frame，不由數量推成玩家選擇。
`QuantityExpr` 恰為 `{kind:constant,value:UInt}`、`{kind:bound,import:Code}` 或
`{kind:expression,expression_id:Code,leaves:[Code]}`；import 引用 §7，expression_id 引用具名純運算描述，leaves 引用本 frame 槽。
最後一支只是保留來源表達式的接口，不授予執行能力；未支援表達式保留 pending，不強轉非負常數。
所有上述巢狀欄位由 A 驗結構、B 驗型別／使用處及來源、C 驗讀回，失敗按 FL-004／FL-019／FL-022。

來源語義角色與遊戲型別用一份 binding，renderer 不另抽一次值，DSL adapter 也不能從中文猜值。
Player／ZoneKind／Phase／DeckPosition／Stat／TokenStatus 等 Code 的顯示名稱，須在該型別登錄中明示 value→TermReference 的對照；
B 驗對照存在、型別吻合並沿同一選詞產生位置，不能用 Code 字串或中文猜概念（FL-005／FL-013）。
數值本身及純文法片段無概念名稱時不造 TermReference。
ZoneKind 本身不是完整目的 ZoneRef；後者還要持有者。union 的分支限定不能上提成全域 filter；
異種聯集須具名 sum type，group 的各組數量不能壓成單一 union。

### 4.1 N0 產生登錄

N0 原以全庫版本 `four-layer-jp-v1` 封版，該版已不再接受（見 §4.3）。一般契約保留未來型別，N0 來源分類只產生下列封閉組合，
不產 Player、ZoneSet、Phase、Stat、TokenStatus、獨立 QuantityExpr、跨 frame port 或 omitted occurrence。
所有 N0 槽 required=true、有 explicit 位置。未知整行可保留合法葉及 occurrence-scoped pending；必要葉未解則不能建立 active binding。

| type | role | 域與必要來源構造 |
| --- | --- | --- |
| Nat | count | 已辨識抽牌、棄牌、移動等動作的明示張數；0..安全整數上限。 |
| Nat | damage_amount、recovery_amount | 明示傷害／回復量；0..上限；回復的「回」不是次數。 |
| Nat | counter_amount | 具名指示物放置／移除的 exact 幅度；0..上限。 |
| Nat | repeat_count、duration_count | 已辨識執行次數／回合長度；0..上限。 |
| Nat | resource_amount | 明示資源量或付款量；0..上限；種類與付款／恢復時點留 canonical。 |
| Nat | cost_value、stat_value | 明示費用／攻擊／體力值及已辨識 header 欄位；0..上限。 |
| Nat | threshold | 主體、比較子、原始／目前值明確的費用、資源、指示物、種類數或生命門檻；0..上限。 |
| Nat | construction_copy_limit | 構築時同名卡片的張數上限；0..安全整數上限。主牌堆、進化牌堆或兩者各自的限制保持來源字面。 |
| Nat | construction_name_kind_limit | 構築時不同卡名的種類上限；0..安全整數上限。本版起始護符來源另限明示值 1，不等於只能放一張。 |
| Nat | arithmetic_multiplier、group_divisor | 已辨識倍數／每組基數；1..上限，不執行整個算式。 |
| Nat | stat_delta_magnitude、cost_delta_magnitude、resource_delta_magnitude、damage_delta_magnitude、counter_delta_magnitude | 明示正負號構造的非負幅度；0..上限；sign、種類及讀值時點留 canonical。 |
| Ordinal | choice_index、card_index、repeat_index、turn_index、deck_index | 唯一完整引介的選項／卡片／次數／回合／牌堆序號；1..上限；跨行選項只保留經完整 owner／欄位／能力作用域守衛的序號，不產 ChoiceContext port。 |
| QuantitySpec | selection_count、choice_mode_count、counter_amount、repeat_count | `quantity.<role>.constant.v1`；mode=exact/up_to，expr=constant；0..上限，由完整構造另收窄。 |
| QuantitySpec | existence_count | `quantity.existence_count.constant.v1`；mode=exact/up_to/at_least，expr=constant；0..上限，僅已辨識物件存在構造。 |
| Concept | keyword、ability、rule_term、trait | 原本已抽出的具名引用；分別用 `concept.<category>.v1`，category 必一致。 |
| Concept | class_filter、declared_class | 已明確的 class filter／header 欄位；`vocabulary.class.v1`。 |
| Concept | declared_trait | header 明示 trait；`concept.trait.v1`。 |
| CardName | declared_name、name_filter、created_name、name_reference | 完整 name／header、已確定 filter／生成，或受保護整名引用；`card_name.any.v1`。 |
| CardKind | counted_kind、filter_kind、declared_kind | 原參數已辨識的計數物、filter 或 header；`card_kind.any.v1`。 |
| LiteralLayout | layout | `layout.whitespace.v1`，只接受 exact、非空的來源空白。 |

| 引用域 | 本次 catalog 限制 |
| --- | --- |
| card_name.any.v1 | card_name 引用，term_id 有效且 category=card_name。 |
| concept.keyword.v1、concept.ability.v1、concept.rule_term.v1、concept.trait.v1 | glossary 引用，分別限制相同 category。 |
| vocabulary.class.v1 | vocabulary 引用，複合鍵 kind=class，code 已採用且有效。 |
| card_kind.any.v1 | vocabulary 引用，複合鍵 kind=type，code 已採用且有效。 |

`any` 僅用於上表列出的完整名稱／header 宣告、受保護整名 operand、已確定名稱 filter／生成，
以及無有限種類限制的計數物／filter／header 欄位構造；不授予整行來源等價。
若來源只容兩種卡種或其他有限集合，須另登錄該構造的有限域，不能用 any 放寬，亦不能把目前觀測成員當成來源上限。
引用實值保留在 binding；新增無關合格概念不改 frame、binding 或 cv。
區域、階段、未參數化能力全名／alias、玩家及 token 身分仍是 canonical 固定詞；不能新增 lexical 葉讓 target 反找位置。
原 braced 術語／圖示、header 欄位與整名參數仍須保留，不以「不新增 lexical 葉」刪除既有必要引用。

### 4.2 N0 整段語義構造登錄

N0 的 resolved key、來源 role、投影及能力作用域只由來源語義分類產生；B 以 v3 來源推導重放比對，不另設版本登錄。
未知 key、role／投影錯配或額外作用域均拒絕；未知完整語法維持 occurrence-scoped pending。

| key | role | projection_kind | 獨立 ability scope 數 |
| --- | --- | --- | ---: |
| metadata.name.v1、metadata.label.v1、metadata.layout.v1 | 各自的 name／label／layout | none | 0 |
| card_keywords.v1 | body | card_field | 0 |
| evolve_entry.v1、feed_entry.v1、ride_entry.v1 | body | ability_body | 1 |
| evolve_feed_entries.v1、evolve_ride_entries.v1 | body | ability_body | 2 |
| token_header.v1 | token_header | card_field | 0 |
| pure_reminder.v1 | reminder | none | 0 |

入口只分類自身完整動作、費用及能力頭；本版不引用轉換後的面，也不產 target-face port，
因此不要求 paired face 關係。來源自身仍須對上精確日文版次、從者種類與 normal phase；
不以 `official_unspecified`、卡號或名字推斷面關係。
組合入口的 scope 依來源順序命名 ability_0、ability_1，parent=null、kind=ability。
pure_reminder 必須有完整具名提示文法及同欄的精確 body anchor；token_header 必須位於 section 起點，
全部明示名稱、class、trait、kind 與數值均須唯一解析。
此登錄描述可產生的有限構造，不表示已完成全庫或正式 workflow 的來源驗收。

### 4.3 N1 正式來源登錄

`four_layer_derivation.derive_source` 使用正式版本 `four-layer-jp-v6`；正式 compile、authored reader 與 DB reader 只接受 v6，不保留舊版 reader。
N0 的來源構造與數值規則是 N1 明列的既有基礎；歷史對帳使用切換前已保存的 N0 建置資料，不從新 frame 反推舊版。
此入口沿相同 source units、分段、catalog、數值規則及來源驗證邊界產葉，不另建來源驗證器。
只擴充精確 face_revision／printing_face 的 effect／section 中 role=body 的部分；name、label、layout、reminder、token_header
及已 resolved 的完整進化／食事／憑依等入口保留 N0。未知整行可以新增合法葉，仍維持整行 pending。

明示 Player 一律用 `player.relative.v1={opponent,self}`，角色分 source_owner、destination_owner、counted_owner、phase_owner。
只由下列省略規則保證 self 時使用 `player.self.v1={self}`；不產 each、both、affected_owner、物件控制者或回合玩家。
小型 Code 域採 `CodeDomain(type,codes)`，codes 為排序唯一的非空列舉，不把裸 self 字串誤當 descriptor ID。
ZoneDomain 可附精確 combinations；N0 未附此欄位時沿用既有成員集合限制，不改既有 descriptor 的順序。

| 具名域 | 正式入口的精確限制 |
| --- | --- |
| zone.single.v1 | 恰七種單元素集合：battlefield、hand、deck、evolve_deck、graveyard、ex、banish。 |
| zone.hand.v1 | 恰 `[hand]`；固定加入／放回手牌的目的區域。 |
| zone.selection.battlefield_ex.v1 | 恰 `[battlefield,ex]`；只用於下列同質選取。 |
| phase.named.v1 | start、main、end；只產 phase_trigger，不產 duration。 |
| deck_position.edge.v1 | top、bottom；固定容器 deck，不產任意位置或 counted_position。 |
| token_status.token.v1 | token；只認明示 token filter，不由名稱猜。 |
| card_kind.selection.v1 | 本次已採用的 follower、amulet、spell 引用；禁止 leader／ep 等其他 type。 |
| card_kind.follower.v1／card_kind.amulet.v1／card_kind.spell.v1 | 固定單種類 descriptor；現有構造／unit 守衛要求單種類時使用，不能按觀測值縮域。 |

Code 的概念對照為 Player.self/opponent → `term:player.self/opponent`、Phase.start/main/end → `term:phase.start/main/end`。
ZoneKind 的 battlefield／hand／deck／evolve_deck／graveyard／ex／banish 依序對應
`term:zone.battlefield`／`term:zone.hand`／`term:zone.deck`／`term:zone.evolve_deck`／`term:zone.cemetery`／`term:zone.ex`／`term:zone.banishment`。
DeckPosition.top/bottom 對應完整 `term:zone.deck_top/deck_bottom`；TokenStatus.token 對應 `term:object.token`。
這裡記錄來源型別含義；renderer 自 v3 起已接 Code→Reference、必要引用集合與新形式，正式來源 workflow 使用完整 v6 來源推導。

P 為明示自分／相手，Z 為上列區域，DP 為完整デッキの上／下，PH 為具名階段。
NP 以卡種、泛稱カード或完整 `『』` 名稱為核心；Q 沿既有合法數量與單位。
動詞採有限拼寫，不使用通用形態還原。下表來源片段只描述文法，沒有官方整句。

| 登錄子族 | 接受的局部構造與葉角色 |
| --- | --- |
| N1-SRC01.select | `[Pの] Z(の/から) NP[を] Q[を]` 接選ぶ／選び／選んで；source_owner、source_zone、selection_count 及 NP filter。 |
| N1-SRC01.search／inspect／reveal | 同一來源構造接探す／探し／探して、見る／見て、公開する／公開して；不套 selection 助詞 alias。 |
| N1-SRC02.locative／ablative | `[Pの] Z(の/から) NP Qを [Pの] Zに` 接有限移動動詞；同子句、同 NP、明示目的地，來源與目的分槽。 |
| N1-SRC02.hand_cost | 完整能力頭（含【進化時】／【攻撃時】觸發頭）與成本分隔內的手札成本、Q、捨てる與冒號；必須在該能力冒號前，觸發後的效果棄牌或裸棄牌不是成本。 |
| N1-SRC03.place／return／add_hand／deploy | `[Pの] Zに` 接置く／置き／置いて／置いてよい；戻す／戻し／戻して／戻してよい；固定手札接加える／加え／加えてよい；固定場接出す／出し／出して／出してよい。只產目的角色。 |
| N1-SRC04.read／quantity／exist | `Zの NPの数/枚数`、`Zの NPが(ちょうど)?Q(以上/以下)?なら(使える)?`、`Zに NPがいる/いない/ある/ない(なら/場合)?`；counted_owner／counted_zone。比較、否定及既有數值角色不合併。 |
| N1-SRC05.arrival／delayed | `[Pの] PHが来たとき`，或 `[Pの] PHに` 接完整具體效果述語；phase_owner／phase_trigger，兩骨架不 alias。 |
| N1-SRC06.source.inspect／source.move／destination | `[Pの] DP Qを` 接見る／見て／公開する／公開して，或直接置入／放回明示區域；`[Pの] DPに` 接有限置く／戻す活用。完整 DP 葉先於普通 deck 區域；不接受 DPから。 |
| N1-SRC07 | 下表三條純省略規則；每個目的地／來源用途各一槽。 |
| N1-SRC08.kind／trait／braced／token／card | 只補 SRC01／02／04／10 已成立 NP 內的明示 filter；class 沿 vocabulary(class)，trait 沿 glossary，泛稱カード使用 `term:object.card`，不造 type=card。 |
| N1-SRC09.threshold／keyword／quick | 四種門檻能力名與數值分槽；合法正文 bracket keyword 與 braced Quick 使用既有可選規則。 |
| N1-SRC10 | 明示同一 P 的場與 EXエリア，以か／や、兩種順序、一個共同 NP/Q、一次選ぶ連接；重複相同 owner 或共同 owner。異質、不同 owner、分支數量或控制分支均拒絕。 |

新明示槽 required=true。純省略槽 occurrences=[]、required=false，binding 必有 self 值及恰一個 omitted occurrence。
來源缺 P 且未通過省略規則時不建 owner 槽，也不新增 fallback。所有 omitted 的 raw_spans／canonical_spans 均空，
沒有假 raw anchor 或零長 trace；邏輯次序由同版來源規則重建。

| resolution_rule | 守衛與固定邏輯使用點 |
| --- | --- |
| owner.actor_hand_cost.v1 | 完整本能力成本邊界；在明示手牌 source_zone 前，能力頭／先行成本之後。 |
| owner.self_hand_destination.v1 | 已知 self 的唯一局部來源集合：同子句選取／搜尋／公開→加入、同 raw 行選ぶ後緊接それを加入，或固定檢視→所見集合公開→加入骨架；在目的 hand 前。不得跨能力、分支、玩家切換或借未知／對手來源。 |
| owner.return_inspected_deck.v1 | 唯一自家檢視牌堆與剩餘集合；直接收尾或固定公開／可加入後收尾。在目的 DP 或整牌堆前；兩種目的地分列，不借開頭來源位置。 |

明示葉按首次 canonical 位置編序，省略槽接在明示槽後按 operand 使用點／角色編序。
matching 以完整槽簽章及穩定省略次序對應，不以共同空 span 任配。來源驗證沿同一 producer 重建，拒絕缺值、偽規則及跨 operand 借葉。
G10 的 zone occurrence 保留兩段 raw_spans；重複 owner 保留兩個明示 occurrence，可共同指向一個 canonical span。
同槽明示位置的排序唯一聯集必須等於 LeafSlot.occurrences；跨槽或 raw 重疊仍拒絕。

selection 的の／から與 `NPをQ選ぶ`／`NP Q[を]選ぶ` 歸一只限 SRC01.select；真正直接移動的來源助詞不 alias。
NC／ネクロチャージ、SC／スペルチェイン只在完整數字門檻頭歸一；コンボ／レッスン不新增別名。
四種能力名使用 typed placeholder，原數值 N 保留；一般 keyword 與 braced ability（含 Quick）保留 canonical 拼寫。
每個新葉可沿來源結果的 registry_rows 對回上列；沿用葉對回 N0。

名稱／提示文保護與事件、歷史、receipt、回合、主戰者持有者、靜態期間等父語境排除先行。
任何成對的「…」（含句點、冒號）都整段受保護，裡面不產外層葉；手牌費用與其他來源共用引號、父語境、並列阻擋及名詞起點檢查。
完整 G10 優先於單區；失敗的並列（含頓號、中黑直接並列的區域）不截右半邊；DP 優先於 deck；門檻能力優先於一般 keyword。
NP 內 trait 形狀或泛稱カード片段須恰好解析為一個 glossary 概念，否則整個 operand 不產新葉、保留 N0。
未解引號條件、其他、表裏狀態、數值限定及非 class 的大括號 token 保留在共同 canonical／trace 中，不穿入新增結構葉。
非 class 大括號即使有 N0 ability 葉仍是未解修飾，該 operand 只用 flat target。
外層合法 owner／zone／quantity／filter 仍可產葉；含未解修飾、多 trait 或泛稱卡片的 operand 使用 flat target，不能用 CardNP 吞掉限定。

結構規則沒有 YAML 開關。可選相依沿 `enabled`／`low_confidence`：數值量與四種門檻沿原規則，NC／SC 分別沿原 alias 規則，
一般 braced ability 與 Quick 沿 `braced_ability_reference`；新 `bracket_keyword_reference` 控制正文 bracket keyword。
停用或空選擇不旁路產葉／resolved，品質沿實際相依傳播。Quick 的完整 body 恰為 `{クイック}`，只在 effect 第一個非空 raw 行、
normal face、type_code=spell（含法術代幣）及精確 owner/context 成立時產 `quick_card_field.v1`；
整行只有已解析 keyword 時產 `card_keywords.v2`。非整行的未知或非 keyword 類別括號不產 keyword 葉、不新增 issue；
整行括號若只有唯一的非 keyword 類別概念，也不套用、不報 issue，保持 pending；真正缺概念或非 keyword 概念不唯一時仍保留 issue。
局部詞彙成功不授予其他整行語義。

## 5. target、form 與部分 NP

`Target={format:1,nodes:[Node]}`，nodes 為非空有序陣列，輸出依序串接；不支援任意運算式、正規式、HTML 或執行程式。
Literal 的內容就是文字，不再解析 `{{slot}}`；既有譯文切換時轉成節點，不保留兩種模板解析器。
card_name label 優先使用[完整名 term 的合格官方名](card-name-concepts.md#3-名稱-owner-的預設綁定)，沒有才用當前 glossary 選詞。
有合格官方名的 card_name term 不接受具名 variant pin，建置拒絕；default pin 使用有效 label。
官方 label 的 low_confidence 為名字政策 OR term 概念，不合併被替代且未使用的 choice 品質；
binding／template／form 的品質仍照實傳播，整句 authority 仍為 unofficial。
「整個名字是葉」指引用範圍覆蓋整個概念文字；EN 特例名稱可用片段 card_name 概念作為葉。

| Node 完整欄位 | 型別／引用與限制 | 驗證者；失敗／案例 |
| --- | --- | --- |
| `{kind:Literal,text}` | text:Text，僅專案譯文固定語句／標點；不可替代必要葉引用 | A/B；required 葉被藏入文字；FL-005 |
| `{kind:LeafRef,slot}` | slot:Code，引用本 frame LeafSlot；以基礎 label／數值呈現 | A/B/C；懸空或錯型別；FL-005 |
| `{kind:Form,form_id,args}` | form_id:Code，args 為該形式簽章要求的具名 slot 引用 map | A/B/C；缺形式、錯格／動詞簽章；FL-012／FL-017 |
| `{kind:NP,constructor,args}` | constructor=CardNP/UnionNP/CountExpr，args 依下表；只引用葉或有限子 NP，不另存 raw／值 | A/B；未知 constructor、重抽值；FL-001／FL-018 |

args 中每個葉引用固定 `{slot:Code}`；NP 子節點仍為上表 NP 物件。無環，不容任意 target 塞進 NP。
Node 的 kind 是上述四種唯一 discriminator；required 槽須由 LeafRef 或可追蹤的 Form／NP 路徑使用，
重排／重複呈現會產生各自 occurrence；省略未綁定的 optional 槽可接受，引用它則整欄缺值退回。

| NP 簽章（? 表示可省略，不能為 null） | 責任及未覆蓋時的行為 |
| --- | --- |
| CardNP(kind,quantity,owner?,zone?,traits?,class?,token?) | kind=CardKind，quantity=QuantitySpec，其他為相應葉引用；traits 為有序 Concept 引用陣列；只做詞序與形式，不執行選取 |
| UnionNP(branches,quantity?) | branches 為至少兩個 CardNP；只組合同元素型別且分支 scope 已明確者，quantity 是整體數量；異種或複雜分支可保留 Literal＋葉引用 |
| CountExpr(expr) | expr=QuantityExpr 葉引用；只呈現既有表達式結構，不建立新的 runtime count 指令 |

CardNP 的 owner=Player（role 限 source_owner 或 counted_owner）、zone=ZoneSet（role 限 source_zone 或 counted_zone）、token=TokenStatus；class 為 Concept 的 vocabulary(class) 引用，
traits 每項為 role=trait 的 Concept glossary 引用；class 的 role=class_filter。
owner 與 zone 同時存在時，source_owner 只配 source_zone，counted_owner 只配 counted_zone；
N1 來源驗證還須確認全部葉屬於同一合法 operand，不能跨用途借相同值；CardNP 必須涵蓋該 operand 的全部 kind／trait／class／token 篩選，不能把漏掉的 filter 在 NP 外另行引用以求通過。DeckPosition 用 flat target／Form，不冒充 zone。
引用域也可使用已登錄的 `concept.trait.v1`／`vocabulary.class.v1`。
目前 traits 限 0 或 1 項，不把 token／class 當 trait；多項篩選須先確認具名構造的 AND／OR 語義。

未覆蓋的長修飾保留在 target 的 Literal＋LeafRef／Form 中，**不建立 opaque NP 字串槽**。
NP 不另占來源 span；它的葉各自有來源位置。NP 移除或新增只改 render projection，不改 frame 或 DSL 值。
若語義本身未解，projection 保留 pending，不能用沒有 NP 推論沒有 filter 或沒有動作。

| FormDefinition 欄位 | 型別／合法域與引用 | 驗證者；失敗／案例 |
| --- | --- | --- |
| id | Code；具名形式，如 zone.locative、zone.allative、zone.ablative、zone.add_to_hand、zone.return_to_hand、keyword.display、quantity.classifier.source_selection | A/B/C；未知形式拒絕；FL-012 |
| lang | Lang；和 target 相同 | A/B/C；錯語言；FL-012 |
| signature | 除 zone.union_separator 外非空 `{name:Code,type:TypeName,role:Code}` 陣列，name 唯一，type 引用 §4 葉型別，role 引用其角色 | A/B；參數不合；FL-004／FL-012 |
| rule | Code；引用釘版 renderer 的有限規則，不接受程式 | A/B/C；未知或超域；FL-012 |
| cases | 非空 `{case_code:[FormPart]}` mapping；case_code 限 rule 登錄的分支，FormPart 是 `{kind:Literal,text:Text}`／`{kind:Label,arg:Code}`／`{kind:QuantityValue,arg:Code}`／`{kind:QuantityPlaceholder,arg:Code}`／`{kind:ZoneMember,arg:Code}`，每分支有序 | A/B；分支缺失／未知、arg 不引用 signature 拒絕；基礎名稱來自當前選詞；FL-012／FL-015 |

QuantityValue 的 arg 必指本形式簽章中的 QuantitySpec，僅在 `quantity.constant_value.v1` 使用，輸入 expr 必為 constant。
它只輸出 UInt 的十進位字串，不輸出 mode、單位或概念位置；all／any、bound／expression 均不適用。
N0 的 `quantity.<role>.value` form 只有對應 role 的單一 QuantitySpec 參數、default 分支及一個 QuantityValue。
arg 沿同一來源葉驗證，計入 required 引用閉包、render 依賴及來源／target occurrence；mode 仍由來源構造驗證。
機械轉換保留原 target 的固定數量修飾，只替換原數字 placeholder，不從中文反推 mode、單位、值或來源位置。

QuantityPlaceholder 僅屬於 `quantity.choice_mode_count.placeholder`／`quantity.choice_mode_count.placeholder.v1`，
簽章恰為 quantity:QuantitySpec/choice_mode_count 與 definition:QuantityExpr/count_expression，只有 up_to case。
arg 必為 quantity，只輸出字面 X 並記該槽的 leaf output／來源 occurrence；mode 詞由該語言 cases 的 Literal 提供。
quantity 必須是 up_to Expression，definition 須與 quantity.expr 完整相等，包括有序葉身分；constant、bound、ContextExpression 不適用。
同一 target 在 placeholder 之外必有直接 LeafRef(definition)，由 target 形狀驗證於 authored 載入及 render 前檢查；定義句可在 X 之前或之後。
全部頂層節點呈現成功後，才把 definition 已呈現的引用按 quantity 的槽名與角色補記實際引用，避免重複展開計數句；任一節點失敗會先拒絕或整欄 fallback。
X 本身不產生概念 annotation；概念與加粗範圍保留在定義句。缺必要選詞仍整欄 fallback。
直接 LeafRef 由 target 形狀驗證，其餘數量配對與引用補記由 renderer 負責；不另驗來源的 owner／能力／分枝、ChoiceContext 或選項數，那些仍由正式 producer 的 binding 驗證負責。

rule 只依 typed args 選具名 case；cases 必完整涵蓋該 rule 的簽章合法域，不能把選詞或語義值複製到規則內。
例如 zone.locative 的 hand／battlefield 分支各組合 Label 與「中」／「上」；三個 quantity.classifier.* 形式依計數語境選中文量詞分支。
形式可產生「手牌」＋「中」、「到」＋「戰場」＋「上」，或「加入」＋「手牌」、「回到」＋「手牌」。
動詞融合形式只可在相符 frame 動作／目的地角色使用，不能把 add、return、draw、discard、destroy 都變成 move。
keyword.display 可供括號，基礎名稱不含括號；別名與全名綁同概念，來源拼寫留 trace。
形式內的 Label 產生概念範圍，Literal 後綴／括號不擴大該範圍。缺基礎選詞則整欄退回（FL-011），未知形式 ID 則結構拒絕。

renderer 使用 `four-layer-renderer-v7`；正式來源 workflow、authored 與 DB reader 採 v6。
renderer 版本參與所有呈現依賴鍵，包含數位名稱政策；因此既有 translation ID 會重算。
render-v3 的內容 hash recipe 與公共快照 wire 保持原契約。
下列形式使用封閉 ID 與 type＋role 簽章；明示 Form 與 NP 隱式使用同一份 definition，不登錄舊 ID alias。

| ID | signature（name:type/role） | rule／cases |
| --- | --- | --- |
| quantity.choice_mode_count.placeholder | quantity:QuantitySpec/choice_mode_count；definition:QuantityExpr/count_expression | quantity.choice_mode_count.placeholder.v1；up_to；QuantityPlaceholder配獨立LeafRef定義 |
| zone.locative；zone.ablative | zone:ZoneSet/source_zone | zone.case.v2；七區域＋mixed |
| zone.counted_locative | zone:ZoneSet/counted_zone | zone.case.v2；七區域＋mixed |
| zone.allative | zone:ZoneSet/destination_zone | zone.case.v2；七區域＋mixed |
| zone.add_to_hand；zone.return_to_hand | zone:ZoneSet/destination_zone | zone.hand_action.v1；default；值限 [hand]，N1 B 驗同一局部 add／return 動作 |
| deck_position.ablative | position:DeckPosition/source_position | deck_position.display.v1；default |
| deck_position.allative | position:DeckPosition/destination_position | deck_position.display.v1；default |
| phase.trigger | phase:Phase/phase_trigger | phase.trigger.v1；default；無 duration producer |
| ability.display；keyword.display | term:Concept/ability；term:Concept/keyword | ability.display.v1；keyword.display.v1；default |
| trait.filter；class.filter | trait:Concept/trait；class:Concept/class_filter | trait.filter.v1；class.filter.v1；default |
| owner.source_possessive；owner.counted_possessive | owner:Player/source_owner；owner:Player/counted_owner | owner.possessive.v1；default |
| quantity.classifier.source_selection | kind:CardKind/counted_kind, quantity:QuantitySpec/selection_count, zone:ZoneSet/source_zone | quantity.classifier.v1；card／object，僅 Literal |
| quantity.classifier.counted_selection | kind:CardKind/counted_kind, quantity:QuantitySpec/selection_count, zone:ZoneSet/counted_zone | quantity.classifier.v1；card／object，僅 Literal |
| quantity.classifier.counted_existence | kind:CardKind/counted_kind, quantity:QuantitySpec/existence_count, zone:ZoneSet/counted_zone | quantity.classifier.v1；card／object，僅 Literal |
| zone.union_separator | 空 signature=[]，呼叫 args={} | zone.union_separator.v2；唯一 default，僅 Literal |
| zone.source_union.locative_or；zone.source_union.or；zone.source_union.and | owner:Player/source_owner, zone:ZoneSet/source_zone | zone.source_union.v1；battlefield／ex／separator，只供同一 G10 operand |

`zone.source_union.v1` 限來源 G10 的 `[battlefield,ex]` 聯集，依 canonical 順序逐成員輸出；
每個成員 case 的非 Literal 部分必須依序恰為 `Label(owner)`、`ZoneMember(zone)`。
ZoneMember 的 arg 只可指該簽章的 ZoneSet；它輸出目前成員的 typed 概念選詞，成員由 renderer 決定，target 不提供索引或常數。
separator case 僅有 Literal；renderer 只驗結構，zh-Hant 定義中 locative_or 在 battlefield 後接「上」、成員間「或」，or 只接「或」，and 只接「與」。
三者皆保留同一聯集及共同 owner，不能藉中文「與」推導兩區各選一次或日文 `と` 的獨立並列。
G10 仍只接受既有封閉 `か`／`や` 構造，`と`／`または`／`および` 不因新 Form 而被納入。
B 依原來源 operand 比對 owner 與 zone 的完整 raw spans，拒絕跨 operand、不同 owner 或非 G10 引用；
A 驗 type＋role、封閉 cases、必呈現的兩參數；C 每次 owner Label 與每個 ZoneMember 各產實際概念 range、葉輸出路徑及必要引用。
格後綴與連接詞不包進概念 range；重複 owner 的輸出仍指原同槽 occurrence，不新增來源槽、值或位置。
未知 Form、錯角色、缺 owner／member／選詞、非二區值均拒絕或依既有缺詞規則整欄退回。
既有空簽章 union separator 與 G10 語義維持原契約，新 Form 的完整 definition 參與 render 相依鍵。

七個區域 Code 為 battlefield／hand／deck／evolve_deck／graveyard／ex／banish；mixed 逐成員引用。
所有 label 形式在每個 case 必須有對應 Label，不能以空輸出或固定字替代引用。
`zone.card_np`、單一 `quantity.classifier` 與 separator rule v1 已移除，使用處依來源角色改為上表 ID。
CardNP 依自己的 zone role 選 locative；量詞依完整三參數 role tuple 派送，其他 tuple 結構拒絕。
NP 無 zone 時只接受唯一 counted_zone 作量詞語境，不能借 destination_zone；無唯一語境沿 unresolved_classifier_context 整欄退回。
all／any 不呼叫量詞形式；數字、來源 unit 與語義分類不因中文量詞重型別。

owner＋zone 使用 owner label 接區域形式；只有 owner、沒有 NP zone 時依 role 使用 possessive。
所有格／格助詞／量詞的 Literal 不包進概念 ranges。缺基礎選詞或必要隱式形式整欄退回。
A 案省略 owner 不選用時不提供 args.owner，不產所有格形式或選詞品質依賴；選用時引用同槽。
ZoneSet 多成員按 canonical 排序顯示，每成員有自己的概念範圍；空簽章 separator 不讀值、不計入必要引用。
這些呈現改動不新增來源葉或放寬來源單位守衛；v3 使 dependency_key 與相依 tr: 更新，不改來源 frame／binding。
固定案例見 [參數群組渲染案例](../../../carddb/tests/fixtures/parameter-groups/render-cases.json)。
案例的 `parameter_group_render` 只驗 renderer 的隔離 typed fixture；`parameter_group_source` 另從合成日文經現行來源推導驗選取單位與 counted_kind。
N1 正文依 §4.3 的封閉來源構造提供 CardNP 的 counted_kind／ZoneSet／Player／TokenStatus 葉；仍只允許同一來源 operand、已涵蓋的修飾與角色，不以 generic renderer 案例擴大來源範圍。

### 5.1 原文單位與中文量詞是兩張表

必須**依計數物、所在區域集合與數量角色查表並驗回原文單位**。
查表還要帶 token 狀態、聯集形狀及具名來源構造；沒有匹配規則保持未解，不能套「非戰場一律枚」。
以下是必須覆蓋的單位域，不是放諸所有動作的通配規則；原文單位表由釘版 normalizer 管理，中文風格由 form 管理。

| 計數物／計數時區域集合／角色 | 必要來源單位規則 | 反例／案例 |
| --- | --- | --- |
| 從者、戰場、卡數 | 已辨識構造可用体；枚須該構造另有明示規則 | 不全域消除枚／体；FL-003 |
| 代幣從者、EX 區、卡數 | 允許体，不因離開戰場改成枚 | 枚不是任何 EX 從者都適用；FL-003 |
| 從者、戰場與 EX 區聯集、整體卡數 | 明示枚規則；保留分支限制與整體數量 | 不逐分支重新算量詞／數量；FL-003／FL-018 |
| 護符、戰場、卡數 | 依具名構造允許つ／枚，保留原單位 | 不把つ一般化為次數；FL-003 |
| 主戰者、無卡片區域、玩家數 | 人；異種並列分別保留玩家／從者計數型別 | 不強轉 Card 集合；FL-003／FL-018 |
| 卡片、手牌／牌堆／墓場、卡數 | 相符構造的枚；以計數當時來源區域為準 | 不用目的戰場推成体；FL-017 |
| 指示物／重複次數／傷害量 | 各自的量與角色；不是 `Cardinality<Card>` | 同數字不同域必拒絕合併；FL-004 |

trace 留下實際 unit、別名及省略主語；即使最後中文統一量詞，原文仍須逐 byte 重建。
`の`／`から` 只在已辨識等義的選取構造內正規化，不對移動來源、期限或任意句子全域替換。

## 6. SourceBinding、trace 與逐 occurrence 位置

SourceDescriptor 定位完整來源欄位，不用同 text hash 取代 owner 身分。

| SourceDescriptor 欄位 | 型別／合法域與引用 | 驗證者；失敗／案例 |
| --- | --- | --- |
| owner | 翻譯契約 §6.3 的具名 owner；恰一種，FK 必存在 | B/C；錯 card／owner；FL-007 |
| field | §6.3 合法 Code；effect/section 等分開 | A/B/C；借另一欄來源；FL-007 |
| ordinal | UInt?；只有 section/action_label 非 null | A/B/C；錯段落；FL-023 |
| source_unit_id | text_unit ID；必為該 owner 自己的 exact 字串 | B/C；同字異 owner 未驗；FL-007 |
| source_hash | Hash；完整欄位 UTF-8 bytes | B/C；過期／錯原文；FL-007 |
| source_ref | `{batch_id,source_version_id,parser,locator,text_hash}`；翻譯契約 §2 來源定位 | B/C；缺封存來源、錯面、hash 不符；FL-007 |

source_ref 的前三個識別為非空 Text，locator 是指向該完整欄位的 JSON Pointer，text_hash=source_hash；
parser 必為本次支援的釘版解析器。batch 與版本 FK 對固定輸入 inventory，不存私人 store／路徑。
來源單元含 Unicode 字串；source_ref 找到的文字相同仍須驗精確 owner 與全部面，不能只比 hash。

| SourceBinding 欄位 | 型別／合法域與引用 | 驗證者；失敗／案例 |
| --- | --- | --- |
| id | `bind:`＋H(binding-v2 payload，見下文) | B/C；ID 錯配；FL-008 |
| source | 上述 SourceDescriptor | B/C；錯來源；FL-007 |
| ordinal | UInt；該來源欄位中按第一個 segment.start 由零連續 | B/C；重複／漏序；FL-008 |
| line_ordinal | UInt；完整欄位原行號，由分段器重建 | B/C；移到別行；FL-008 |
| frame_id | FrameID FK | A/B/C；懸空或變體不符；FL-002／FL-005 |
| source_span | `{role,segments:[Span],anchor:UInt?}`，沿翻譯契約 §4.1 | B/C；頂層重疊／漏字／壞 anchor；FL-008 |
| values | `{slot_name:TypedValue}`；槽的 type 決定 §4 值形狀 | A/B/C；缺必要值、多餘值、錯域；FL-004／FL-005 |
| occurrences | 下述 LeafOccurrence 陣列，依 raw 起點與 slot 排序 | B/C；漏掉重複引用或範圍錯；FL-008／FL-014 |
| trace | TracePiece 陣列，依來源位置排序；非空 binding 不可空 | B/C；不能回復 exact bytes 或 canonical_source；FL-008 |

OccurrenceKey 恰為 `{owner,field,ordinal,source_hash,line_ordinal,role,segments}`，各型別沿上述；
它指來源實例，不含新 frame ID，故 pending 語義身分不循環。相同實例的 source_ref 改封存批次不會重配語義身分。
binding-v2 payload 恰為 `{recipe:"binding-v2",occurrence:OccurrenceKey,frame_id,source_span,values,occurrences,trace}`；
source_ref／source_unit_id 仍驗來源並存 DB，但不是封存批次改名的 ID 擾動來源。

| 位置／trace 子物件欄位 | 型別／合法域與引用 | 驗證者；失敗／案例 |
| --- | --- | --- |
| LeafOccurrence.slot | Code；本 frame 槽引用 | B/C；懸空；FL-005 |
| LeafOccurrence.ordinal | UInt；該槽按來源次序由零連續，同槽值須一致 | B/C；漏 occurrence／不同值塞同槽；FL-014 |
| LeafOccurrence.raw_spans | 排序非空 Span 陣列；完整欄位座標 | B/C；越界／把 normalized offset 當 raw；FL-008 |
| LeafOccurrence.canonical_spans | 排序非空 Span 陣列；canonical_source 座標，對應 LeafSlot.occurrences | B/C；schema 錯配；FL-008 |
| LeafOccurrence.source_unit | Text?；數量有單位時必為 exact 單位，其他 null | B；單位與語境不符拒絕結構合併；FL-003 |
| LeafOccurrence.source_presence | explicit/omitted | B；省略卻造 raw span；FL-020 |
| LeafOccurrence.resolution_rule | Code?；omitted 必須指具名解析規則或保持語義 pending | B；擅自默認 self；FL-020 |
| TracePiece.raw_span | Span；完整欄位座標 | B/C；遺漏、越界或重疊；FL-008 |
| TracePiece.canonical_spans | Span 陣列，可空（排版／移出提示）；多對多可共指同一 canonical 區間 | B/C；未記 NFKC 展開；FL-008 |
| TracePiece.rule | Code；normalizer 內已登錄規則，含 identity／layout／unit／alias | B；未知改寫或吞語義；FL-008／FL-021 |

omitted occurrence 的 raw_spans 與 canonical_spans 都為空（上述非空規則的唯一例外），
resolution_rule 非 null 才能有推導葉值；未知省略保留 pending 且不造可執行值。explicit 的 resolution_rule 可 null。
raw 字串由 source_unit 按 span 擷取；trace 不再保存另一份 raw bytes 或逐詞 hash／採納收據。
NFKC 的前後字元由 raw_span 與 canonical_spans 取出，允許多對多，禁止移動 raw 座標。
驗來源分兩步：按頂層 segments／trace 重組完整 UTF-8；再從 raw 重跑具名規則驗 canonical、角色、葉值與原單位。
不能只渲染中文後重跑同一 normalizer 當作驗證。body/reminder/header/layout 恰分割原欄位，unknown 不能當空字串。

## 7. 規則投影接口

本節只固定來源角色、型別與 scope；不新增作者 A.* 語法、遊戲原子或正式 DSL adapter。
完整本體仍依[作者語法](../../dsl/author-syntax-1.0.md)與 `dsl/` 的實作狀態。

| Projection 欄位 | 型別／合法域與引用 | 驗證者；失敗／案例 |
| --- | --- | --- |
| projection_kind | ability_body/ability_flag/card_field/none/pending | A/B/C；未知分類須 pending；FL-006 |
| discriminator | Code?；resolved 類別的具名分類規則，pending 時 null | A/B；無規則卻標 none；FL-006 |
| scopes | 依 id 排序的 `{id:Code,parent:Code?,kind:ability/branch/sequence}` 陣列；id 唯一，parent 引用本陣列，根 parent=null，父子無環 | A/B/C；未知或循環作用域；FL-022 |
| imports | Port 陣列，依 name 排序唯一 | A/B；錯型別、未知先行詞；FL-022 |
| exports | Port 陣列，依 name 排序唯一 | A/B；結果越 scope；FL-022 |

Port 恰有 `{name:Code,type:TypeName,source_role:Code,scope:Code}`，皆必填；type 引用該分類規則版本的封閉接口型別登錄，
例如 ObjectSet、PlayerRef、ReceiptId、CapturedValue、QuantityExpr，不允許 Any／任意 it 字串。
source_role 限該分類規則明列的 selected_objects/action_result/paid_result/selection_snapshot/count_value 等；
scope 指同 frame 宣告的 ability／branch／sequence 作用域。A 驗簽章，B 驗實際來源上下文與支配／可見範圍（FL-022）。
同一 scope 代碼的具體能力實例由來源 owner／行位置限定，不跨卡共享值。
尚未支援的 type／scope 關係保持 pending、拒絕相依可執行投影，不假裝 adapter 已完成。

`interface_key:Hash` 是由 B 從已驗 Projection 推導的 DSL 依賴鍵，不是 authored 欄位或 frame 身分；
其 canonical-json-v1 payload 恰為 `{recipe:"frame-interface-v1",frame_id,scopes,imports,exports}`，取完整 SHA-256。
陣列沿上表排序；同 key 仍比完整 payload bytes，碰撞拒絕。C 驗 DB 讀回的組成欄位，B 重算鍵（FL-001／FL-016／FL-022）。
每個使用該 frame 的 DSL 依賴必納入此鍵；更動任一 port／scope 都須重驗所有使用處的型別、來源與可見範圍。
移除仍被 QuantityExpr.import 或相依接口引用的 port 是壞引用，須拒絕；key 改變本身不代表新接口有效。
只有來源語義及既有引用仍有效的接口描述調整，才可保持 frame、binding 與 render 身分；不保存歷次接口帳本。

| 類別 | 必要語義及結果 | 案例 |
| --- | --- | --- |
| ability_body | 可對接能力本體；targets/choose、cost/effect、may、分支／結果不可丟 | FL-002／FL-019／FL-022 |
| ability_flag | 例如已辨識 Quick 對應 A.ability.quick；有明確所屬能力，不當空白本體 | FL-006 |
| card_field | 例如構築限制對應 A.card.deck_rule；不是執行指令，也不是 none | FL-006 |
| none | 已辨識且無獨立執行內容的 layout／純提示；imports/exports 必空 | FL-006／FL-008 |
| pending | 未知／未解分類，保留原來源與原因清單，不能計為 no-op 或已完成本體 | FL-006 |

各頻次 frame 都可保存 DSL；是否取得合格巨集資格是另一條軸，同一 resolved 變體至少三張不同卡
（卡名概念＋是否進化；重印／異圖不累計）僅滿足頻次條件。完整資格依 [#497](https://github.com/gbaian10/sve-kit/issues/497)，
本文件不改作者語法 §10，也不授 reviewed／verified。後四類不計為完成的 ability_body。

## 8. 建置 DB、render projection 與依賴

以下是[build-db §9.3](../build/build-db.md#93-四層資料契約)的邏輯列契約；N1 已接入正式 DDL 與建置邊界，實際儲存欄位見該文件 §9.1。
所有新 Json 欄均使用本文件具名型別，C 逐層驗證後才能交給其他層。
現有 authored_source_id／record_key／origin／low_confidence 與真實 source_id 的型別及 nullable 保留。

| 邏輯列／鍵 | 四層欄位、引用與差異 | 驗證者；失敗／案例 |
| --- | --- | --- |
| sentence_template；PK id | §3 Frame＋建置才有的 canonical_source:Text；source 描述拆欄或型別 Json；取代舊六欄 payload | B/C；hash／域錯配；FL-002／FL-004 |
| template_translation；PK template_id,lang,variant_key | target:Target、expression_targets:{Code:Target}；其餘品質欄保留；template_id FK、variant_key=default 表一般值 | A/B/C；壞 target；FL-005 |
| glossary_term、glossary_translation；鍵不變 | 基礎概念／選詞及 emphasis 保留；形式不把基礎名字複製成多份 translation | A/B/C；引用錯配；FL-013／FL-015 |
| translation_form；PK id,lang | §5 FormDefinition＋人工來源／品質欄；rule 隨 renderer pin，signature／cases 有型別 | A/B/C；壞 form；FL-012 |
| text_template_binding；PK id，UQ use_id,ordinal | §6 SourceBinding；use_id FK translation_use 取代 context_id，context 沿 use 取得；owner 不同即各驗來源 | B/C；錯 owner／錯面／同字串借用；FL-007／FL-028 |
| binding_leaf_occurrence；PK binding_id,slot,ordinal | LeafOccurrence 拆列；binding_id FK text_template_binding；不能以 translation_term 去重掉次數 | B/C；位置漏失；FL-008／FL-014 |
| render_leaf_occurrence；PK translation_id,binding_id,node_path | `{translation_id:ID,binding_id:ID,node_path:[UInt],slot:Code,source_ordinals:[UInt],ranges:[Span]}`；前兩欄 FK，slot／source_ordinals 指該 binding 的葉 occurrence，node_path 為 target／form 展開後的唯一節點路徑，ranges 指輸出；所有陣列非空 | B/C；來源／輸出位置失聯，重複引用被去重；FL-014 |
| translation；PK id | 原 context/target_lang/text/origin/authority/low_confidence/source_hash/source_id 保留；render-v3 內容鍵，tokens 不作隱藏承載 | B/C；依賴不一致；FL-015 |
| translation_binding；PK translation_id,binding_id | translation_id／binding_id FK 及 (template_id,lang,variant_key) FK 保留；frame／語言／source context 一致 | B/C；引用失配；FL-005／FL-007 |
| translation_term；PK translation_id,term_id | 保留去重反查索引；不是位置表、不能據此搜中文字串補 span | B/C；卡名內誤加粗；FL-013 |
| annotation_set；PK id | `{id:ID,text_unit_id:ID,occurrences:[Annotation]}`；text_unit_id FK；相同文字不同概念須不同 id | B/C；錯 text identity／去重語義；FL-013／FL-014 |
| translation_annotation；PK translation_id | `{translation_id:ID,annotation_set_id:ID}`；兩欄 FK，text 必等 translation.text | B/C；錯目標字串；FL-014 |
| translation_use_annotation；PK use_id | `{use_id:ID,annotation_set_id:ID}`；兩欄 FK，text 必等 use 的來源；沒有譯文也能存在 | B/C；JP offsets 套 EN；FL-023 |

render_leaf_occurrence 的 node_path 以零起算子節點索引逐層定位；同 form 多個 Label 各有不同路徑。
主 Target 與 expression map 共同參與資料序列化、選稿與 render dependency；固定字、實際展開的
expression AST、label／Form、品質及 variant 改動皆重算。完整稿件不同不能僅因主 AST 相同而合併。
source_ordinals 排序唯一；同葉值多處來源共同供一次呈現時可列多筆，輸出重複兩次時必有兩列。
省略來源但經具名規則解析的葉仍有 omitted occurrence，因此不用假 source span。

translation_context 仍以 source_unit_id 與 semantic_variant 唯一，後者在四層是 context 的聚合鍵：
`cv:`＋H(`{recipe:"context-variant-v2",assignment,bindings:[{frame_id,values,source_span}]}`)，
assignment 為既有 context_assignment 的 variant（沒有時 default），bindings 依來源 ordinal 排序。
它不是單一 Frame.semantic_variant 物件；同字異概念會因 values 不同分 context，NP／form 改動不分 context。
兩欄均必填，C 驗 FK／唯一，B 驗聚合重算（FL-013／FL-015）。

Annotation 的每欄如下；set 是 exact 文字的語義位置，binding 對應從來源 occurrence 與渲染節點路徑保留，不靠文字搜尋。

| 欄位 | 型別／合法域與引用 | 驗證者；失敗／案例 |
| --- | --- | --- |
| ordinal | UInt；同 set 依 start/end/reference 排序後由零連續 | B/C；重複／跳號；FL-014 |
| reference | TermReference 或 CardName 引用，FK 依 §4 | B/C；同字冒充同概念；FL-013 |
| ranges | 非空、排序、互不重疊 Span 陣列，對 exact text_unit | B/C；負值、越界、UTF-16 混用；FL-008／FL-014 |
| bold | Bool?；glossary 規則推導，未定 emphasis 為 null 並列原因 | B/C；未定偽裝 false；FL-015 |

同一 set 各 occurrence 不得重疊；同一概念出現兩次保留兩筆，單次跨段可有多個 range。
NP 範圍不是 annotation，卡名內的同字不另套術語 annotation。form 的括號／後綴通常不含在概念 range。
`annotation_set.id = ann:`＋H(`{recipe:"annotation-v1",text_unit_id,occurrences}`)；完整內容相同才共用。
純字串去重 text_unit 不代表共用 annotation_set；無翻譯時原文 set 仍存在，公開如何承載由 #496 決定。

必要引用集合 E 逐 `(slot,role,reference)` 計算；ZoneSet 逐成員，CardName 是整名引用。
先排除由來源規則產生、required=false 且 occurrences=[] 的純省略槽，其值仍必須存在。
其餘明示結構／required 葉、四種門檻能力名，以及該用途實際選中的 label／variant 的 emphasis=true 或 origin=project／machine 都須引用。
A 只計 LeafRef、Form 的實際 Label／QuantityValue／ZoneMember 與 NP 展開的輸出；形式的讀值參數不等於呈現引用。
QuantityPlaceholder 只消耗 quantity 槽，不代用 definition；其必要概念須依上述獨立定義句的成功渲染補記。
E−A 必須為空；同字不同角色、卡名內同字與另一槽的引用不能補洞。重複呈現各保留 node_path／occurrence。
缺必要或實際使用選詞沿 missing_term_translation 整欄 fallback；有 target 但漏 E 是結構錯誤，不能改叫缺譯。
未用的純省略 label 不加品質或 fallback 依賴；emphasis／origin 是 render 條件，不入 frame hash。
Code→Reference 固定對照：Player self／opponent→term:player.self／opponent；Phase start／main／end→term:phase.start／main／end；
DeckPosition top／bottom→term:zone.deck_top／bottom；TokenStatus token→term:object.token。
ZoneSet graveyard→term:zone.cemetery、banish→term:zone.banishment，其餘 Code 對應 `term:zone.<Code>`；不拿 Code 當中文。

`render projection` 在建置內恰有 `{text:Text,annotation_set_id:ID,dependency_key:Hash}`，皆必填；
set 引用上表、text 必相同，B/C 驗閉包（FL-014／FL-015）。
render-v3 使用 `{recipe:"render-v3",context_id,target_lang,dependency_key,text,origin,authority,low_confidence}` 取 H，
translation.id=`tr:`＋完整 hash，revision 為前 13 hex 的 52-bit 數；同 revision 異 hash 拒絕。
dependency_key 是下列實際使用值的 canonical hash，包含 target／form／label／emphasis 及來源 binding 的呈現值、renderer 版本、
選中的 variant_key、origin／authority／low_confidence；排除 note、時鐘與私人路徑，不加核可快取。
binding 的呈現值恰為 `{source_hash,frame_id,values,source_span,occurrences,trace}`，不含 owner／封存批次；
interface_key 及 scopes/imports/exports 不入 render 依賴，相關引用仍須通過 §7 驗證才能重用既有呈現。
同 context 可共用相同 render，但所有來源 use 仍各驗 owner 並以 translation_binding 記用途。
官方 counterpart 的逐 owner 依賴另依翻譯契約 §7.2，不用此去重規則借用官方資格。

| 變動 | 必須失效／重建 | 不因此失效 | 案例 |
| --- | --- | --- | --- |
| 選詞、form 中文風格、target NP 覆蓋、emphasis、品質旗標 | 所有依賴 render／annotation／selection；文字未變但 bold 改也重算 annotation | frame ID、葉值與 DSL 語義審查 | FL-001／FL-015 |
| source owner／face／hash、葉值或原單位 | 該來源 binding、use、render、裁定用途及相依 DSL freshness | 無關來源；resolved frame 語義 payload 相同可重用 ID | FL-007／FL-016 |
| canonical、normalizer、slot role/domain、semantic_variant、projection_kind／discriminator | frame 重鍵、全用途 binding／render／裁定映射／相依 DSL 重驗 | 永久卡片身分 | FL-002／FL-016 |
| 來源語義與既有引用不變，只調整 scopes／imports／exports 描述 | interface_key 重算；相依 DSL 的接口連接、用途審查與實跑資格失效並重驗 | frame ID、binding／context、target、render／annotation、裁定映射及永久卡片身分 | FL-001／FL-016／FL-022 |
| DSL body 或 adapter lowering | 其 body version／用途審查與實跑資格 | 純呈現與未改的 frame 語義身分 | FL-016 |

依賴反查可由本次型別列建索引；不用每詞採納收據或永久失效帳本。建置每次產生新 DB，舊快照不原地修補。

## 9. 舊模板重鍵與裁定引用

舊 T/C ID 與新 frame 的關係是多對多；不能依 hash 前綴、相同中文字、
`supersedes_id` 線索或最大覆蓋率選一個新 ID。
映射只在一次重建及當前裁定檢查使用，舊格式仍留 Git；不建長期相容 reader。
版本唯一差異的自動映射須先移除 normalizer_version 後比完整 frame-v1 payload bytes，並確認 occurrences／values／scope／接口語義不變、新目標存在且唯一。
其他 canonical／schema 改動依精確來源用途重建，不因中文相同授予語義等價。interface_key 重算，相依 DSL 只延續實際存在且先前合格、通過既有型別／來源／可見範圍檢查的用途；不捏造 reviewed。
不能從新版本重算未知歷史域，原 reference pending 與裁定 strength 保留。
明示舊域格式及寫入端驗證見 §9.1.1；正式 reader 不因保留舊引用而恢復舊翻譯格式。

### 9.1 裁定版本與引用來源

裁定文件採 `format:2,kind:ruling`，原本文不另包 data；`id` 與 `revision` 必填。
revision 是 1..9007199254740991 的整數，不接受 Bool；format 不是裁定版本。
既有未編版裁定在切換時明寫 `revision:1`，命名當時現行內容，
不把先前 Git 歷史追認為同一版，也不提升裁定強度或審查資格。
正式 reader 不接受舊封套，不為缺失 revision 補值，不接受 version 別名。
後續裁定的已解析內容（含 applies_to 順序）改變須明示遞增 revision；
只有排版或註解改變時，可保持 revision，但建置仍須重新驗實際來源 hash。
同一 `(id,revision)` 的不同已解析內容不得混入同一輸入閉包。

`ruling_ref={id:ID,revision:UInt,reference_ordinal:UInt}` 指該版裁定的
applies_to 陣列中一項；reference_ordinal 從零起算，不能排序後重編或先按 ID 去重。
相同字串出現在不同 ordinal 仍是不同引用。
`ruling_source={path:Text,sha256:Hash}` 指本次固定 authored 輸入中的完整裁定檔：
path 為 repo 相對 POSIX 路徑，限 authored/rules/rulings 下的裁定 YAML，
不得有絕對路徑、空路徑節、`.` 或 `..`；sha256 為 exact 檔案 bytes 的 hash。
建置既有輸入紀錄另釘 authored 修訂；同一 `(id,revision)` 的所有引用使用同一來源檔。
這是本次來源追溯資料，不是新增 authored index、逐引用核可收據或永久帳本。

A 讀取裁定一次並驗封套、型別及 ID 唯一；B 從現行裁定檔保留 id／revision、
引用 ordinal、原始字串與完整檔案 hash；C 沿既有型別邊界檢查 DB 列。
檔案缺失、hash 錯、ordinal 越界、引用字串被替換、版號不存在均為錯誤，不能改成 pending。

本節 Mapping／RulingResolution 處理舊模板引用；IR 元素引用不轉成 template_id 或 frame。
所有 applies_to 項均須進本次引用對帳；IR 引用另保存 ruling_ref、ruling_source 與原始 ID，
標明非模板保留，沿原裁定／IR 契約驗證。未通過其既有可執行檢查者不得進執行閉包，
本節的保留紀錄不授予其有效性，也不算模板重鍵 resolved。

### 9.1.1 明示歷史適用域

`applies_to` 的陣列位置仍是原引用 ordinal。每一項可以是 IR 字串、
尚未補證的裸 T 字串，或 `LegacyReference`：

```yaml
legacy: T0118c58cae
namespace: classification-v0
uses:
  - cards: [BP20-041, BP20-SL13, BP20-U04]
    occurrence:
      owner: {kind: face_revision, revision_id: "rev:v1:..."}
      field: effect
      ordinal: null
      source_hash: "..."
      line_ordinal: 1
      role: body
      segments: [{start: 0, end: 1}]
```

以上座標與 hash 是形狀示意，不能直接當成資料。`legacy` 保存原 T ID；
`uses` 是完整舊域，不得為空或重複 occurrence；每個 use 的 `cards` 是非空、
不重複且按字典序排序的日文卡號；uses 依 occurrence 的 canonical JSON bytes 排序。
來源必須是 face revision 的 effect 或 section，role 必為 body，
segments 須按順序且互不重疊。同一命名空間與 T 在不同裁定中的 uses 必須一致。
同一 T 不得在不同裁定或同份裁定中混用裸字串與明示域，以免替未補證引用借用其他裁定的域。
不把 frame ID、候選、處置或逐用途核可記錄寫進 authored。

完整驗證只在資料寫入端執行，普通建置信任已審查資料。修改裁定引用或 A1 分類資料後，
提交前執行獨立指令：

```bash
sve-carddb rulings validate --repo /absolute/candidate-repository --build-db /absolute/validated-build.sqlite
```

`build-db` 必須是已驗證、關閉且包含所選歷史來源版次的 offline 建置 DB；
指令只讀取它，不產生或修補來源證據。產生器亦可於採納前呼叫
`domains.rulings.historical.validate`，由已驗的來源邊界提供觀測。驗證內容包括：
全部 cards × face／field／ordinal／line 必須恰好分割原模板的成員集合，
每個 span 的文字長度與 hash 必須等於原成員 body。來源觀測由已驗的建置提供；
同字不同 revision 的歷史選擇仍須以凍結舊快照與完整 face 證據審查，
不能只靠當前 body 相同便宣稱證明了舊域。一次性遷移腳本與對帳產物不進版控。

寫入端驗證明示來源域的 owner 存在、
欄位全文 hash 相等、span 在指定行內（只有 LF 推進行號），以及 cards 確實屬於該來源 face。
這些來源事實錯誤直接使資料驗證失敗，不得採納。普通建置透過 `historical.rebuild`
直接讀取已審查的用途清單，不載入 A1 catalog、不重查舊來源欄位，也不重驗成員、hash 或長度；
不重跑舊 normalizer，也不在執行時讀 Git 歷史。
新舊 scope 均從 occurrence 的 role 與 field 產生，domain 使用 `effect` 或 `section`，
它只界定來源欄位，不宣稱 token 定義的執行語義。

當前候選只取本次翻譯建置已驗證且已寫入的完整欄位 binding；每次建置重算映射。
缺 binding、切分變動或語義變體未定案保留用途 pending，不能回退為引用未知或
把同 frame 的其他用途納入。明示引用若未提供已驗證的重建輸入，resolver 直接拒絕。
舊 v0 沒有具型別的操作數，重建的空 parameters 表示未提供此證據，不能推論參數等價；
frame family 中 `classification_v0.t...` 只是原定義的來源群組，不是已核定語義。

一次性對帳須保留每個原 ordinal 與 IR 引用，分別報告候選分類及正式 resolution。
候選分類只用來與舊盤點比較；完整舊域以已證明的 occurrence 清單為準，不能據此縮小歷史用途。
rebuild 的 callback 保存上述已驗物件供 resolver 比對，不是另一層獨立來源驗證。
裁定內容與適用域變動仍依 §9.1 明示升 revision，保留原強度、證據與日期。

### 9.2 已知舊域的 Mapping

| Mapping 欄位 | 型別／合法域與引用 | 驗證者；失敗／案例 |
| --- | --- | --- |
| legacy_namespace | Code；明示且已驗的舊 ID recipe 與 normalizer 版本 | A/B；不同版本 T ID 混用；FL-024 |
| legacy_template_id | ID；該命名空間的實際舊定義 | B；不存在；FL-024 |
| occurrence | OccurrenceKey；精確舊適用來源 | B；不屬舊模板範圍；FL-025 |
| frame_id | FrameID FK | B/C；目標不存在；FL-026 |
| semantic_variant | 與 frame.semantic_variant 相同 | B；變體不符；FL-024 |
| scope | `{role:Code,domain:Code}`；具名來源角色及舊適用域的交集 | B；擴大適用域；FL-025 |

映射邏輯鍵是 `(legacy_namespace,legacy_template_id,occurrence,frame_id,scope)`；重複邊拒絕。
Mapping 各欄皆不可 null。未知舊域不建立 Mapping，不以 placeholder 冒充舊定義。

### 9.3 引用層級及用途層級的 RulingResolution

| RulingResolution 欄位 | 型別／合法域與引用 | 驗證者；失敗／案例 |
| --- | --- | --- |
| ruling_ref | §9.1 的物件；指實際裁定版本及其中一個模板引用 | A/B/C；漏引用、重複或不存在；FL-026 |
| ruling_source | §9.1 的物件；精確固定檔案 | A/B/C；錯檔、hash 或同版來源矛盾；FL-026／FL-027 |
| level | reference/occurrence | A/B/C；未知值或層級混用；FL-025／FL-026 |
| legacy | `{namespace:Code?,template_id:Text,occurrence:OccurrenceKey?}`；全部必填 | A/B/C；條件見下文；FL-024／FL-025 |
| status | resolved/pending | A/B/C；第三種狀態拒絕；FL-026 |
| target | `{frame_id:FrameID,semantic_variant,scope,occurrence:OccurrenceKey}` 或 null | B/C；resolved 須有唯一精確映射、不擴域；FL-024／FL-025 |
| candidates | 排序唯一的 target 形狀陣列；排序按 canonical JSON bytes；每一候選 FK／來源／域亦須可驗 | B/C；候選不得是假目標；FL-026 |
| reason | Code?；封閉原因集合見下文 | A/B/C；狀態矛盾；FL-026 |

legacy.template_id 是該裁定原始引用的非空文字，必須逐字相同；
此處特意使用 Text 保存可能失去定義的歷史引用，不宣稱它已通過 ID 登錄 FK。
進入 occurrence 分支與 Mapping 時，仍須驗它是該舊命名空間的實際定義。

本次對每個模板引用恰取下列一種表示，兩者不能混用：

1. **用途層級**：已從固定舊輸入證明命名空間、實際定義及完整明示適用域。
   level=occurrence；namespace 與 occurrence 均非 null。
   每個舊 occurrence 恰有一筆 RulingResolution，包含沒有新候選的用途。
   唯一鍵為 `(ruling_ref.id,ruling_ref.revision,ruling_ref.reference_ordinal,level,legacy.occurrence)`。
   全部列的 occurrence 集合必須等於原引用的完整明示域，不漏列、不加列、不靠總列數充當集合相等。
   明示域空集合是無有效適用對象的錯引用，拒絕建置；不能輸出零列當完成。
2. **引用層級**：固定裁定確有此歷史模板引用，但舊命名空間、實際定義或完整適用域無法確立。
   level=reference，occurrence=null，status=pending，reason=unknown_legacy_scope，
   target=null，candidates=[]；上述欄位都必須明寫。
   唯一鍵為 `(ruling_ref.id,ruling_ref.revision,ruling_ref.reference_ordinal,level)`。
   namespace 已有可驗 recipe／normalizer 證據時填其 Code，否則必為 null；
   不用 `unknown` 字串、T10 長度、現行 normalizer 或任意版本名替代未知。
   此列只聲明歷史引用存在，不宣稱其模板、適用域或目標已驗證有效。

namespace=null 僅准於引用層級；occurrence=null 當且僅當引用層級。
同一 ruling_ref 不得同時有引用層級列與用途層級列，即使只找到部分舊成員，
也先保留整個引用的 unknown_legacy_scope，不用已知子集合冒充完整域。
引用層級不得保存 target 形狀候選；supersedes 等未驗線索可列在本次診斷報告，
不放進 candidates，也不由消費端自動升為 active。

用途層級的 resolved 要求 target 非 null、candidates=[]、reason=null；
target 的 frame、semantic_variant、scope 與 occurrence 必與一筆已驗 Mapping 一致。
用途層級的 pending 要求 target=null、reason 非 null；原因限
no_candidate、ambiguous_variant、source_changed、unsupported_relation。
no_candidate 的 candidates 必空；ambiguous_variant 至少有兩個不同且已驗的候選；
其他原因的候選可空，但若列出仍必須逐筆通過全部來源、FK 及適用域檢查。
unknown_legacy_scope 只用在引用層級。source_changed 表示舊 occurrence 已驗、
新來源無法安全承接，不表示可忽略本次提供來源的 hash／owner 錯配。

正式建置直接讀現行 format 2 裁定檔，不要求 repo 外的一次性遷移輸入、
永久 authored allowlist 或 runtime Git 查詢。裸 T 引用按原 applies_to ordinal
產生引用層級 pending；明示域引用於寫入端通過 §9.1.1 後，普通建置直接讀取並產生完整用途層級結果。
非模板 IR 引用原樣保留並獨立列帳；不符合支援形狀的引用拒絕建置。
裁定來源記錄保留本次檔案 path 與實際 bytes hash。

### 9.4 完整性與執行邊界

B 逐筆保留裁定原始 applies_to 清單與 ordinal，重複文字的引用仍各有一筆記錄。
明示舊域的完整用途集合於寫入端依 §9.1.1 對 A1 catalog 驗證。
對帳同時列出模板引用、非模板引用、引用層級 pending、用途層級 resolved／pending；
不把引用數和用途列數相加，不以「輸出 0 個裁定」宣稱驗收完成。
N0 的 resolution.build 依現行裁定的原始 ordinal 產生引用列；DB 檢查 SQL PK／FK 與逐列封閉型別。
resolver 檢查裁定邏輯鍵唯一／互斥及完整用途集合；已採納舊域不於建置重驗，當前候選來源由本次翻譯建置提供，
DB 邊界另驗候選 frame 與 binding 都已存在於本次建置（FL-027）。

active 裁定邊只能由已驗的用途層級 resolved 產生，且其 scope 僅限該用途。
多個舊用途可 resolved 到同一新 frame；任何一筆都不代表適用新 frame 的所有卡。
active 懸空引用必為零；所有 pending 與其候選不得進可執行閉包。
這不自動授予 resolved 邊所依賴 DSL 的 reviewed／verified、freshness 或實跑資格。
裁定正文仍可查讀，pending 原因仍可見；無用途級資格時不出適用該卡的肯定提示。
有下游可執行資料要求 pending 引用時，拒絕該執行閉包，不把 pending 當 no-op。

明示舊域格式與寫入端驗證不會自行改寫正式裁定資料。修改 applies_to、正文或其他
已解析內容須明示續版，保留未變的證據、日期與強度；舊引用與版本歷史仍可追溯。
映射成功不自動繼承既有 reviewed／verified，也不重寫過去裁定決定。

## 10. 舊責任對照與交付接口

| 舊位置／做法 | 新責任 | 固定案例／後續 |
| --- | --- | --- |
| translation-contract §1、§7.2 的 shared_jp／unchecked 與 divergence 閘門 | 同卡同面且有效 JP 即取 JP 繁中；display owner 與來源 owner 分開；完整 JP 效果不按 EN ordinal 拼 | FL-023；#496 定義 jp_source basis 與公開格式 |
| translation-contract §2 source_exception | 新入口拒絕此 kind；清查後移除，純 EN 卡另案 | FL-010／FL-023；#500 |
| build-db §5／translation-contract §7.1 區域差異 | 保留 aligned／freshness／divergence 的 DSL、機制與官方 counterpart 資格 | FL-023；不因 JP 譯文顯示授予 aligned |
| 舊六欄 payload、uint/literal/reference、字串 text | §3 語義身分、§4 葉型別、§5 target；NP/form 不入語義 hash | FL-001～FL-005；#498 |
| translation_term 反查集合 | §6 逐 occurrence ＋§8 annotation；文字去重與語義位置分離 | FL-008／FL-013～FL-015；#496 承載 |
| 舊 T ID applies_to | §9 已知域用途級 resolved／pending，未知域引用級 pending，非模板引用獨立保留對帳，禁止假有效引用 | FL-024～FL-026；#498／#499 |

公開 3.0 的 producer／Python 與 TS reader 已同步接線；舊公開 wire 文件不得再作「divergence 禁止 JP 繁中」的政策依據。
新版 producer／reader 必須同步切換 basis 與 annotation，不能用舊 shared_jp_unchecked 冒充新政策。
固定案例是規格，並不代表 normalizer、renderer、全庫譯文、DSL adapter 或 runtime 已通過驗收。

## 11. v6 來源推導與 expression 呈現

`four_layer_derivation.derive_source(raw, source, classifier)` 產生 `DerivedField`，
由其 `bind` 建立 `four-layer-jp-v6` 身分；`verify` 從原始來源重新推導，
再驗完整 frame、binding、語義及接口。正式 compile、classifier、authored 與
SQLite 來源契約只接受 v6，沒有可選家族或舊版執行入口。

此接口只在正式來源辨識上依序套用已封版的容量、具名參數與區域計數／SEP 規則，
最後依最終葉建構常數選項接口；共用型別不授予其他來源句型。
`ChoiceContext` 僅是 PortType，Port 仍只有 name、type、source_role、scope，
不是 LeafType。選項的來源連接必須另由完整來源重建，型別本身不授予可執行資格。

### 11.1 常數選項接口與指示物容量

常數選項接口僅描述同一片段、同能力中已有唯一在前 intro、常數 choice_mode_count 與
按印刷次序連續 1..K 的 choice_index。沿既有完整欄位的 choice 守衛，所有 intro／序號
均須已具 typed 葉且無來源問題；跨片段、孤兒、歧義及缺卡名／數值前置者不加接口。
`DerivedField.interfaces` 保存逐片段的可選 projection；`bind` 建立 v6 frame／binding
並附接口，接口沿既有 `interface_key` 獨立驗證，不進 frame hash；`verify` 從原文重建後比對。
`choice` 能力 scope 匯出
`ChoiceContext/choice_mode_count`，依印刷序的 `choice_1`…`choice_K` branch scope
各匯入 `ChoiceContext/choice_index`。Port 名稱與 scope 相同，沿契約按名稱排序；
實際序號與 mode 由來源葉及 occurrence 重驗，不塞進 Port。canonical、leaf schema、
值、位置與 target 形狀保持不變，原 pending 語義仍 pending；這是 I，不能當嚴格 V 搬移。
不新增 bound mode、跨行或選項歷史關係。

指示物容量僅接受本物件具名指示物的完整容量句：明示「これに」、具名種類、
「カウンターは」、常數與「個まで置ける。」。容量為 `Nat/counter_capacity`，
0..9007199254740991，沿數字的精確 raw occurrence，不是 counter_amount 或 threshold。
具名種類保留字面，`term:object.counter` 以既有 `Concept/rule_term` 閉域引用，
缺概念時保持 pending，缺語言選詞時沿既有 renderer 整欄 fallback。
不新增具名指示物概念或 CounterNP／Form，不建立其他 holder、Stack 或歷史關係。
容量用途須經正常來源 bind／verify 後才能建立 target。

### 11.2 有限 expression 與來源綁定

`four_layer_expressions.DEFINITIONS` 是封閉的 expression 登錄。
結果 measure 為 `card_count`，read_structure 為 `current_zone_count`，
只含下列參數／限定組合，以及明示的一層常數偏移：

| 依序形參 | 固定限定 | 偏移 |
| --- | --- | --- |
| owner、zone | 無 | 無／subtract_offset |
| owner、zone、kind | 無 | 無／add_offset |
| owner、zone、name | 無 | 無／add_offset |
| owner、zone、card | 無 | 無 |
| owner、zone、kind、trait | 無／exclude_this | 無 |
| owner、zone、card、trait | 無 | 無 |
| owner、zone、class、kind | 無 | 無 |
| owner、zone、class、card | 無 | 無 |
| owner、zone、keyword、kind | 無／exclude_this | 無 |
| owner、zone、token_status、trait | token | 無 |
| owner、zone、kind | exclude_this | 無 |

ID 為 `count.current_zone.<依序形參>.<固定限定>.<偏移>.v1`，沒有的部分省略；
例如 `count.current_zone.owner.zone.v1`。偏移的最後一個形參為 `offset`，
對應 `Nat/arithmetic_offset`；其他形參依序對應同名的 `read_*` 角色，
使用原有閉域。專用域 `quantity.count_expression.current_zone.v1`
只收此登錄內的 QuantityExpr，不放寬任何既有常數域。
沒有任意運算式 parser、數值求值或負數截零。

每個 authored frame Target（含 variant）保存自己的 `expression_targets:{expression_id:Target}`，
語言沿用該句 Target 的 `lang`。`ExpressionTargets` 只從該欄實際選中的句子 Target 建立，
pin 換 variant 時連同 expression 譯文一起換；沒有全域同 ID 的單一譯文。
所有權單位是 `(template_id, lang, variant_key)`；每個 BoundTarget 分別查自己的 map，
不先聯集同欄 map、不與 default 合併，也不跨語言借稿。
同一 descriptor 在不同句子可以依指涉、區域、引號、單位或受修飾動詞保留不同呈現，
不改 descriptor ID、來源語義或簽章。expression Target 僅允許 Literal／LeafRef／Form。
Target 引用規則的形參；`Expression.leaves` 依序提供實際 frame 槽名，
兩者不必同名。展開先驗型別、角色、閉域與完整必要參數，再代入實參。
主 Target 以任一節點引用 expression 槽時（LeafRef、Form、CountExpr，
或數量為 expression 的 CardNP／UnionNP），其實參也計入 required 使用閉包；
未引用的 expression 不傳遞使用。閉包沿 wrapper 及 import source leaves 遞迴，map key 必須
恰為可達 descriptor 集合。expression 譯文仍必須呈現全部形參；只在 Form case 判斷讀值不算呈現。
同句同 ID 多次呼叫都逐次驗實參並代入，不能只驗第一個 occurrence；這些呼叫共用一句內的一份稿。
若需要兩種無法無損外提的稿件，明確報衝突，不取首份／末份，也不為中文改語義 ID。
authored reader 對所有語言與未啟用 variant 先驗已可判定的 ID、節點、形參、引用及未用項目；
依 binding 才能判定的來源值、scope 及閉包仍於所選 Target 一起進入正常 `Renderer`，
再驗 required 閉包及實際用到的 expression 集合。缺 expression 譯文以
`missing_expression_translation` 整欄 fallback；多帶未使用的 expression 譯文拒絕。
未知 ID、錯參數、錯角色、跨 scope、缺實參、循環皆屬結構／來源錯誤，不降格為缺譯。
引用閉包、文字範圍、葉路徑、origin
及低信心沿正常 renderer 傳遞，expression 譯文沿用所屬句子 Target 的品質旗標。
Bound 的 import 必須存在且型別正確，來源證明的使用 scope 只能看見本 scope
或祖先 scope 的 import，不能借用同層另一 branch；此登錄域本身不放行 bound。

共用 `bind_recognized` 的 mixed 映射沿正常來源路徑啟用：omitted occurrence
沿同一 recognized 槽的完整 explicit 簽章與實際位置對應，保留 resolution_rule，
不按值尋找 antecedent；不能唯一定位就拒絕。純 omitted 槽仍按既有建構順序配對。
明示數字槽不得偽造成 omitted occurrence。
共用映射可表達某種形狀，不等於來源已核准該省略關係；正常來源 verify 仍須重新推導證明。

### 11.3 具名參數來源

`four_layer_n2_parameters` 在正常來源推導辨識 effect／section 中的下列形狀。
來源位置由既有單位的 raw origins 連回完整 owner 欄位；原有葉、分支順序、
正文／標頭／提醒分段及提醒依附保留。葉辨識成功不代表整個能力已有可執行語義。

| 來源 | 葉與界限 | 保留的邊界 |
| --- | --- | --- |
| 土之秘術付款前綴 | `Concept/ability` 與 `Nat/earth_rite_amount`，1..安全整數 | 能力篩選與歷史支付不產付款量；其他能力參數不改 |
| d6 擲骰或明示擲骰事件後的本文條件 | `Nat/dice_face`、`dice_lower_bound`、`dice_upper_bound`，1..6 | 單點、離散 OR、閉區間分開，區間下界不得大於上界；本文條件不移到事件過濾器 |
| 完整衍生物標頭末端的裸紋章／裝備種類 | `CardKind/declared_kind`，既有 `card_kind.any.v1` | 正文與卡名子字串不取得種類資格，不擴大 selection kind 域 |
| 已分段、指定本回合攻擊力／體力的從者提醒 | 兩個 `Nat/stat_value`，0..安全整數 | 設定值不當增減量，提醒的期間、stat 引用及依附不改 |

付款量省略時，僅土之秘術具名規則給值 1；其 occurrence 為 omitted，
raw／canonical spans 為空，`resolution_rule=earth_rite.omitted_amount.v1`；
與其他純省略槽相同為 required=false，target 可只呈現能力名而維持卡面寫法。
明示數字仍有自己的來源位置。不能因付款葉已辨識而解除同片段的其他數值阻擋。
標頭的種類拼寫與其他標頭一樣留在 canonical；補齊後的完整標頭沿既有 `token_header.v1` 判定。
骰子的 `term:object.die` 引用只隨已辨識的上述點數條件產生，
不全域抽取其他擲骰敘述，也不建立宣告、history、RNG 或未印刷的 else。

正常 Renderer 與 authored 接受以下兩種有限 Form；Form 只有 default case，按下表順序各呈現
一次 typed argument，其間可有 Literal；數字沿普通葉輸出，不產概念標註。

| Form ID／rule | 依序形參與型別／角色 |
| --- | --- |
| `earth_rite.display`／`earth_rite.display.v1` | ability：Concept/ability；amount：Nat/earth_rite_amount |
| `dice.interval`／`dice.interval.v1` | lower：Nat/dice_lower_bound；upper：Nat/dice_upper_bound |

土之秘術沿 `term:ability.earth_rite` 的必要選詞，標註範圍僅含 label；
括號與數字在範圍外。骰子區間 Form 只呈現上下界，不自行加入 die 引用。
缺必要 label 沿既有整欄 fallback，不能用 Literal 或 optional 繞過。

### 11.4 現時區域計數與明示 SEP 零值

來源只接受同一本文片段內唯一明示的 `Xは「…」である。` 定義，
區域必須明示相對玩家及戰場、手牌、牌堆、墓場、EX 或消失區。
限定須完整符合 §11.2 的有限 expression 簽章：單一類型、卡名、職業、種類、關鍵字、衍生物或排除自身；
沒有明寫「卡片」就不增加 `read_card` 葉。
定義與使用點共同決定是否接受，僅辨識傷害、回復／能力值增減、費用降低、
原費用上界、抽牌與具名從者出場的既定使用形狀。
指示物 X、分配傷害、支付 X、紋章計數、不明持有者或區域、表向進化牌堆、
引號讀值、歷史與跨能力／跨片段連接均不因此取得新的來源結構。

通過來源檢查的定義與每個 X 使用點連至同一 `QuantityExpr/count_expression` 槽，
每個 occurrence 保持獨立的原文及 canonical 範圍與印刷順序。
expression 依使用點讀取當下數量，不建立 captured receipt 或跨片段 Bound。
明示加減僅使用登錄的三種偏移形狀，幅度是 `Nat/arithmetic_offset`；
運算方向由 descriptor 保留，結果不轉為 Nat 或截零。
重綁至不同 authored 槽名時，expression 實參也依來源位置同步重映射。
缺必要卡名仍 pending，不能用 Literal 或 optional 略過。

正常 renderer 沿用 `ExpressionTargets` 及正常 labels／Form，
主 Target 可透過 expression 槽間接使用其 required 實參，無須再逐葉重複呈現；
其他 required 葉仍須引用。expression 的形參必須完整呈現（§11.2）。
來源的 whole-line semantic pending 不因局部計數成功而升格。

SEP 僅辨識明示「自己的 SEP 等於 0」的條件數字，沿用 `Nat/threshold` 的零下界。
SEP、持有者、等值及條件作用域保持 canonical 字面，不增加 Concept、Player 葉或新資源語義。
其他資源、非零值與比較不受本項擴張。

### 11.5 context expression 與條件接口

`contracts.four_layer.ContextExpression` 是數值讀值，`PredicateExpr` 的結果為 Bool。
兩者保存 `expression_id`、依簽章排序的 `leaves`、依形參命名的 `imports` 與 `scope`。
`four_layer_context.DEFAULT_REGISTRY` 封閉登錄 descriptor 的結果型別、measure、來源家族、
leaf／import 簽章、capture 與 IR bridge；renderer 的 expression target key 沿用 descriptor ID。
這些是來源契約，不代表 effects schema 或 runtime 已支援。

新葉的 `LeafSlot.expression` 必須與來源讀值結構一致；descriptor、實參連接與 source scope
因此進入 leaf schema 及 frame 身分。`Frame.mode_rules` 保存模式數量規則，也進入 frame payload。
沒有新結構時省略這兩個欄位，B11 的域成員維持原樣；SQLite 的 mode_rules 欄保存明示陣列。
執行證據、runtime capability 與 unresolved lowering 另存，不放在 expression 或 mode rule。

每個新 import 必須指向 `Port.evidence`：精確 `OccurrenceKey`、capture、typed value，
以及本 frame 的 `source_leaves`。引用必須同 owner／face／field／來源 hash，
同一 ability 的當前 scope 或祖先可見；不能從相鄰分支或其他 ability 借值。
ObjectSet 明分單體／集合及 holder、event subject、selection、direct ability source；
ReceiptId、CapturedValue、PlayerRef、ChoiceContext 與事件引用各有獨立型別。
CapturedValue 的 Nat／Bool 與 measure 也必須符合簽章；ReceiptId 簽章必須宣告
`receipt_result`（objects／amount／payment_complete），結果種類不同即拒絕。缺 provider 不補 false 或 0。
來源推導的物件、事件與 receipt ID 是 producer 依來源位置給的符號參照，只用來證明
同一 frame 內連到同一物件世代；不是 runtime 實例 ID，也不能用卡號、卡名或最近事件代替。

入場域 `predicate.entry.provenance.v1` 的角色為 `entry_condition`，只登錄：

| descriptor | 實參 | imports |
| --- | --- | --- |
| `entry.self.from_zone.eq.v1` | `entry_from_zone`：hand／graveyard | holder 單體、`EventRef<EnterField>` |
| `entry.self.from_zone.ne.v1` | `entry_from_zone`：僅 hand | 同上 |
| `entry.event_subject.from_zone.eq.v1` | `entry_from_zone`：僅 graveyard | event subject 單體、該入場事件 |
| `entry.self.method.ability.v1` | 無；固定 AbilityPut | holder 單體、入場事件 |
| `entry.self.method.ability_source_trait.v1` | `entry_source_trait`：僅已登錄 umamusume | holder、入場事件、直接能力來源卡片的事件快照／LKI |

zone／trait 葉沿舊域，以 descriptor 限制組合。事件與 subject 必須是同一物件世代；
能力來源類型還須連到事件的直接來源，不能用持有者、自身類型或更早祖先代替。
事件不存在、kind 錯誤、Unknown 來源都不能成為 Bool 結果。
自身入場事實與 CurrentTurn 歷史分型，不能以歷史有無事件代替自身入場。

共用讀值域是 `quantity.read.context.v1`（`read_value`）、
`quantity.history.current_turn.v1`（`history_read_value`）、
`quantity.count.distinct_name.v1`（`distinct_name_count`）；歷史 Exists 另用
`predicate.history.current_turn.v1`（`history_condition`）。
CountFacts、CountSubjects、Sum、Exists 的結果量綱獨立；HistorySignature 明示 CurrentTurn、
include_execution 與 at_query cutoff，授予能力中的查詢不凍結為授予時的數字。
`read.quantity.v1` 接受這些已型別化的讀值，供 A1 共用使用槽；
`condition.quantity.at_least.v1`／`condition.quantity.equal.v1` 接讀值與
`comparison_threshold:Nat`，供 A7 消費。B11 的既有 QuantityExpr 也能接比較條件，
其舊 descriptor、參數順序與域不擴張。讀值用於傷害、回復或其他 frame 位置時，
不改變讀值本身的 measure，也不按 consumer 新造域。

`quantity.scale.once.v1` 是 A1／A2 共用的一層常數倍率：`value:QuantityExpr` 與
`factor:Nat/constant_multiplier`。基底必須是 registry 明列 scalable 的讀值，
或沒有偏移的 §11.2 現時區域計數（直接重用 B11 expression，不另登錄一份）；
帶偏移的 B11 expression 已用掉這一層。measure 為 `operand` 的其他家族 wrapper
（例如 HalfUp）適用同一規則，因此 wrapper 之間不能互相包住。
巢狀倍率、透過 alias 隱藏的巢狀倍率、predicate、未登錄組合與非正倍率均拒絕。
Nat 常數葉及原 B11 接受域保持原意，沒有一般算式 parser。

`ConditionSlot` 是本片段的 `resolved(predicate)`、
`pending(source_descriptor, source_span, raw_sha256, reason)`，或整合入口的
`linked(source:OccurrenceKey, condition)`。linked 指向同欄唯一 intro 的原始條件，
其 condition 仍區分 resolved／pending；不把外片段的槽放進本 binding。
完整欄位的 `DerivedField.verify` 重建該邊，局部驗證另確認來源等於 ChoiceContext 的來源。
span 為 owner 欄位 raw 座標，
raw_sha256 是該段 raw 文字的 SHA-256。reason 區分
`missing_provider`（已核來源尚待 provider）、`unsealed_source` 與 `ambiguous_source`。
pending 保存來源，不能作真假分支。`ModeCountRule` 保存 base／alternative 的原
exact／up_to 常數 QuantitySpec、ConditionSlot、ChoiceContext import 與 scope；
模式數量判斷不能跨 ability／branch 借條件。

新 expression target 與 §11.2 相同，所有形參都必須呈現。import 的 source leaves
依簽章順序展開成 `<import形參>.<零起算序號>` 形參，也必須呈現。
它們保存在當次所選句子／variant 的 map，wrapper 與 import 閉包內每次呼叫皆用該份稿；
同一 ID 跨句不同稿互不覆蓋，局部稿與主句共用品質及審閱單位。
只有主 target 實際引用 expression 時，required 使用才遞迴傳到這些實參；
遺漏值、錯型別、未知 descriptor、未呈現形參與循環均拒絕。
正常 label、annotation、origin 與低信心資訊仍沿 renderer 傳遞。

家族在各自模組宣告 `Descriptor` 與 `SourceFamily`，由單一 `derive_source` 的整合流程組合。
`DerivedField.registry` 與正常 renderer 使用同一組家族 descriptor 閉域。
callback 接收 raw、DerivedField、Classifier 與可選 CardContext，回傳更新的來源結果；
verify 從同一來源重建。callback 只在原 projection 上補 scope、port 與 mode rule，
不改 projection_kind／discriminator；整行 semantic pending 不因 port 而升格。
家族自用 descriptor 透過 `Registry.extend` 登錄，不得覆寫共同 ID。

可重用的公開合成契約在 `carddb/tests/domains/translations/context_fixtures.py`：
`contract_case("A1" | "A2" | "A3" | "A11")` 提供 BoundTarget、renderer、expression targets
與 ModeCountRule，分別驗 A1→A7、A2→A1／A7、A3→A1、A11→A7。
這些 fixture 只驗 typed 接線，不辨識官方來源，不提升 runtime 可執行狀態。

事件引用亦可明示 `EventRef<Play>`，供獨立的 played_from 家族登錄；
`ImportSignature.event_kind` 封閉事件 kind，`subject_import` 指向同一簽章中的單體 ObjectSet。
共用邊界驗證事件與該主體的世代，Play 與 EnterField 不能互借。
played_from 的來源辨識及 descriptor 由其來源家族負責，不加入 A11 五個 descriptor 的域。

### 11.6 合計、合計約束與固定配對

`four_layer_total_sources.FAMILY` 由整合流程辨識 A8／A10 來源。
`four_layer_totals.DESCRIPTORS` 封閉其簽章，不擴張舊 B11、常數域或 DSL schema。
本家族只登錄來源結構，whole-line 語義仍 pending，不提供 selector 或 DSL runtime 求值器。

| 結構 | 來源與葉 | 降低界線 |
| --- | --- | --- |
| 兩組張數合計 | `quantity.aggregate.source_total.v1`／`aggregate_read_value`；兩個 `quantity.aggregate.source_count.v1` operand，各自保留 owner、zone、name／trait／kind／token；來源連接詞及有限偏移在 descriptor 分型 | BP06-035／036 的重疊計數保留 `overlapping_total`；不以 union／分別計數相加決定 frame 身分 |
| 原費用／攻擊力合計 | `quantity.aggregate.property_sum.v1`／`property_sum_value`；descriptor 保存 property、區域或選取來源及有限 noun 簽章，原費用篩選數字與合計門檻分開 | 區分牌堆搜尋、墓場選取、已看牌集合、戰場及支付材料；來源限定文字保留，不藉此完成同名／不同名 runtime |
| 合計比較 | `predicate.aggregate.selection_bound.v1`、`payment_bound.v1`、`condition_bound.v1`；各自對應 `aggregate_*_constraint`，接 property Sum 與 `aggregate_threshold:Nat`；ge／le／eq 分型 | 選取合法性、支付合法性、效果條件是不同位置；原選取張數仍沿 constant QuantitySpec，不改為任意公式 |
| 互斥固定配對 | `quantity.group.disjoint_pair.v1`／`pair_count`；左右各有 owner、zone 及 name 或 trait＋kind 簽章 | 僅 CP03-022、BP19-040；兩組 Count 取 Min，每張最多使用一次，空組為零；不接受任意名稱、重疊集合、多組或最佳配對 |

共用 owner／zone 或 noun 在原文只印一次時，兩 operand 指向同一來源葉，
不因相等值誤合併不同位置。合計中的 Count 是來源 selector 的結構節點，
沒有額外印刷數字；其 occurrence 為 omitted、`resolution_rule=aggregate.selector.count.v1`，
但 required 仍為 true。主 target 必須經合計 expression 閉包呈現所有 operand，
不能用 optional 或 Literal 隱藏缺引用。新葉與既有葉共享原始 provenance，
expression 依賴經 transform 一起重映射，binding 與 `LeafSlot.expression` 必須一致。

`aggregate.pair.named.v1` 固定兩個已核卡名概念；`aggregate.pair.kind.v1` 固定 mage 類型、
follower／spell 兩個互斥種類。兩者都限制自己的墓場，不能以總張數除二。
`disjoint_pair_count` 僅供有限契約驗證，拒絕同組重複及兩組交集，不負責解析或求值 DSL selector。

`four_layer_totals.lowering` 另列來源 descriptor、operand 槽、具體 unresolved 原因及配對規則，
`executable` 固定為 false。執行證據與去重策略不放進來源 frame；
之後補證據不能反過來改動來源連接詞或常數含義。
BP02-035／036 的法術連擊計數母體、任意算式樹與其他配對仍不在接受域。

### 11.7 名稱關係、名稱片段與雙類型

`four_layer_names.FAMILY` 由整合流程辨識 A3、A5、A9。
名稱種類數沿共用 `name.distinct_count.objects.v1`，結果是 Nat；
不同名選取使用 `name.selection.distinct.v1`，結果是 Bool。兩者不互換。
相對名稱條件保存 holder／選取物件／處理結果來源，不以卡名顯示字猜接。
公開來源入口、required 閉包與正常 renderer 使用相同的家族 registry。
本回合歷史子句內的同名除外屬 §11.10 的歷史查詢；本家族即使單獨啟用也跳過該段，不另編名稱條件。

| 來源結構 | 新域／角色 | 實參與來源 |
| --- | --- | --- |
| 與本物件不同名、與選取物件或處理結果同名 | `predicate.name.relative.v1`／`name_relation` | ObjectSet 或 ReceiptId import，保存精確來源及 capture |
| 選取的卡片各有不同名稱 | `predicate.selection.distinct_name.v1`／`selection_name_constraint` | ObjectSet import，與名稱種類數讀值分型 |
| 名稱包含片段或有限片段 OR | `predicate.name.contains.v1`／`name_contains_filter` | 必要 CardName 引用，角色 `name_fragment`；沿既有 `card_name.any.v1` 詞條來源，不另建片段清冊 |
| 正文兩個類型同時成立 | `predicate.traits.all2.v1`／`trait_conjunction` | `filter_trait_left`、`filter_trait_right` 各自沿 `concept.trait.v1`；kind 保留獨立引用，已辨識的 kind 葉沿用原角色 |

片段來源以原始引用位置還原，不能把正規化後的名稱 placeholder 當字典查找鍵。
`CardName/name_fragment` 葉顯示「『日文來源片段』（繁中提示）」，正文說明為
「日文卡名含有」或對應的否定條件；有限 OR 的每個片段也沿用此規則。
引號及提示括號由同一個葉輸出，模板不重複包引號。繁中提示取原本已選的繁中標籤，
只輔助閱讀，判定仍依日文；缺少繁中選詞時只顯示日文片段，不自造提示。
繁中提示與日文片段字串完全相同時不輸出括號提示；僅做字串相等比較，不做繁簡、異體或其他正規化。
實際輸出的標籤各保留引用及粗體 annotation，同一來源葉的位置涵蓋其完整呈現，
依賴及低信心取實際使用的標籤。缺少日文標籤時整欄退回來源，
不改用繁中片段；完整卡名引用仍顯示已選譯名。
完整名稱、片段條件與合作名稱不因中文顯示字相同而合併。
片段 OR 僅登錄單一、二、四、十個來源片段的有限簽章；不提供一般布林樹或名稱集合生成。
類型條件只接受正文連續兩個已登錄類型及 kind；三個以上類型、token header、
token 身分與兩個 selector 擇一保持來源結果。

import 的物件及 receipt 識別字是由精確來源定位建立的符號，不是對戰物件或結算結果。
holder 以同一能力的來源符號共用；離場事件主體使用 event_subject／LKI，不能冒充 selection。
receipt 必須有同能力／分支內唯一的公開或消滅 producer；缺少或歧義時保留具名來源問題。
類型與 kind 所需概念缺失時也保留 pending，不填入未登錄的必要引用。
這些結構不執行名稱比對，也不授予 DSL 可執行資格。
同名生成保留 PredicateExpr 關係，交由 emission 骨架消費；Bool 不能傳給 token 名稱參數。
多規則名稱的唯一化或選名仍屬 unresolved lowering，不取任意第一個名稱。

### 11.8 選項使用限制與同欄跨行選項

`four_layer_mode_context.FAMILY` 明示啟用 A6／A7。`mode_sources` 從原欄位定位唯一
intro、連續的 1..K 選項及數量替代子句；卡名、引用的能力與提醒文字不參與外層配對。
跨行只連立即相鄰的選項行，空行、獨立能力頭、巢狀 intro、重複或錯序序號都不連接。
來源限制為同 owner／face／effect 或 section；不把整欄所有序號合併成一組。

A6 的 `mode.unused_this_turn.v1` 是 `mode_reuse_condition:PredicateExpr`，匯入
`ChoiceContext`，保存 AbilityInstance／Mode／Turn 的來源關係。來源推導的 ability 與
CurrentTurn 識別是待 lowering 的來源參數，沒有建立 runtime 使用紀錄。執行時必須
以實際能力實例及回合取代，重新授予產生新能力實例；不得改成按卡名共用。

A7 在參與來源片段保存 `ModeCountRule`。intro 的有限區域張數、表向進化牌堆具名張數、
關鍵字門檻及主戰者生命條件使用家族自有 descriptor，以既有的 owner／zone／名稱／
`existence_count` 葉作實參，保留原有常數 QuantitySpec 的 exact／up_to／at_least 含義；
因此不另建 `comparison_threshold:Nat` 葉，也不改用 §11.5 的 `condition.quantity.*`。
比較本身不擴張舊 B11 接受域。主戰者生命條件使用新角色 `mode_condition_owner`，
不借用卡片集合的 `counted_owner`。已由其他家族產生、且 occurrence 落在該條件內的
typed predicate 直接消費；條件 provider 的 scope 與 imports 沿原 projection 保留，
不以字面值相同配對。

單獨啟用此家族時，其他片段的條件來源邊以 `choice.condition.source_edge.v1`
保留 span／hash 及 missing_provider。§11.11 的整合入口將它接成 linked，
指向唯一 intro 的 OccurrenceKey 及原條件；未封條件仍是來源 pending。
A11 六種入場表面與 A2 的本回合進化存在，在 provider 尚未合流時分別以
`predicate.entry.provenance.v1`／`predicate.history.current_turn.v1` 標 missing_provider；
其他條件（含相似但未封的入場或本回合寫法）保留 unsealed_source。
這些都是來源候選，整合後須重產身分，不能直接當正式封版 ID。

前行 optional cost 只在立即相鄰、唯一且額外費用數字一致時建立來源邊：producer
匯出 `CapturedValue<Bool,payment_complete>`，數量條件匯入相同來源參數。缺少唯一
producer 時保持 pending；沒有猜測跨能力付款。這不代表已有付款執行證據，沿 R-0006
驗打出成本與模式決定時序前，lowering 仍不可標為 executable。

### 11.9 A1 讀值與 A4 宣告 X 的來源

`four_layer_n2_reads.FAMILY` 由正常整合流程載入。家族自有簽章在 `four_layer_n2_read_descriptors`，沿
`quantity.read.context.v1`／`read_value`；現時區域計數直接重用 B11 的
`count_expression`，不重複登錄 descriptor，也不改既有角色或 domain 成員。
同一讀值可用在傷害、回復、衍生物數量、能力值與費用位置，consumer 仍保存於來源骨架。

來源只接受已封的區域計數、物件攻擊力／體力／原費用／有效費用、PP 最大值、指示物、
處理結果、骰面、表向進化牌堆與原費用種類數。自身的即時讀值、選取快照、LKI、事件主體、
receipt 固定結果及宣告參數各有 import 簽章，不能只因結果為數字而互換。
選取的物件在讀值前被破壞、消滅，或移到墓場、手牌、牌堆、EX 區時，讀值改用 LKI。
處理結果張數引用 `ReceiptId<objects>`，取下指示物數引用 `ReceiptId<amount>`；
兩者與 `payment_complete` 都不能互換。
有名稱、類型、職業、關鍵字或指示物引用時，必須沿目前採用的來源概念；
指示物種類的名稱維持來源骨架，引用沿既有通用指示物概念，與 §11.1 相同。
沒有唯一 producer 或缺概念時不補 Literal、不挑第一個、不建立可用 binding。

直接讀值與明示 X 定義使用相同 descriptor。定義與每次使用分別保存來源 occurrence；
後續的 X 門檻也引用同一 X，門檻數字另存為 `Nat/comparison_threshold`；
`「…」と同じだけ-Nする` 的每單位減少量是 `Nat/cost_reduction_unit`。
來源 anchor 與 expression 的實參互斥，expression 結構、實參連接及能力／分支 scope 進入 leaf schema。
一層常數倍率沿 §11.5；HalfUp 與已封的單一偏移各有有限 descriptor，不提供任意巢狀算式。
授予能力另立能力 scope，其中的自身讀值指能力持有者；不把文字提供者當持有者。

A4 的 `read.parameter.chosen_x.v1` 引用 `CapturedValue<Nat, declared_x>`，capture 為
`play_parameter`。同一能力／分支的 PP、指示物及材料支付 X 與其效果共用一個宣告值；
重複的 PP 費用仍保留每個 occurrence。付款完成是獨立的 `ReceiptId<payment_complete>`，
不拿 receipt 實付總額替換 X。原骨架保留 exact／上限、資源種類與各 consumer 位置。
付款合法性、零支付、取消與失敗路徑仍是 DSL lowering 的驗收事項。

source port 的物件、receipt 與 binding ID 是由來源位置及 scope 建立的符號；
它們不是 runtime 物件、對手隱藏資訊的固定身分或已執行的付款結果。
同欄唯一、明示的追加費用來源可以供後行讀取該次追加費用的結果，並保存前行來源座標；
其他能力／分支不能借用此 port。歷史讀值由 §11.10 的 P2 producer 擁有，
本家族不再重編或標記 history provider pending。名稱種類數消費 §11.7 的既有讀值，
將 X 定義與使用點接到同一槽；缺唯一 provider 時保持具名 pending。
X 在名稱片段 selector 外時，其定義可獨立辨識，selector 由 P3 擁有。
不以 runtime 缺口替來源填入數值。

### 11.10 本回合歷史與單次事件來源 producer

`four_layer_history.HISTORY_FAMILY` 與
`four_layer_provenance.PROVENANCE_FAMILY` 由 §11.5 的整合流程啟用。
前者登錄有限歷史事件及篩選簽章，後者是 A11 與 played_from 的來源辨識入口。
這兩個家族不擴張 B11 或常數域。

歷史來源的 descriptor 區分事件、固定篩選形狀、aggregate 及 include_execution；
玩家、類型與卡名等變動 operand 另有 required 葉，不從中文或相同數值推測。
玩家 import 分 actor、subject_owner 與 resource_owner；物件或生命資源的擁有者不等於造成事件的行動者。
打出張數使用 CountSubjects，攻擊／UB／生命增減的次數使用 CountFacts，
生命增加量及土之秘術取走的指示物數使用 Sum，單純是否發生使用 Exists。
生命減少次數不等於減少點數。雙事件條件保存兩個 Exists 再 And；
兩類卡片的打出張數則是另一個有限篩選簽章，不能互代。
所有查詢使用 CurrentTurn／at_query；明示排除這次執行時才關閉 include_execution。
同片段唯一 X 定義及使用點保存各自 occurrence，不跨能力或編號選項連接；
一層倍率沿共用 wrapper，不改量綱。

來源 scope 依能力標頭及編號選項分開，並保存本文、能力可使用限制、打出限制或模式數量的位置。
授予能力內的歷史使用獨立能力 scope，供未來能力執行時查詢，不能 capture 為授予時的數字。
同片段兼具未來誘發與歷史條件時，只辨識回溯子句，不把期間誘發或下一次事件當成歷史。
全體從者進墓場的計數不加上原文沒有的玩家限制；自身增益及排除持有者／同名的篩選另需 holder import。

A11 沿 §11.5 五個 descriptor，保留本文、能力可使用、模式數量及事件本文四種位置。
事件主體條件必須有唯一、同片段的已辨識入場誘發來源；缺少時保存 typed 結構與
`missing_unique_entry_event_provider`，拒絕 active binding。
條件的原文位置保持在其子句內，不能吸收前面的傷害或提前改成 trigger filter。
直接來源類型沿 required `entry_source_trait`，不能以進場卡自身類型代替。

played_from 另用 `play.self.from_zone.eq.v1`、
`predicate.play.provenance.v1`／`played_from_condition`，
`played_from_zone` 沿 `zone.single.v1`，本版僅 EX／手牌。
其 imports 是 holder 單體與 `EventRef<Play>`，不使用 EnterField 或 A11 descriptor。
能力要求打出與能力直接放場仍是不同來源；非手牌也不等於 AbilityPut。

producer 的物件、世代與事件識別是由精確來源定位產生的符號 handle，
表達 runtime 必須提供的關係，並非證明遊戲中已發生該事件。
一般 typed 邊界驗 kind、世代、scope、capture 及直接能力來源，
正常來源 verify 以相同來源重新推導逐項比對。
EntryFact 的持久連接、Unknown、來源快照／LKI、授予能力持有者與查詢時點，
仍須在 DSL schema／loader／runtime 的 lowering 驗收完成；缺證據不能自動回傳 true、false 或 0。
局部 typed 辨識不提升整段 semantic pending 或中文的審閱資格。

### 11.11 十一項來源家族的整合

`four_layer_integrated.FAMILY` 由唯一正式入口 `derive_source` 啟用。它依來源依賴執行歷史、
入場／打出來源、名稱、合計、讀值／支付 X，最後接選項 consumer；
authored 只載入正式 v6 定義與譯文，來源家族由來源推導決定。
整合 registry 沿各家族的閉域簽章，不另建 parser 或常數域。

同一來源構造只由一個 provider 擁有：

- 歷史子句內的同名除外及歷史讀值由 P2 擁有，P3／P1 讓位。
- 名稱種類數由 P3 產生；P1 將同一能力內的 X 定義及使用 occurrence 接到該數值槽，
  不重建另一個母體或多包一層讀值。
- 合計由 P4 保存骨架，合計 selector 的名稱排除、不同名及雙類型使用 P3 的 typed predicate。
  `aggregate.sum.deck.filtered_trait_card.original_cost.v1` 消費名稱排除，
  `aggregate.sum.graveyard.filtered_kind.original_cost.v1` 消費雙類型，
  `aggregate.sum.deck.distinct_filtered_kind.original_cost.v1` 同時消費雙類型與不同名。
  這些仍使用 `quantity.aggregate.property_sum.v1`／`property_sum_value`；
  predicate 保留自己的域、角色及 imports，不以合計 descriptor 的名詞形狀再編一次。
- P5 消費 P2 的 A11／歷史條件；表向進化牌堆具名張數由 P5 直接使用既有 B13 葉。
  其他片段的 mode rule 透過 linked 消費同一 intro 的條件；付款行也沿此來源邊，
  producer 的自我 import 不視為另一條跨片段邊。未封存在條件仍 pending。

`verify_claims` 在每個 provider 後確認已擁有的 descriptor 及原始 occurrence 保留且唯一；
新增使用點可以延伸同一讀值的 occurrence，不能取代原 provider。
相同來源 anchor 的重複構造會報錯；同片段的不同構造仍可組合。
descriptor ID 可以把原文字面編進構造而不留槽，所以全部 provider 後另由 `verify_nesting` 檢查：
某個 context／predicate 的 occurrence 若落在另一個節點直接葉的原文範圍內，
必須是該節點的 operand（或反之），否則視為同一構造被編兩次而報錯。
變換槽名時同步重映射 expression DAG 與既有 port 的 source_leaves，
不能因槽名剛好仍存在而借到另一個值。

`four_layer_ability` 共用能力標頭，包括出走時、ドライブ獲得時、アドバンス起動，
進化／食事／憑依僅在接費用標頭時切開。整合 scope 依完整 raw 欄位的能力、
授予能力與編號分支建立；本文、能力可使用、打出限制及模式數量保留 sequence scope。
同一能力的 holder 共用精確來源符號，事件引用同步保留相同物件世代；
授予能力仍有獨立 holder，不能合併為外層能力的物件。

打出時的 optional cost 決定是 `CapturedValue<Bool,payment_complete>`，
支付完成結果是 `ReceiptId<payment_complete>`；兩者的時點與型別維持分開。
這些接線不執行付款、歷史、名稱規則或入場來源求值；lowering／runtime 仍須各自驗收。
新中文 target 仍經相同 required 閉包與 annotation renderer；缺選詞保持具名問題，
不以 Literal 或 optional 隱藏。中文重對齊仍須通過相同來源驗證與 renderer，
不因來源辨識完成而啟用。

### 11.12 有限來源形狀與構築上限

正式來源版本為 `four-layer-jp-v6`。既有 N0／N1／N2 仍經同一 `derive_source`，
沒有按家族混用版本的入口。`four_layer_n3_numbers` 只在完整卡片效果／section 的
具名形狀及同欄提醒 anchor 內補數值證據，不登錄新的 expression、Form 或 DSL 語義。
lexical numeric veto 只有在整體來源守衛成立且無其他問題時才能解除；原先的低信心、
整段 semantic pending、譯文來源與審閱資格不因此改變。

| 來源家族 | 有限接受形狀與保留界線 |
| --- | --- |
| N01～N03 | 二／三枝、純擇一或純並列的具名生成，每枝獨立明示數值且同單位；各自生成則沿既有不限名稱數的單一 count 與單一印刷 occurrence。完整掃描清單，拒絕長清單尾段或混合連接的新增生成。既有 compound action 接受域不收窄。 |
| D01～D02 | 單名稱的戰場／EX 替代目的地，及三名稱各自生成的有限雙目的地。完整目的地片語固定在 canonical，不借選取來源的 ZoneSet 域。 |
| R01～R03 | 緊鄰、同能力、唯一具名生成或抽牌的明示數量替換。至多替換只放寬具名生成，另驗採用種類與單位；非搜尋的其他動作仍拒絕至多替換。精確存在條件、原張數與替代張數各自保留。替換句後可接同能力的後續句。 |
| C01～C03 | 完整名稱的現時存在量、OR 左側單 trait／class 的至少門檻、完整墓場集合及明示單一玩家。等於與至少不互換；OR 右側既有葉不重編。 |
| S01～S03、S10 | 具名／明示種類的有限選取與戰場省略、兩名稱共享總數、兩種具名物件各自數量。名稱的種類證據取全部確認採用的同名定義；不同印刷來源可供相同完整名稱，異種或缺失證據仍拒絕。不從單位推種類，不新增省略玩家或區域葉。 |
| S04～S09、S11 | 檢視後公開留頂、表向進化牌堆翻面、墓場與消失區共同具名選取、各玩家留下數量、場與 EX 的 trait/token 選取、手牌置入牌堆二端擇一、具名進化牌堆打出。每形各驗局部完整動作；不新增通用多區域、端點或任意張數域。 |
| P01～P03 | 明示追加費用、Fusion 的有限材料與引號能力減費、EX token 消滅連用後接手牌本物件移動的成本鏈。材料數、原費用門檻與減費幅度分槽，不建立付款結果或跨能力 receipt。 |
| T01、L01 | 在場期間 class/token 靜態集合只新增明示 token 葉；不推 follower。精確存在條件後的頻率沿既有 duration_count／repeat_count，原條件葉保留。 |
| 構築張數上限 | 完整名稱主牌堆上限、既有本物件進化牌堆上限、排除單名稱片段後各牌堆分別上限，統一用 construction_copy_limit。各牌堆字面保留，單一印刷數字不偽造多個 occurrence。 |
| 起始護符提醒 | 同欄標記後、明示一種不同名稱的構築上限，使用 construction_name_kind_limit；完整 Nat 域與來源限值分開。 |
| 四段原費用種類門檻 | 同一進化時能力、完整首段主語、嚴格遞增的四段累計條件；四個數字沿 threshold，不新增 read_value。 |
| 十費用逐項支付 | 僅依序明列 1..10，每費用各一張；十個 cost_value 與單一 count 使用完整 Nat 域，完整 tuple 與一張由來源守衛限定，不壓成總張數。 |
| 提醒及幅度承接 | 課程 1 的具名 EX 消滅付款；同欄兩段 NC 累加提醒並逐組驗正文參數；同一打出成本的三段 SC 替換減幅；承受對手能力傷害的明示減幅。保留提醒 Unicode、符號、引號及累加／替換字面，不跨能力承接。 |

生成、直接移動與支付沿 `Nat/count`，選取及至多沿 `QuantitySpec/selection_count`，
存在比較沿 `QuantitySpec/existence_count`。本節增加的角色只有 §4.1 的兩個構築上限角色；
所有數值域維持非負安全整數。新來源辨識只建立必要葉與正常 binding，
缺少繁中 target 仍回報缺稿，不能以來源解鎖宣稱整欄中文完成。

有限 flat 篩選中的 trait、明示 card／kind 與 token 仍須有 required 葉；
其來源拼字留在 canonical，沿既有型別與引用域驗回位置，不為了新增葉重寫骨架。
缺少必要 trait／card 概念時保留 `missing_n3_filter_concept`，不能略過該葉完成 binding。
構築限制中被否定的名稱片段使用 `CardName/name_fragment`，不標為完整名稱引用。

**Token 詞彙引用。** 來源正文、提醒與 Token 表頭中尚未被其他葉持有的 `トークン`，由共用詞彙補齊
建立必要的 `TokenStatus/token_reference` 葉，沿 `token_status.token.v1` 的 `token`
值讀取 `term:object.token`。引號內的完整名稱與既有 Token 篩選葉保留原引用，
不建立重疊位置。補齊只增加詞彙依賴，不改來源分段、既有語意、數值或接受條件。
模板正文與提醒均引用該葉，不將譯詞寫死在 Literal；改動葉結構時同步重算共用
模板 ID 與譯本引用。繁中選詞為 `Token`，與相鄰中文字不加空白。

**類型名稱詞彙引用。** 已登錄的 `term:rule.trait_type` 以 `rule_term` 類別釘住日文 `タイプ`，
繁中選詞為「類型」。同一詞彙補齊流程對正文、提醒與 Token 表頭中尚未由其他葉持有的
`タイプ` 建立必要 `Concept/rule_term` 葉，沿 `concept.rule_term.v1` 引用該詞彙。
完整名稱、圖示與既有結構葉的範圍不重複抽取；個別類型仍引用 `term:trait.*`。
模板用 LeafRef 接在類型名之後，授予與否定句型的其餘文字不變；改動共用選詞即可更新譯名。
此補齊只建立顯示詞彙依賴，不增添可執行語意或擴大來源接受條件。

`four_layer_n3_references` 在完整構築限制、四段種類數條件、十費用支付、
三段 SC 替換減費、承受能力傷害及三種提醒內補齊已封的固定詞彙引用。
玩家／區域詞彙沿既有 `Concept/rule_term`，明示卡種沿 `CardKind/filter_kind`；
已有結構葉持有的位置仍由原葉負責，不建立重疊來源。
起始護符沿 `term:object.start_amulet` 的 rule_term 類別，兩個構築牌堆位置分別引用，
提醒維持 raw Unicode，不開放 N1 的玩家／區域推導。
只有明列詞彙及完整來源守衛可產葉；未登錄或類別不符保留
`missing_n3_required_reference`，不建立可用 binding。

### 11.13 #749 有限池、holder、事件與參數接口

正式版本為 `four-layer-jp-v6`。本節與 §11.12 使用相同的正常 `derive_source`、
`bind`、來源重建驗證、SQLite reader 與 renderer；沒有另外的執行入口。
`source_relations` 不增加任何種類。projection 的 provider／consumer 只用 port、
scope、OccurrenceKey 與 capture 證據連接，操作次序保留於 canonical 來源。

新增 port value 恰為以下兩型，均納入既有 PortEvidence 的 tagged union：

| 型別 | 欄位與限制 |
| --- | --- |
| PoolContext | `kind,pool_id,source_kind,members,receipt,scope`；generated 的 members 是一或兩組有序且不同名的 CardName／Nat slot pair，receipt 必為 null；search_result 的 members 必為空，receipt 必指同操作的 objects receipt。 |
| AbilitySetRef | `kind,holder_generation,ability_kind,source_selector`；holder_generation 為目前能力的 holder 世代，ability_kind 恰為 `term:event.evolve`，source_selector 為這次選取的完整 OccurrenceKey。不是 ObjectSet，也不是前次執行 receipt。 |

P01 的具名生成池、P02 的十名稱搜尋實際結果池、P03 的生成三張且填滿場容量形狀，
均由同一序列 scope 供應第一次放置。`verify_pool_allocation` 檢查 pool、paired export、
實際 objects receipt、第二次放置的 remainder 及 capture／scope／引用型別。
P03 另要求 live `source_actor`；場容量是操作限制，不產生 Nat 葉。
搜尋不足與放置被替代時，remainder 的含義仍是實際池成員減去第一次實際放置成員，
不能以要求數量相減；本階段來源驗證不執行遊戲容量、替代或失敗處理。
lowering／runtime 必須保證不同 allocation 不重複使用同一物件世代；本階段來源驗證不執行此約束。

n1／n2／n3 接口補充：第二次放置只匯入一份 ObjectSet，新增
`ObjectSetRef.origin="pool_remainder"`，其 `pool` 與 `placement` 皆必填，
分別指向同一 PoolContext port 與第一次實際放置的 objects receipt export。
其他 origin 禁止這兩欄。remainder evidence 涵蓋第一次與第二次放置的來源座標，
paired export／import 的身分與證據一致；不再匯入一份重複的 pool 或 receipt。
最後一次放置沒有後續使用者，因此不匯出無用 receipt。
既有扁平放置池仍沿原登錄，不因本節而一般化。

下列是新增的 **source_role 接口用途碼**，不是數值葉角色：

| 用途碼 | 含義 |
| --- | --- |
| pool_source | 同一 PoolContext provider 的 export |
| pool_allocate_any | 從實際池任意分配的操作入口 |
| pool_allocate_exact | 有明示數量的池分配入口 |
| pool_fill_battlefield | 填至場容量的入口，另需 source_actor |
| pool_move_remainder | 將第一次放置後剩餘成員移至 EX |
| pool_remaining_after_first | 與第二次入口同身分的 remainder export |
| pool_search_result | 搜尋操作實際 objects receipt export |
| source_actor | 當前能力控制者，PlayerRef／live |

非池用途另沿既有 read_subject／read_event／read_receipt／read_player、choice_context、
declared_x、payment_complete 用途；本節的具名 provider 用途碼為
`history_actor`、`declared_dice_face`、`dice_event`、`dice_result`、`ability_source`
與 `highest_satisfied_holder`。它們同樣是接口用途碼，
各自受上述封閉簽章與 exact source 重建限制，不是新增數值角色。

H01 的 EX 指示物兩段比較使用同一張卡、同一物件世代、live 同種類指示物讀值。
這是 [#749](https://github.com/gbaian10/sve-kit/issues/749) 的維護者專案詮釋，
不是官方裁定；適用範圍只限 BP21-060／061／063／065／069 的五個已鎖定 face revision，
由 `HOLDER_REVISIONS` 記錄，不成為其他來源的預設 holder。
見證必須讓已達最高門檻成立：4／10 只可選 10；4／9 可選任一；10／12 可選任一，
但兩次讀值必同世代；3／2 或空集合為 false／false，不捏造 holder；0／10 只可選 10。
錯種類、換世代、換能力、換分枝都不可借用。這些 K5 見證條件是 lowering／runtime 的約束；
`verify_counter_witness` 提供參考檢查，供後續 lowering 使用，本階段來源驗證不執行。
前段五個 4 隨 typed read 使用 `comparison_threshold`；沒有 typed read 的舊 `threshold` 不改。

H02 的 `counter_amount` 是 up_to 常數 QuantitySpec，與目前量、移除量分開。
H03 保存 Stack 進場／移除的 `Nat/counter_amount=1`、`Nat/threshold=0`、
版次原有的移動／破壞措辭與子能力 scope；移交所有指示物不捏造額外數字。
H04 的具名生成一張與相鄰本文 `+N` 分開，提醒文的「+する」不補上省略數字。
R01 保留本次實際捨棄 receipt 加 offset 的既有讀值，只補 deck 至多一張的 existence_count。
A01 選取並再次使用這個 holder 的一項進化時能力，沿 `Nat/count=1`，不增加數值角色。

新增 descriptor 恰為以下五個；參數與 imports 的順序是封閉簽章，
必要引用全部經原有 expression target／標籤閉包，不能靠 Literal、optional 或 fallback 通過：

| Descriptor | 參數 | Imports 與結果 |
| --- | --- | --- |
| history.dice.face.exists.v1 | owner:Player/history_owner=self；face:Nat/dice_face，1..6，來源限 6；die:Concept/rule_term | actor:PlayerRef/current_turn；CurrentTurn、exists、include_execution=true、cutoff=at_query；Bool/truth |
| predicate.dice.matches_declaration.v1 | lower:Nat/declaration_lower_bound=1；upper:Nat/declaration_upper_bound=6（兩者 domain 1..6）；selection:QuantitySpec/selection_count exact 1；die:Concept/rule_term | declared:CapturedValue/declaration/Nat/declared_dice_face；event:EventRef/at_event/DiceRoll；result:CapturedValue/receipt/Nat/die_face；Bool/truth |
| read.result.destroyed_follower_by_player.v1 | kind:CardKind/read_kind=follower | receipt:ReceiptId/receipt/objects；player:PlayerRef/receipt，依破壞時控制者分組；Nat/card_count |
| read.event.damage_amount.v1 | 無 | subject:ObjectSet/at_event，單體 event_subject；event:EventRef/at_event/Damage，subject 必相同；Nat/damage |
| condition.quantity.at_most.v1 | value:QuantityExpr/read_value；threshold:Nat/comparison_threshold | 無；來源只限 Fusion chosen X 的上限 2；Bool/truth |

D01 是當回合自身擲骰結果存在查詢，不是目前骰子物件的讀值。
D02 宣告的上下界、宣告值與結果骰面使用不同名稱及 capture，
宣告在擲骰前，event 與 result 指向同一次擲骰的 exact source／scope，沒有 nearest-event fallback。
P04 的 receipt 是本次全體破壞實際結果，分組 divisor 是 `Nat/group_divisor`（最小 1），
與要求破壞數及具名生成一張分開。R02 讀當次 Damage event 實際 amount，
與要求傷害或目前體力分開。

D02 接口補充由協調者隨 #749 封版定案：`EventRef<DiceRoll>` 必填 `actor:PlayerRef`，
其含義是發動當前能力的控制者，與 P03 共用 `source_actor` provider；
DiceRoll 禁止物件 subject。EnterField、Play、Damage 仍必填 `subject:ObjectGeneration`，
禁止 actor；條件型別驗證拒絕錯型與缺欄。matches_declaration 仍恰有三個 imports，
actor 只記錄擲骰者，不參與宣告比對。這是接口補充，沒有增加遊戲語義或骰子執行引擎。

新域恰為 `predicate.dice.declaration_match.v1` 與
`quantity.choice_mode_count.context.v1`。既有 `predicate.quantity.compare.v1`
只增加 `condition.quantity.at_most.v1` 成員。
新數值角色只有 `declaration_lower_bound`、`declaration_upper_bound`；
`declaration_match` 是 predicate 角色。
新 capture 只有 `declaration`，既有 declared_x 的 play_parameter 含義不變。

X01 的 Fusion 材料 X、放置指示物 X、定義 X 共用同一 chosen_x provider；
原費用下限 1、材料 trait／card、上限 2 及獨立 payment_complete receipt 全部保留。
X02 的新 QuantitySpec domain 只允許 `mode=up_to`：
一層 B11 `count.current_zone.owner.zone.card.trait.v1`（self、battlefield、單 trait、明示 card），
或既有 `read.parameter.chosen_x.v1` 支付 provider。
兩者都須有同能力唯一的 ChoiceContext，恰三項有序、不重複、連續選項，
intro 的 X 與 provider 來源可追，expression 與 imports 的必要葉全部呈現。
不能放 offset、history、其他算式、循環或多層依賴。
舊 `quantity.choice_mode_count.constant.v1` 與 ModeCountRule 的常數驗證不變。
K2 只另轉移 BP17-056 的既有 frame，中文 AST 依精確來源用途重接至新數量葉，
up_to 接管原「至多」前綴，既有文字須保持相同，並交 #750 重審。

### 11.14 固定動作文字與必要詞界線

`four-layer-jp-v6` 的十種原費用逐項支付句及課程提醒，保留既有完整來源句型、數值守衛與 pending 分類；動作「消滅」留在 canonical 固定文字，不另產生 `term:action.banish` 的 Concept/rule_term 葉。其他卡片、玩家、區域、能力與數值葉仍須完整引用；不改 glossary 概念或基礎選詞，也不新增動作受詞 form。

這是全庫來源葉規則換版，既有正式定義與引用同步重鍵；版本以外的轉移依 §9 比完整 payload，葉結構變更按精確來源 occurrence 重建。canonical 相同的 occurrence-scoped pending frame 仍保留各自 scope，不因中文完成而改成 resolved。
