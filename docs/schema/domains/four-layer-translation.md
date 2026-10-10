# 四層翻譯共用資料契約

本文件是 [ADR-0016](../../adr/0016-four-layer-translation.md) 的共用契約；#498 已實作 N0 的 reader、DDL、來源 binding 與渲染。
區域／階段葉、可選範圍、省略持有者與 NC 全名等價等 N1 行為由 #380／#499 承接；本文不是 DSL 語法。
用語依[術語表](../../terminology.md#四層翻譯)；來源選用依[翻譯契約](translation-contract.md)。
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
| template_translation；templates/values | template_id,lang | `{template_id:FrameID,lang:Lang,target:Target}`；text 改 target，A/B/C 驗引用與完整使用，FL-005 |
| template_translation_variant；templates/values | template_id,lang,variant_key | 上列加 `variant_key:Code`，不可 default；不是 semantic_variant，FL-001／FL-005 |
| template_translation_candidate；templates/candidates | source_kind,candidate_id,lang | 保留未啟用的七欄草稿，依翻譯契約 §2.1；不能被 LeafRef／pin 引用，FL-029 |
| translation_form；forms | id,lang | §5 FormDefinition；封閉文法表，不保存官方全文，FL-012 |
| template_match；overrides | context_key | `{context_key,source_hash,matches}`；matches 為 null 或 `{frame_id,source_span,values}` 陣列；values 依 §4，A/B 驗唯一匹配、來源與全覆蓋，FL-007／FL-008 |

新增 `forms/<sequence>.yaml`；definitions／values／candidates 及既有 glossary／overrides 路徑不改。
template_match 不容手填未驗來源值；null 僅撤回人工指定，回本次自動匹配。
context_key.variant 指 context_assignment 的具名 variant（缺省 default），不是 §8 建置後的 context 聚合鍵。
source_exception 整個 kind 移除，divergence/default_jp 都沒有新入口；切換必須列出舊記錄及處置，遇遺留記錄拒絕，不靜默跳過。
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
合法域是來源限制，不能從 target 推導。可省略的槽 required=false；省略時 values 不含該 key，不用 null 造值。

| type／值 | role／合法域與語義界線 | 案例 |
| --- | --- | --- |
| Nat／UInt；Ordinal／1..安全整數上限 | count、damage_amount、counter_amount、repeat_count、threshold、choice_index 分角色；負號不是幅度值 | FL-004／FL-021 |
| Player／具名 Code | self/opponent/each 等由構造的 domain 明列；source omission 不能全域默認 self | FL-020 |
| ZoneSet／排序唯一 ZoneKind 陣列 | counted_zone/source_zone/destination_zone 分開；各元素引用型別登錄及其詞彙對照；deck top/bottom 是 ordered position，非 ZoneKind | FL-003／FL-017 |
| CardKind／vocabulary 引用 | counted_kind/filter_kind；vocabulary 的 kind=type，code 必存在 | FL-003／FL-004 |
| Concept／TermReference | keyword/trait/ability/rule_term 各自有 category 與角色限制；`{kind:glossary,key:ID}` 或 `{kind:vocabulary,key:[kind,code]}` | FL-005／FL-018 |
| CardName／`{kind:card_name,term_id:ID}` | 名稱概念 FK；整個名字是葉，不抽名字內的術語，不因同名猜 card／token 身分 | FL-013 |
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
Player／ZoneKind／Phase／Stat／TokenStatus 等 Code 的顯示名稱，須在該型別登錄中明示 value→TermReference 的對照；
B 驗對照存在、型別吻合並沿同一選詞產生位置，不能用 Code 字串或中文猜概念（FL-005／FL-013）。
數值本身及純文法片段無概念名稱時不造 TermReference。
ZoneKind 本身不是完整目的 ZoneRef；後者還要持有者。union 的分支限定不能上提成全域 filter；
異種聯集須具名 sum type，group 的各組數量不能壓成單一 union。

### 4.1 N0 產生登錄

N0 使用全庫版本 `four-layer-jp-v1`。一般契約保留未來型別，N0 authored／來源分類只接受下列封閉組合，
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

N0 的 resolved key、來源 role、投影及能力作用域由 `contracts/n0.py` 封閉登錄。
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

## 5. target、form 與部分 NP

`Target={format:1,nodes:[Node]}`，nodes 為非空有序陣列，輸出依序串接；不支援任意運算式、正規式、HTML 或執行程式。
Literal 的內容就是文字，不再解析 `{{slot}}`；既有譯文切換時轉成節點，不保留兩種模板解析器。

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

CardNP 的 owner=Player、zone=ZoneSet、token=TokenStatus；class 為 Concept 的 vocabulary(class) 引用，
traits 每項為 role=trait 的 Concept glossary 引用；class 的 role=class_filter。
引用域也可使用已登錄的 `concept.trait.v1`／`vocabulary.class.v1`。
目前 traits 限 0 或 1 項，不把 token／class 當 trait；多項篩選須先確認具名構造的 AND／OR 語義。

未覆蓋的長修飾保留在 target 的 Literal＋LeafRef／Form 中，**不建立 opaque NP 字串槽**。
NP 不另占來源 span；它的葉各自有來源位置。NP 移除或新增只改 render projection，不改 frame 或 DSL 值。
若語義本身未解，projection 保留 pending，不能用沒有 NP 推論沒有 filter 或沒有動作。

| FormDefinition 欄位 | 型別／合法域與引用 | 驗證者；失敗／案例 |
| --- | --- | --- |
| id | Code；具名形式，如 zone.locative、zone.allative、zone.ablative、zone.add_to_hand、zone.return_to_hand、keyword.display、quantity.classifier | A/B/C；未知形式拒絕；FL-012 |
| lang | Lang；和 target 相同 | A/B/C；錯語言；FL-012 |
| signature | 非空 `{name:Code,type:TypeName,role:Code}` 陣列，name 唯一，type 引用 §4 葉型別，role 引用其角色 | A/B；參數不合；FL-004／FL-012 |
| rule | Code；引用釘版 renderer 的有限規則，不接受程式 | A/B/C；未知或超域；FL-012 |
| cases | 非空 `{case_code:[FormPart]}` mapping；case_code 限 rule 登錄的分支，FormPart 是 `{kind:Literal,text:Text}`／`{kind:Label,arg:Code}`／`{kind:QuantityValue,arg:Code}`，每分支有序 | A/B；分支缺失／未知、arg 不引用 signature 拒絕；基礎名稱來自當前選詞；FL-012／FL-015 |

QuantityValue 的 arg 必指本形式簽章中的 QuantitySpec，僅在 `quantity.constant_value.v1` 使用，輸入 expr 必為 constant。
它只輸出 UInt 的十進位字串，不輸出 mode、單位或概念位置；all／any、bound／expression 均不適用。
N0 的 `quantity.<role>.value` form 只有對應 role 的單一 QuantitySpec 參數、default 分支及一個 QuantityValue。
arg 沿同一來源葉驗證，計入 required 引用閉包、render 依賴及來源／target occurrence；mode 仍由來源構造驗證。
機械轉換保留原 target 的固定數量修飾，只替換原數字 placeholder，不從中文反推 mode、單位、值或來源位置。

rule 只依 typed args 選具名 case；cases 必完整涵蓋該 rule 的簽章合法域，不能把選詞或語義值複製到規則內。
例如 zone.locative 的 hand／battlefield 分支各組合 Label 與「中」／「上」；quantity.classifier 依計數語境選中文量詞分支。
形式可產生「手牌」＋「中」、「到」＋「戰場」＋「上」，或「加入」＋「手牌」、「回到」＋「手牌」。
動詞融合形式只可在相符 frame 動作／目的地角色使用，不能把 add、return、draw、discard、destroy 都變成 move。
keyword.display 可供括號，基礎名稱不含括號；別名與全名綁同概念，來源拼寫留 trace。
形式內的 Label 產生概念範圍，Literal 後綴／括號不擴大該範圍。缺基礎選詞則整欄退回（FL-011），未知形式 ID 則結構拒絕。

CardNP 的區域詞尾使用同語言 `zone.card_np`（`zone.case.v1`），不由 renderer 在持有者與區域後一律補所有格。
其 cases 包含單一區域與 `mixed`；詞尾與多區域的合計修飾由該語言的 Literal 表達。
量詞使用既有 `quantity.classifier`，計數區域優先取 NP 自己的 zone，不能借用目的地區域。
沒有 NP zone 時，只有唯一的 counted_zone 能提供語境，否則整欄退回。
單一 trait 使用 `trait.filter`（`trait.filter.v1`），簽章恰為 trait:Concept/trait，default 必含 Label(trait)。
class 與 token 仍各自顯示已綁葉，不在 trait 形式內藏入名稱。

ZoneSet 的多成員按 canonical 排序顯示，每個基礎名稱有自己的概念範圍。
成員間使用同語言 `zone.union_separator`（`zone.union_separator.v1`）：簽章為 zone:ZoneSet、default 只含 Literal。
連接詞與詞尾都不屬於任何成員的概念範圍；缺少必要 NP 形式或連接詞時整欄退回。
這些是既有葉的呈現規則，沒有新增 N0 來源葉或放寬來源單位守衛。
固定案例見 [參數群組渲染案例](../../../carddb/tests/fixtures/parameter-groups/render-cases.json)。
案例的 `parameter_group_render` 只驗 renderer 的隔離 typed fixture；`parameter_group_n0` 另從合成日文驗正常來源分類。
N0 正文仍無可供 CardNP 使用的 counted_kind／ZoneSet／Player／TokenStatus 葉，因此不以 renderer 案例宣稱已完成真實來源的 NP 接入。

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

以下是[build-db §9.3](../build/build-db.md#93-四層資料契約)的邏輯列契約；N0 已接入正式 DDL 與建置邊界，實際儲存欄位見該文件 §9.1。
所有新 Json 欄均使用本文件具名型別，C 逐層驗證後才能交給其他層。
現有 authored_source_id／record_key／origin／low_confidence 與真實 source_id 的型別及 nullable 保留。

| 邏輯列／鍵 | 四層欄位、引用與差異 | 驗證者；失敗／案例 |
| --- | --- | --- |
| sentence_template；PK id | §3 Frame＋建置才有的 canonical_source:Text；source 描述拆欄或型別 Json；取代舊六欄 payload | B/C；hash／域錯配；FL-002／FL-004 |
| template_translation；PK template_id,lang,variant_key | target:Target 取代 text；其餘品質欄保留；template_id FK、variant_key=default 表一般值 | A/B/C；壞 target；FL-005 |
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
已知舊域的資料接口由 #499 補上；正式 reader 不因保留舊引用而恢復舊翻譯格式。

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

本次 #498 的正式建置直接讀現行 format 2 裁定檔，不要求 repo 外的一次性遷移輸入、
永久 authored allowlist 或 runtime Git 查詢。舊 `T` 加十位小寫十六進位 hash 引用
目前沒有已知舊域，直接按原 applies_to ordinal 產生引用層級 pending。
非模板 IR 引用原樣保留並獨立列帳；不符合支援形狀的引用拒絕建置。
裁定來源記錄保留本次檔案 path 與實際 bytes hash。

已知舊域的用途層級解析、完整用途集合與候選來源檢查由 #499 實作；
以上用途層級形狀是後續契約，#498 不產生用途層級 resolved 或 pending。

### 9.4 完整性與執行邊界

B 逐筆保留裁定原始 applies_to 清單與 ordinal，重複文字的引用仍各有一筆記錄。
已知域的完整用途集合對帳由 #499 承接。
對帳同時列出模板引用、非模板引用、引用層級 pending、用途層級 resolved／pending；
不把引用數和用途列數相加，不以「輸出 0 個裁定」宣稱驗收完成。
N0 的 resolution.build 依現行裁定的原始 ordinal 產生引用列；DB 檢查 SQL PK／FK 與逐列封閉型別。
裁定邏輯鍵唯一／互斥、完整用途集合及用途候選來源檢查由 #499 接上；本次不宣稱 DB 已驗證這些集合條件（FL-027）。

active 裁定邊只能由已驗的用途層級 resolved 產生，且其 scope 僅限該用途。
多個舊用途可 resolved 到同一新 frame；任何一筆都不代表適用新 frame 的所有卡。
active 懸空引用必為零；所有 pending 與其候選不得進可執行閉包。
這不自動授予 resolved 邊所依賴 DSL 的 reviewed／verified、freshness 或實跑資格。
裁定正文仍可查讀，pending 原因仍可見；無用途級資格時不出適用該卡的肯定提示。
有下游可執行資料要求 pending 引用時，拒絕該執行閉包，不把 pending 當 no-op。

本次 #498 同批完成裁定封套／revision 的機械初始化、reader／validator／DB 邊界及引用層級完整對帳，
用途層級對帳由 #499 承接。
不改裁定正文、證據、日期、applies_to 內容與順序或 supersedes。
已知舊域的用途解析、正式 authored 適用引用換鍵、補齊歷史語義證據、裁定內容與用途審查由 #499 承接；
換鍵或內容變更須明示續版，舊引用與版本歷史仍可追溯。
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
