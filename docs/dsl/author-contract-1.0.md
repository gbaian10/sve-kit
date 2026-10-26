# 效果 DSL 1.0：承載、版本與執行邊界

作者查詢、簡寫輸入與現行格式寫入指令見[效果作者工具](authoring-tools.md)。

第一版保留嚴格 reader、精確展開、來源綁定及 expanded JSON 與 typed IR 的交接。§4–5 的資格、用途依賴追蹤、rekey、邊界案例及實跑 DB 屬延伸契約，第一版不執行；review 證據只驗封閉形狀，不授予准入。

本文是DSL 1.0 的文件契約，與[DSL 語法](author-syntax-1.0.md)、[IR 詞彙](ir-vocabulary-1.0.md)共同閱讀；實作範圍見 §1，不表示格式已發布或來源已准入。語法的唯一機器權威仍在 `dsl/`，不能把既有 astra/1 Schema 當作 DSL 1.0 驗證器。決定方向見 [#799 的維護者決定](https://github.com/gbaian10/sve-kit/issues/799#issuecomment-6282231068)。

## 1. 版本承諾與入口

封套、共用本體承載、內容版本及用途依賴、公開 typed IR 與能力准入是單向門：改變時須重新審查契約與遷移影響。本文的資料欄位是具體可審的 1.0 文稿；尚未發布前可同步修訂。內部資料表、函式、快取、診斷代碼拼字、支援登錄的分檔方式及 adapter 實作是雙向門，可在 1.x 文件修訂，不因此放寬既有語義或來源責任。發布後沿DSL 語法的主／次版規則，不用文件小版掩蓋不相容變更。

| format／kind | DSL 支援集 | Schema `$id` | 資源（實作範圍見下文） |
| --- | --- | --- | --- |
| `1 / author_effect_set` | `1.0` | `urn:sve-kit:author:1.0` | `dsl/author.schema.json` |
| `1 / effect_body_shard` | `1.0` | `urn:sve-kit:effect-bodies:1.0` | `dsl/effect-bodies.schema.json` |
| `1 / effects_index` | 定位資訊，不選語義 | `urn:sve-kit:effects-index:1` | `dsl/effects-index.schema.json` |

目前三個入口都有封閉 Schema 與 Python 結構 reader；共用 `author-nodes.schema.json` 提供完整能力圖、`A.file`、`A.card` 與 `$defs/authored`。結構 reader 不授予來源或資格證明。明示 API 已提供檔案／索引雙向閉包、generated_by／具型別葉展開、原 JSON body hash 及 `expanded` profile；current 入口核對 JP current、registry Frame／Binding 與 effect-source-v1，能力映射限五類 entry 的完整 payload 與空 ports。輸出為 JSON 樹及分離 sidecar；current_source_checked 仍不代表用途依賴、review／exam 資格或可執行。完整用途依賴、rekey、資格與正式執行准入仍待完成。能力持有面由 SourceUse.owner 的 face 決定，卡級／面級容器只表示位置，多面卡的卡級能力不表示全卡常駐。完整容器形狀與檢查邊界見[入口形狀矩陣](author-entry-shapes-1.0.md)，可用 API 見 [dsl/README](../../dsl/README.md)。

兩個DSL 資源共用 `urn:sve-kit:author-nodes:1.0`（`dsl/author-nodes.schema.json`）的 A.* 定義。Schema 採 JSON Schema 2020-12。所有固定物件封閉；動態 map 只接受各自卡號／識別字規則，不以任意額外欄位吸收未知構造。Python 型別與 `$defs` 必須逐一對應並驗漂移；Rust 依同一資源的展開 profile 解碼並做漂移測試，不能另立語法。

`format/kind` 選入口，卡片 `meta.dsl` 與本體 `dsl` 必須在該資源的支援集中；沒有額外頂層 `schema` 選擇器。每個 loader 發行版將 `(format,kind)` 對應到恰好一個現行 Schema 資源；逐卡 meta.dsl 只檢查是否在其支援集中，不另選 Schema。相容次版以新 `$id` 及指紋取代該映射，支援集保留先前相容版本；同檔可含支援集內不同版本，Schema 依 dsl 窄化適用構造。舊 `$id` 只供已釘輸入重現，不原地替換資源內容。沒有登錄的版本拒絕，不選「最新可用」Schema。封套變更另升 format。既有 `effect_set`／`keyword_registry` format 1 與 `urn:sve-kit:effects:astra:1` 維持原型契約。

## 2. 卡片、本體與用途

### 2.1 封套與分片

逐卡檔恰為 `{format:1,kind:author_effect_set,scope,cards}`，`cards` 沿 A.card；`scope` 必填 `shared | en_override`，產生器預設 shared，只有已確認英文例外才選 en_override。四層契約目前只定 JP Frame／SourceBinding；本版對 en_override 在來源准入明確報 `en_source_contract_unavailable`，不產可執行樹，不借 JP 來源。此值保留給另行定義的 EN DSL 來源契約，不代表已有 EN 支援。本體檔的 scope 本版只准 shared，恰為 `{format:1,kind:effect_body_shard,scope,records}`。路徑分別為 `authored/rules/author-effects/<owner>/<sequence>.yaml` 與 `authored/rules/effect-bodies/<owner>/<sequence>.yaml`。

分片大小、排序與只增序號沿 [authored 格式 §1](../schema/domains/authored-layout.md#1-路徑與共同格式)。跨包本體首次以當次已知用途最小 `(region,card_no,variant_key,printing_id)` 所屬 owner 歸檔，新增用途不搬檔；owner／檔名不是本體身分。同一採納輸入集合的 `(frame_id,ability_scope,body_version)` 只准一筆，連完全相同的重複也拒絕；同內容可有多個用途與多個適用域登錄。

### 2.2 companion record

每筆 record 恰含以下欄位；未註選用的欄位必填。`Code` 沿四層的小寫代碼規則；本文件新增的 `ParamName` 為 `^[A-Za-z][A-Za-z0-9_]*$`，不是來源 Code。`Digest` 是 `sha256:` 加 64 個小寫十六進位字元；四層原有 Hash／interface_key／source_hash 保留無前綴 64 hex，不混用。檔案 scope（地區選擇）、ability_scope（來源能力作用域）及 applicability_scope（本體適用域登錄 ID）是不同欄位；分片 owner 是歸檔代號，來源 owner 是 SourceDescriptor 身分。

| 欄位 | 型別與責任 |
| --- | --- |
| `frame_id` | 四層契約完整 FrameId；須為 resolved ability_body |
| `ability_scope` | 該 frame 的 Projection.scopes 中 kind=ability、parent=null 的精確 id；一筆本體只對應一個能力 scope |
| `body_version` | §4 的帶 recipe 內容版本 |
| `dsl` | 精確 DSL 版本；本版為 `"1.0"` |
| `params` | `Map<ParamName,ParamDecl>`，可空；§3 |
| `body` | 一個 A.companion-head 的能力頭及 payload；沿用 A.ability-head 的本體子圖並排除 keywords，不含 line、section、sources、generated_by，位置由用途提供；可含 §3 的封閉 param 節點 |
| `ports` | `{imports:Map<Code,LocalBinding>,exports:Map<Code,LocalBinding>}`；Projection port 名到本體 binding 的接線，空 map 仍必填 |
| `written_by` | 非空 `{actor:Actor,date:YYYY-MM-DD}` 陣列；與卡片 meta.written_by 同型，Actor 見 §5 |
| `registrations` | `{id:Code,domain:Domain,reviews:[Review]}` 陣列，id 唯一；未登錄可空，不取得 qualified |
| `note` | 選用文字，不授予語義或資格 |

Domain 恰為 `{regions}`：非空、排序唯一的 `jp/en` 集合。本版 shared 的來源驗證地區集合恰為 `{jp}`；不因 shared 可用於對齊的 EN 版次而捏造 EN 來源已驗。用途在域內恰為「同 frame／ability_scope／resolved 語義變體，且來源驗證地區集合是 domain.regions 的子集」；EN 仍受 §2.1 的來源准入限制。這個初版域不表示局面 predicate：需要更細邊界時須新增封閉可檢查構造，不得用 note 收窄後宣稱已驗。本體可不登錄而存放；要引用至少有一筆 registration，reviews 可空、頻次不足仍可引用，但逐用途須獨立審核。域內容改動使域指紋及其證據失效；id 不是版本。域指紋為 canonical-json-v1 的 `{recipe:"effect-domain-v1",domain}` 完整 Digest。

LocalBinding 恰為 `{path,bind}`：path 是由該能力頭起算、只含物件鍵字串及非負陣列索引的結構路徑，bind 是 DSL binding 名。imports 的 path 固定空陣列，明示在能力根作用域宣告該 local 名，型別及來源角色取自對應 Port；exports 的 path 指向實際宣告 binding 的構造。路徑只定位已存在DSL 節點，不能執行任意查詢。兩端都須存在且型別、source_role、支配與 scope 相符；branch local 不因 export 越域。ports 入本體語義 hash，不靠名字相同猜接線。

### 2.3 逐用途選版與 inline

A.ability 沿用 `line`（1 起算）、選用 `section`（0 起算），正式完整輸入另有必填 `sources`。普通能力與 generated_by 使用恰一筆的 SourceUse `{binding_id,occurrence,ability_scope}`；獨立 keywords 宣告只使用下述 CardKeywordSourceUse。兩者精確沿四層契約的 SourceBindingId／OccurrenceKey，參與片段完整列在該 OccurrenceKey.segments，不另拆來源記錄。這是位置證據，不是另創來源身分；同一行有多個能力時，必由真實 projection scope 區分，不能只靠 line 配對。

能力內容二選一：既有單一能力頭（inline），或 `generated_by`。後者恰為 `{macro:{frame_id,ability_scope,applicability_scope},version,params}`；version 是 body_version，params 是 §3 的具型別實值 map。`applicability_scope` 指向該 record 的 registration.id。generated_by 記在產生的能力位置，不用一筆卡片 meta 假裝整張卡只依賴一個巨集；卡片 meta 保留逐卡彙總證據。同一能力同時有能力頭和 generated_by、未指定版本、找不到本體／域、scope 不符，一律拒絕。

inline 能力另有必填 `ports`，形狀與 companion 相同，無接線時兩個 map 為空；generated_by 能力從本體取得 ports，不另覆寫。每個能力另有選用 `imports`：`Map<Code,{binding_id,ability_scope,port}>`；鍵為其 Projection import 名，值精確選定本用途已驗且可見的上游 export。inline 同樣須驗 ports／scope；沒有 import 時省略。同一 projection ability scope 的多片段只組成一次能力；同位置多能力分別保留，不能按 source 數量複製執行。token 能力必掛 token 的 A.card／face，不能借母卡 owner。sources 的 owner、face、field、section、行與能力容器必須一致；classification 零起算 line 到 A.ability.line 只轉換一次。

`A.card.faces` 為 `Map<FaceId,{abilities}>`，FaceId 是永久 `f:` 加 32 lowercase hex；面容器不再嵌 meta。卡級 abilities／faces 與面級 abilities 可空，不將空與省略改寫成同一份DSL bytes。選填的能力 imports 若出現必非空。inline 沒有參數宣告：共用節點 Schema／TypeAdapter 仍接受合法葉位的 ParamRef，完整結構 reader 另沿已解碼型別圖拒絕 inline 內的 ParamRef（`inline_param_reference`），不以 raw dict 同名鍵判斷。

用途總是明示 body 版本及適用域，沒有依檔序、最高版本或「先找到」選版。跨域皆符合也不自動挑一個；inline 不隱式套用候選 body。本版一個能力的 sources 僅准相同 `(frame_id,ability_scope)` 且同來源能力實例；同 binding 的多段 segments 已在 OccurrenceKey 中保留，不拆成重複執行。不同 frame 合成一個能力尚無封閉角色接線，保持 pending，不以同名 scope 猜關係。由 imports 連來的其他 frame 則是先前能力輸出，不當作本能力片段；依賴追蹤仍涵蓋這些上游接口。`evolve_feed_entries.v1` 的 ability_0／ability_1 各產一筆有明確 scope 的 A.ability；缺 scope、重複同能力或漏掉需輸出的 sibling scope 均拒絕。

ability_flag 只填對應宣告，card_field 只填已登錄的卡片宣告容器：一般欄位如 A.card.deck_rule，或 abilities 容器內的 A.ability.keywords。容器位置不將 card_field 改成 ability_body。none 不產指令，pending 保留原因並拒絕相依自動結算；不能用空 do 冒充分類完成。卡片層宣告的來源以卡片 meta.source 覆蓋，並在建置依賴保留其精確 binding。

#### 獨立 keywords 的非本體來源

CardKeywordSourceUse 恰為 `{binding_id,occurrence}`，不含 ability_scope。A.ability.keywords 的 sources 在 authored 與 expanded 都**只**接受恰一筆此型別；不是與舊 SourceUse 的 anyOf，其他頭也不能改用無 scope 來源。這是外部承載形狀的替換，不是新增四層 scope；不改 card_keywords.v2 的 card_field、空 Projection.scopes 或永久 frame／binding／原文 hash。

keywords 為非空、保序、元素不重複的清單，ports 必為 `{imports:{},exports:{}}`，imports 與 generated_by 必須省略。一筆精確來源 binding 對應一個完整宣告，不得把同一行的多個 keyword 拆成多次使用同 binding。來源清單與宣告逐元素全等；漏、換、多出、換序、重複、未知或未支援 keyword、錯 owner／面／定位及漏 sibling 都應整卡拒絕。不同面各印相同 keyword 是獨立來源義務，不能跨面合併或當成全卡常駐。token owner 尚未閉合時仍拒絕，純 reminder／layout 不新增宣告。

companion.body 的 A.companion-head 子圖排除 keywords，未引用本體也必須驗；generated_by 不得產生 keyword 宣告。grant.keywords 與 activate.quick 是原有本體內的不同語法，不適用此 sources 型別。完整 reader 仍拒絕 inline ParamRef；本片不開非本體 companion 或新的葉參數位置。

正式 current reader 已從完整 current 來源獨立重建 ward／aura 義務，逐筆對帳作者宣告並產生獨立 declaration sidecar；metadata 與 printed_keywords 使用封閉的非本體 inventory 型別。本體與宣告在 D0 分別精確消耗，tree 與 sidecar 同時漏項仍由獨立 inventory 拒絕，普通本體義務不能冒充 keyword。純結構展開或外供 context 不授 current proof；D0 的來源對帳成功也不代表已降低、可執行或取得用途資格。Ward／Aura 的具名降低、精確整卡准入與封閉對局規則見下方〈守護與光紋的具名宣告准入〉；用途資格仍須獨立證據。

### 2.4 effects-index

`authored/rules/effects-index.yaml` 恰為 `{format:1,kind:effects_index,records}`。每筆恰為 `{card_id,scope,file,kind,record_key}`：card_id 是既有永久 CardId，scope 同上，file 是 authored 根相對 POSIX 路徑（禁止絕對路徑、`.`、`..`），kind 是 `effect_set | author_effect_set`；原型 effect_set 項的 scope 固定 shared，因其封套沒有 scope。record_key 是檔內 cards 的原樣 CardNumber。先驗 index 再讀候選；候選 YAML 失敗仍可診斷到 card_id。

同 `(card_id,scope)` 只准一筆；index 與被選檔案的全部 cards 記錄須雙向閉合，孤兒記錄或懸空 index 均拒絕。載入成功後核對卡號、永久身分、kind 與 scope。companion 不進逐卡索引，依 body key 從本體分片集合定位，重複鍵即拒絕。原型與新格式不能同時覆蓋同卡同 scope；shared 與 en_override 是地區選擇，不是兩段都執行。原型整批切換的操作由實作處理，不新增永久雙格式 runtime。

## 3. 封閉葉參數

`ParamDecl` 恰為 `{type,source:{slot,component}}`。slot 指同 frame 的 LeafSlot 名；component 僅 `value | constant | negated_magnitude`。value 取原子葉值；constant 只取 QuantitySpec.expr 或 QuantityExpr 的 constant.value，拒絕 bound／expression／ContextExpression；negated_magnitude 只把已證明負號的 Nat 幅度轉為負 Int，負號須在同 frame 的來源 canonical 構造及 adapter 規則明示。宣告不能附預設、程式或任意替換路徑。

以下是葉參數的**完整初版白名單**，未列的角色／位置不接受 param，仍可使用既定 inline 常量。路徑按 A.* 型別位置判，不按全域鍵名字串判；中間的列表代表逐元素，不准替換整個列表。每次轉換還須核對來源構造、operand、域與目標語義角色，不能跨 operand 借同數值。

| DSL type | 來源型別／role／component | DSL 葉位置 |
| --- | --- | --- |
| `Int` | Nat/arithmetic_offset/value | A.expr.minus 的第二個常量 operand；A.expr.plus 的偏移常量 operand；加減方向固定留 body；minus 只配來源 descriptor 的 subtract_offset，plus 只配 add_offset，不符即拒絕 |
| `Int` | Nat/cost_delta_magnitude/negated_magnitude | A.effect.cost_mod.delta，僅來源明示負向減費 |
| `Nat` | Nat/count/value | A.act.draw.n、A.act.discard.n、A.act.create.n |
| `Nat` | Nat/damage_amount/value；Nat/recovery_amount/value | 前者只配 A.act.damage.n；後者只配 A.act.life.delta 且來源明示增加 |
| `Nat` | QuantitySpec/selection_count/constant | A.target 的 n／upto、A.act.choose 的 n／upto、A.act.search 的 n／upto；必分別匹配來源 mode=exact／up_to |
| `Nat` | Nat/cost_value/value | A.effect.cost_mod.set，僅同一打出費用設定構造 |
| `CardName` | CardName/name_filter/value；CardName/created_name/value | 前者只配 Sel.name；後者只配 A.act.create.token 的單一名稱／名稱列表元素 |
| `Concept` | Concept/trait/value；Concept/keyword/value；Concept/class_filter/value | 依序只配 Sel.trait／Sel.traits_all 元素、A.ability.keywords／A.act.grant.keywords 元素、Sel.class |
| `CardKind` | CardKind/filter_kind/value | Sel.kind |
| `Player` | Player/counted_owner/value；Player/source_owner/value；Player/destination_owner/value | 依序只配 A.expr.count.who、Sel.who、A.act.draw.who；只准 self→self、opponent→opp |
| `Zone` | ZoneSet/counted_zone/value；ZoneSet/source_zone/value；ZoneSet/destination_zone/value | 依序只配 A.expr.count.zone、Sel.zone、A.act.move.to／A.act.search.to／A.act.create.to；只接受單元素集合並取該成員 |
| `Phase` | Phase/phase_trigger/value | A.event.phase.which |

來源沒有 Int 葉型別；有號算式由 DSL Expr 保留。CardName 解成已採納的日文規則名稱，Concept 按 category 解成已登錄的 DSL 字串，沿用 inline 的種族／職業名稱及關鍵字拼法，不限於來源 Code 的 ASCII 語法；來源引用及映射版本保留在依賴，不由中文譯文推值。Zone 的封閉對照為 battlefield→field、hand→hand、deck→deck、evolve_deck→evolve_deck、graveyard→cemetery、ex→ex；第七種來源 banish 在本版葉參數映射中拒絕，待作者區域代碼明確登錄，不用「同義」猜字串。任何前端已定的構造若不在此表，只是不能參數化，不刪除原有 DSL 語法。

封閉引用節點恰為 `{param: ParamName}`（A.param），實值恰為 `{type:<同宣告>,value:<整數或字串>}`。型別完全匹配，值由上表轉換，禁止缺參數、多參數、bool 冒整數、數字字串。不得取代 do、define 整體、selector 整體、能力頭、運算子、binding 名、scope、ports、result、capture 時點或期限結構。ObjectRef／ReceiptId／CapturedValue 經 ports 接線，不是葉參數。展開後不得殘留 param／generated_by。

BP08-038 的局部示意：offset 宣告為 `{type:Int,source:{slot:offset,component:value}}`，來源需為 Nat/arithmetic_offset；body 保留 `minus:[{count:{who:self,zone:deck}},{param:offset}]`，用途提供 `{offset:{type:Int,value:5}}`。這不是已採納 frame；所需 resolved frame 或來源槽不在時，不得偽造，仍須先完成來源分類／審查。反例：cost_delta_magnitude=2 用 component=value 直接放 delta 應拒絕，只有已證負號的 negated_magnitude 才產 -2；value 給 `{minus:[...]}` 也拒絕，不能輸入整棵 QuantityExpr。

## 4. 本體版本與新鮮度

### 4.1 body recipe

body_version 格式固定 `effect-body-v1:sha256:<64 lowercase hex>`。完整 SHA-256 的輸入是 canonical-json-v1 編碼的以下物件，無前後換行：

```json
{"ability_scope":"ability_0","body":{"spell":{"do":[]}},"dsl":"1.0","params":{},"ports":{"exports":{},"imports":{}},"recipe":"effect-body-v1"}
```

payload 恰含 `recipe,dsl,ability_scope,params,ports,body`。ability_scope 是本體的來源能力 scope；body 是參數展開前的DSL 節點，params 含完整宣告（含來源 slot／component），ports 含接線。alias／merge 先由嚴格 YAML loader 展開；不補語義預設、不重新命名 binding、不重排步驟、不做代數化簡。canonical-json-v1：UTF-8、物件鍵排序、陣列順序保留、無空白、無 Unicode 正規化；U+0000–U+001F 用小寫 `\u00xx`，引號／反斜線 JSON escape，其他 Unicode 原字輸出。只准 null／bool／字串／安全整數／陣列／物件，拒絕浮點、孤立 surrogate、非字串或重複鍵；安全整數範圍 ±9007199254740991。沿用既有 core canonical，不另造 JCS 變體。

frame、variant、域、作者、日期、note、行號與檔名不入 body hash；它們另參與用途與證據依賴。改縮排／鍵序不改版；改 minus、follow、ports 或宣告來源槽即改版。相同 hash 還須比較完整 payload bytes，碰撞拒絕。exact authored bytes 另作候選來源 hash，不拿它當語義版本。

上例為合成已知值（空 spell 只測編碼，不授予可執行資格），其 hash 見本節末；必須由兩端依這份固定輸入比較，不以各自輸出互作預期。

預期 body_version：`effect-body-v1:sha256:d4b82bd23c514f4ec3167e143885706c8428456bce2e037c1a4bbe6fb218c3d7`。上述單行即 canonical bytes，不含 code block 的換行。

### 4.2 來源與用途依賴

meta.source 使用 `effect-source-v1:sha256:<64 lowercase hex>`。payload 恰為 `{recipe:"effect-source-v1",normalizer:Code,region:"jp",faces,token_links}`；normalizer 是四層來源推導的精確版本。只有 shared 可使用此 recipe，en_override 依 §2.1 拒絕。

faces 是非空、按 face_id 排序且唯一的記錄，涵蓋本卡全部面及依 token_links 實際引用的 token 面之傳遞閉包；每筆恰為 `{face_id,revision_id,texts,attributes,rules_names}`，revision_id 為 JP 已採納 current，不能借另一面或候選觀測。texts 恰列 name／effect 及全部 section，元素為 `{field,ordinal,state,source_hash}`：ordinal 在 section 為零起算整數，其餘 null；依 name、effect、section 順序，section 依 ordinal。state=`present | absent | unknown`；present 的 source_hash 是 exact UTF-8 無前綴 SHA-256，absent 只用於已有確定無主文證據的 effect 並取空字串 hash，unknown 的 hash=null 且拒絕可執行輸入。原文不去空白、不 NFKC、不經翻譯。

attributes 恰含 `class_code,type_code,cost,attack,defense,traits,titles,special_kinds`。每欄恰為 `{state:"known",value}` 或 `{state:"unknown",value:null}`；任何 unknown 拒絕可執行輸入。known 的 class_code 為 Code 或規則確定不存在時的 null、type_code 為 Code，cost／attack／defense 為非負安全整數或規則確定無該數值時的 null；後三欄為排序唯一 Code 陣列。欄位來源是 face_revision 及 face_trait／face_title／face_special_kind，不假造 SourceDescriptor 文字欄位。rules_names 為排序唯一 `{rules_name_id,role}`，包含該 face 的全部有效 primary／collab／treated_as 規則名稱關係，沿既有規則名稱登錄的 role 與永久 ID；中文顯示名不入 hash。

token_links 是按 canonical bytes 排序且唯一的 `{source_owner,section,token_face_id}` 陣列。source_owner 是實際引用 token 定義的 SourceDescriptor owner，section 是其補充段落 ordinal（本文引用時 null）；token_face_id 是已驗選用結果；token 定義及規則值已由 faces 的完整閉包釘住。輸入僅由既有 token 選用及來源核對產生；未知或歧義選用拒絕，不能由名稱猜 token 身分。token_links 改變即改本卡 source；無 token 依賴時為空。

這是保守的來源新鮮度 recipe，不取代 binding、接口或規則等義審查。文字與結構值分開，absent 與未知分開；exact authored bytes／來源批次 provenance 另存。固定向量見 §4.4。

建置每個用途保存下列依賴快照（邏輯 shape，非指定 SQL 表）：

| 欄位 | 值 |
| --- | --- |
| `sources` | 完整有序 `{binding_id,occurrence,ability_scope}`；含當前來源版本及 owner 定位 |
| `frame_id,ability_scope,semantic_variant` | 主來源的精確值；只接受 resolved body；其他上游接口在 interfaces |
| `body_version` | 引用版；inline 用同 recipe，params 為空，ability_scope 取來源，ports 取 inline 明示接線 |
| `interfaces` | 排序唯一 `{frame_id,interface_key}` 陣列；涵蓋 sources 及 imports 的所有直接／傳遞上游 frame，interface_key 沿四層無前綴 Hash |
| `applicability_scope,domain_hash` | 引用域及其指紋；inline 兩欄為 null |
| `source` | 上述 effect-source-v1 值 |
| `rulings` | 排序唯一 `{id,revision}`，採用已驗且適用的當前裁定；沒有則空 |
| `expansion_recipe,adapter_recipe` | 精確且不可變的 C0 配方識別及資源 Digest |
| `params` | 此用途完整 typed 實值；inline 為空 |
| `imports` | 按 port 名排序的 `{binding_id,ability_scope,port,dependency_key:Digest}` map；dependency_key 是上游完整 effect-use-v1 依賴鍵，不嵌遞迴快照。無則空 map，上游依賴圖須無環 |

interfaces、rulings 及 BodyDependencies.sources 以完整 canonical-json-v1 bytes 排序且唯一；Domain.regions 以字串排序且唯一。UseDependencies.sources、written_by、reviewed_by、registrations、registration.reviews、meta.rulings 與能力陣列保留原序，不由 reader 重排。

每個配方 pin 恰為 `{id:Code,hash:Digest}`。依賴鍵是 canonical-json-v1 `{recipe:"effect-use-v1",dependencies:<上表物件>}` 的 Digest。不同用途不得借同卡號的證據；C0 內部來源位置 sidecar 以DSL JSON 路徑對應此依賴，不塞私有欄位進 A.* 樹。

有效資格在建置時比對當前依賴；任何一項不符都失效，列出 changed dependency、舊值／新值及用途位置。verified 另釘 D0 lowering recipe、完整 program hash、engine build、policy、測試輸入版本。裁定／adapter 改版即使 body hash 未變仍失效；只改中文譯名字串不影響這些規則輸入。反向用途索引可重建，不存永久逐次收據或歷次接口帳本。

### 4.3 normalizer 重鍵

loader 只讀當前精確引用，不自動重映射。normalizer 變更時，按四層 §9 逐用途核對完整 frame-v2 payload、occurrences／values／scope／接口語義及目標唯一性。只換版本不改 frame／binding／use ID；來源基準及證據鍵仍須更新。語義改變則由 writer 更新受影響閉包的引用。其餘變更正常失效並重新審核。

機械延續只限實際存在且先前有效的證據，不冒充審核者再看過新版。Review／body_boundary 可帶選用 `rekey`：恰為 `{original:{input_hash,record_key},before,after,mapping_recipe}`。original 釘既有 authored 證據或前次已驗 DB 輸入的 exact Digest／記錄鍵；before／after 是完整相同型別的依賴快照，mapping_recipe 為 `{id,hash}`。原始 Review 的日期／審核者／verdict／dependencies 不改，工具以 rekey.after 計算目前適用性；只准依上段證明改 normalizer 與其衍生識別，body／params／語義／案例／runner／裁定／展開配方皆不可改。寫入驗證要取得原證據及舊來源 payload 驗證，不可只相信映射自稱相同。

延續鏈在寫入時壓成同一筆原證據到當前用途的完整證明；不是永久逐次帳本。普通建置只比 after 與當前依賴，不重新核准歷史。沒有原證據、非唯一映射或缺舊 payload 就不延續；上游更新須由寫入操作同步更新 meta。這保留四層契約既有的有限延續，不把身分續接當成新審核。

### 4.4 來源固定向量

以下合成資料僅測 exact 編碼／hash，不表示已存在的官方卡或已驗來源；空字串 hash 示範確定無主文，known null 與 unknown 仍為不同資料。

```json
{"faces":[{"attributes":{"attack":{"state":"known","value":1},"class_code":{"state":"known","value":null},"cost":{"state":"known","value":1},"defense":{"state":"known","value":1},"special_kinds":{"state":"known","value":[]},"titles":{"state":"known","value":[]},"traits":{"state":"known","value":[]},"type_code":{"state":"known","value":"follower"}},"face_id":"face:fixture","revision_id":"revision:fixture","rules_names":[],"texts":[{"field":"name","ordinal":null,"source_hash":"2ab25f10948b26d75da5b1623465c24e8ad39e11a3295c95c810e99dfa56e462","state":"present"},{"field":"effect","ordinal":null,"source_hash":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","state":"absent"}]}],"normalizer":"fixture-v1","recipe":"effect-source-v1","region":"jp","token_links":[]}
```

不含換行的預期值：`effect-source-v1:sha256:6d089f3235bb580294945c5c0b5b4afb9d6ce6b2d622ac6b0081476348bbebe6`。

## 5. 審核、測試證據與 meta

人寫審核放 authored，版本化案例放 `tests/`（各子專案既有測試根），機械執行的已驗結果與計算資格存建置 DB。案例存在不等於 pass；結果不提交 repo，不以 CI log 作唯一來源。

已驗 DB 是由 validate 或資料變更驗證入口產生、含精確依賴及真實執行結果的完整候選。普通建置由 caller 明示結果的選用位置，讀取該位置由驗證入口最近一次原子更新選定的 DB；不是任意歷史 DB，不准手動退回舊 pass。普通建置唯讀載入其中的當前結果，在新建 DB 中重算依賴後保留仍有效的結果，並記錄所消費 DB 的 exact bytes Digest；普通建置產物不改選用位置，也不能成為選用結果輸入。寫入／validate 以選用位置的輸入為基礎執行受影響案例，產生完整的新 DB 候選，連同當次 fail 原子替換該選用結果，之後才由建置消費。未受影響的結果可沿用精確依賴；失敗不能留下舊 pass 當「目前結果」。初次、結果遺失或無可驗 DB 時，先獨立 validate 建立結果；普通建置不代跑，缺證據即不 qualified，依下述範圍拒絕。CI 不保留、上傳或快取結果 DB；資料變更 CI 以無結果輸入重新執行當次採納資料所需的全部案例，不沿用前次結果。DB 儲存／選用指標的實作是雙向門，須明示輸入與成功／失敗更新，不新增永久收據帳本，也不原地改已發布快照。

Actor 恰為 `{who:Text,model:Text|null}`；模型的 model 必填標準產品與精確版本，真人為 null。所有 written_by 使用非空 `{actor:Actor,date}` 陣列，reviewer 使用同一 Actor；model 用 AGENTS.md trailer 的「產品名 版本」標準形式，例如 `Claude Opus 5.5`、`Codex gpt-6-astra`；不接受 API ID（如 claude-opus-5-5）、縮寫或別名。寫入端核對實際模型並填標準名稱，不靠寫入者自由改拼法。排除自審與模型去重以此精確字串比對，不按 who 的自由顯示字串。

BodyDependencies 恰為 `{frame_id,ability_scope,body_version,applicability_scope,domain_hash,interfaces,sources,rulings,expansion_recipe,adapter_recipe}`。interfaces 沿 §4.2，sources 為排序唯一的已審 `{binding_id,occurrence,ability_scope,source}` 集合；新增來源不自動使同版 body 審核失效，但新用途仍驗三項檢查；已審來源變動則失效。

Review 是封閉和型別：`{kind:"body",reviewer,date,verdict,reason,dependencies:BodyDependencies,rekey?}` 或 `{kind:"use",reviewer,date,verdict,reason,dependencies:<§4.2完整物件>,rekey?}`。verdict 為 `ok | revise | disputed`，reason 對 disputed 必為非空文字，其餘可 null。body review 只存 registration.reviews；use review 存卡片 meta.reviewed_by，須是逐用途獨立語義審核。保留完整依賴快照才可報舊／新值，不只存一個 hash。

meta.reviewed_by 的 use review 可保存同一用途的多個不同獨立審核者；另准一種機械項：`{kind:"macro_mechanical",body:{frame_id,ability_scope,body_version,applicability_scope},use_dependencies:<§4.2完整物件>}`。它不列名真人／模型，建置反查 body 的真實 reviews 及本用途機械三檢查；不能冒充這些模型親審該卡。每個用途只准獨立 use review 或 macro_mechanical 一類資格條目，兩類並存即拒絕，不由建置挑較有利者；未完成可為空。

同一審核者、同一依賴組只准一筆當前 verdict；改 verdict 的寫入保留 Git 歷史，相矛盾重複拒絕。同一 body／用途的任何現行 revise 或 disputed 都阻擋其取得 ok 類資格，不以其他 ok 的票數抵銷。body 有 disputed 時該 registration 不得 qualified，引用它的 macro_mechanical 用途保留 disputed 並退回獨立審核；切換條目類型也須明示解決原爭議，不能藉切換消除。寫這份 body 的人所用模型都不計入「兩個不同非 DSL 模型」的數量；真人 review 不冒充第二模型。disputed 是人工未解爭議：來源改版不自行清除，寫入工具須沿精確用途保留該爭議，只有明示解決 reason 且經獨立審核的寫入才可更新 verdict；不能以過期不計數解除。

邊界測試輸入恰為 `{kind:"body_boundary",dependencies:BodyDependencies,case_set:{id,hash},runner:{id,hash},results,rekey?}`。case_set／runner 是 Code＋Digest，results 為非空 `{case_id,boundary,result}` 陣列；case_id 唯一，boundary 僅 `zero_cards | no_legal_targets | large_values | timing | information`，result 僅 `pass | fail`。case_set hash 覆蓋全部案例內容與版本，runner hash 覆蓋實際驗證配方；只有真實執行入口可以寫入，缺任一邊界、未知案例、依賴不符、fail 或無當前結果皆不 qualified。

指定 DB 中同一依賴／case_set／runner 只准一組當前結果；執行完整組原子替換前組，fail 不與舊 pass 合併，不向歷史 DB 回找 pass。舊引擎實跑報告可留診斷歷史，不能覆蓋當前結果選擇；body_boundary 與逐用途 engine exam 是不同類型，不把前者當 verified。

頻次、兩模型、五類邊界、登錄與逐用途三檢查仍依[DSL §10](author-syntax-1.0.md#10-巨集規範)及 [MQ](macro-qualification-cases.md)。只有實際展開 DSL 產生的反譯才算檢查，回放原文不算。未取得 qualified 可走逐用途獨立審核。

寫入工具依真實證據產生 meta.status／reviewed_by，建置逐項重算是否仍有效，過期／不一致即報診斷。普通建置只拒絕該卡當前 scope 的自動執行及程式出貨，保留文字與拒絕理由，不使其他卡建置失敗，也不回寫 authored；格式／引用閉包損壞仍依既有入口拒絕。資料變更 CI 必須因 meta 不一致失敗，上游來源、裁定或配方 PR 須同步更新相依 DSL 狀態，不能把缺證據留 main 卻宣稱 reviewed。

卡片 status 為全卡彙總：有未解爭議為 disputed；其餘任一用途 draft 則 draft，全部至少 reviewed 才 reviewed，全部有有效引擎實跑才 verified。非 body 宣告仍須來源／型別及獨立審核，不以無步驟自動 verified。主版遷移沿既有降級規則；draft 不提升，disputed 不消失。過去 review 與實跑的原始內容保留 Git／已指定 DB 證據，但不混進有效名單。

獨立 keywords 的非本體用途依賴／審核尚未實作，不能套用含 body_version／ability_scope 的本體 UseReview。第一版 reader 對含獨立 keyword 宣告的卡只接受 status:draft、reviewed_by:[]、verified_by_exam:[]；不滿足時精確拒絕，不回寫或清除爭議／歷史。此保守限制也排除該卡混入其他用途 review，避免將部分本體審核當整卡核可；後續具備逐用途非本體證據型別時再縮小限制。這次不新增持久 declaration-use recipe，不改 effect-source-v1／effect-body-v1／effect-use-v1 的 payload 定義；schema 指紋進入既有 adapter／expansion 配方所造成的相依失效仍須重驗。未來非空非本體 review 須另定封閉依賴、定位、hash 與失效契約，不能用假 body_version 或空 scope 代填。

目前結構 reader 對非空 `meta.verified_by_exam` 報 `unsupported_exam_evidence`，其封閉逐用途實跑格式須另行實作；空陣列不授予 verified 資格。reader 保留 review／rekey 的封閉形狀與原序；review 重複、相依關係、爭議及資格檢查不屬第一版 reader。body_boundary 是測試輸入與建置 DB 的實跑證據，不是這兩種 authored 容器的欄位。

## 6. C0 與 D0 的交接

C0 讀嚴格 YAML、驗 DSL 型別、展開共用本體與葉參數，交出通過共用 DSL Schema 的 `expanded` profile 的 JSON 樹；D0 讀這棵完全展開的 DSL 樹並降低 typed IR。sources／meta 為DSL 資源本有欄位；共用資源提供兩個封閉入口 `$defs/authored` 與 `$defs/expanded`：後者遞迴排除 A.param／generated_by，展開產物一律用後者驗，Rust 型別亦對它測漂移。C0 額外的逐節點位置與依賴 sidecar 是內部介面，和樹分開，不是新公開 wire 格式。C0 的等值驗收比較去除 line／section／sources 與審核 metadata 後的 canonical 能力樹（ports 保留）；inline 與 companion 的同義能力必須逐 byte 相同，IR 等值與執行則是 D0 的責任。

目前 `CardAccepted.lowering_input()` 另交接逐能力的 `current_source_checked`、已檢查的完整 SourceUse、DSL 輸入與來源文字 hash。D0 對帳 sidecar 與 expanded 能力路徑的精確集合、SourceUse 與行／section；缺漏、重複、非 current proof 或來源不符均整卡拒絕。`CurrentCardProgram` 分別保留 expanded 樹、來源 sidecar 與 IR；`source_at(author_path)` 沿能力祖先路徑回查永久 CardId／FaceId／revision、原 field／section／line／fragment 及 occurrence segments，IR instruction 沿既有 SourceSpan.author_path 使用同一對應。來源證明不等於用途資格或執行准入；原型 base／ordinal 入口不變。

current 入口另從全部 current 面與 sections 的 registry 來源建立獨立 `source_inventory`，
不從DSL 樹或 sidecar 回推必需能力。每個 `(binding_id, ability_scope)` 保存完整 SourceUse
與去掉 line／section／sources 的能力本文（含 ports、費用、全部分支）。D0 逐一消耗這個集合；
即使 expanded 與 sidecar 同時漏掉 sibling 或 section 能力，仍整卡拒絕；多出、重複或改變
未選模式本文也拒絕。清單沿用既有來源 hash，不新增 hash 配方或公開格式。
非 body 的來源宣告另存在 `non_body_sources`；卡面屬性與普通費用仍在 source_payload，
不當成缺能力，也不因沒有本文而聲稱已執行。token 來源與非空 token 選用目前仍整卡拒絕，
不把 token 能力複製到母卡。來源 adapter 未支援的能力圖也維持拒絕，清單對帳不擴張來源支援範圍。

獨立 keywords 的來源沒有 ability_scope；current handoff 已使用封閉的宣告義務與獨立 declaration sidecar，將本體與宣告分別精確消耗。不能把它塞入能力 scope 對帳或只保留任意 Value 就聲稱已降低。純 expanded 與 current 宣告的 SourceSpan.scope 使用 `keyword_declaration:<author_path>` 作編譯期局部座標，與普通 SourceUse.ability_scope 分開；它不屬 Projection.scopes、不入 FrameId／BindingId 或來源 hash，不能拿來選 companion／port。ScopeId／AbilityId 仍是引擎分配的執行期識別。宣告的已驗來源可由獨立 sidecar 回查，保留原 occurrence 與 binding、owner／face／field／section／line；Ward／Aura 的對局消費者只讀這些已准入定義，不以 enum 名稱或原型 keyword 陣列授權。每個元素的 SourceSpan.author_path 保留 `/keywords/<index>`，共用原宣告的編譯期 scope 與無來源 scope 的 reference；各元素另分配 runtime ScopeId／AbilityId。對局執行不授予用途資格。

| 接縫 | 必須保留／拒絕的責任 |
| --- | --- |
| Int／Nat | 值域與靜態型別分開，bool 不作 Int；BP08-038 的 Live 有號減法不先轉 Nat，負數動作依規則處理 |
| Live／capture | define 保留算式別名，執行動作開始才讀；只有明示 capture 保存指定時點值，展開不常數化或移動 capture |
| ReceiptId／ObjectRef | result 只有 receipt；follow 依 receipt 型別取真正 moved 的新世代；found／chosen 不當 moved，舊世代不追同卡號 |
| scope／ports | 能力、分支、序列及 execution 邊界不穿透；前向引用、異型別分支合流、未登錄 port／source_role 拒絕 |
| source | 卡、面、token owner、欄、section、行、fragment 與 A.* 路徑可回溯；公開來源只用可發布定位，不帶私人路徑或隱藏物件 ID |
| 失效輸入 | 未知 DSL／recipe／Schema 指紋、stale interface／來源／裁定不能以 fallback 載入；報精確依賴差異 |
| 缺能力 | 列出所有缺失代碼及引發它的 IR／DSL 位置；整張卡拒絕自動執行，仍可顯示文字與診斷 |

CP04-060 只對 search 實際 moved 的新世代安裝 delta -2、until:turn；不替 found 但未移動者減費，不把 delta 改 set 0。替代移動是否算某種 receipt 的成功由精確規則定義，不能靠模糊成功布林或這份承載契約補裁定。

### 守護與光紋的具名宣告准入

`sim/engine/src/ir/printed_keywords.rs` 定義兩個封閉的 Field／Ordinary／Static 形狀，來源恰為 `Keyword(Ward|Aura, Printed(face,line))`，holder=SourceHolder、controller=Controller，沒有 body、play、trigger 或 event captures。Ward 的權限在持有者橫置時禁止對手從者攻擊己方領袖及不屬於橫置 Ward 的己方從者；以有效能力視圖保留多個 Ward 的合法目標。Aura 的權限禁止對手選取持有者作為能力目標，不擴及己方選取或無目標的集合效果。兩者保留無時間到期、installation anchor 與對應規則定位。Ward 的可選姿態選擇屬規則程序，不降低為無條件橫置。

准入逐欄比較整個具名形狀，並透過獨立 declaration reference 核對原 owner／face／field／section／line／fragment、local scope 與確切 keyword index；不開放任意 KeywordId、Granted 或巢狀 origin。只有不同的 Ward／Aura 元素可共用同一印刷面與行，本體能力仍不能與它們混用座標。完整有序清單原子降低，未支援元素使整卡失敗。

執行期以 `card/printed.rs` 的有效印刷能力視圖為共同依據：已載入整卡、有效印刷面、Field 有效區域且未失去能力的精確宣告才可用。`card/keywords.rs` 將這個視圖提供給攻擊、選目標、姿態程序與投影；入場事件的能力資訊也讀同一視圖。`state.keywords`、原型能力及 enum 名稱不補足正式卡的宣告。owner 不決定敵我，必須使用當前持有者 controller 或已固定的能力 controller。離場後查詢新世代，既有待處理能力則保留其舊世代及離場前來源資訊，不從新區域補讀舊能力。

守護入場依 CR 12.8.2，先以預計入場資訊產生控制者的 `WardEntry` 決定，再直立入場，接受時才以規則原因橫置。決定 token、actor 與舊世代均由引擎綁定；問題提供當時區域及一次性位置區分同名卡，不暴露固定隱藏 ID。整批入場先完成各張決定，再共同入場。普通打出與已准入的效果入場共用此程序。結束階段沿既有 CR 7.4.3，在誘發能力完成後可選任意張直立守護橫置，再開對手 Quick；多個橫置守護均為合法攻擊目標，直立或失去能力的守護不限制攻擊。

光紋依 CR 12.15，只從打出時的目標候選排除對手控制的光紋，不排除己方光紋，不禁止攻擊。無目標的 Field Destroy 集合及已選定目標的結算集合不套這個過濾；目標可行性與正式目標決定使用相同候選。能力授予、ignore-ward、通用 suppress／換面指示與其他關鍵字形狀仍未准入；有效面及既有失去能力狀態不因此繞過檢查。

BP19 完整入場曲所需的 `kind:follower` 公開 Field 破壞集合沿用 `card/selection.rs` 的候選述詞與逐候選篩選器。私有觀察閘只對已驗的 Field Destroy 集合述詞消耗這份授權，其他節點位置、私有區域與未支援讀值仍拒絕；不以放寬 `read.kind` 等全域責任白名單取代。

## 7. 公開 typed IR 與引擎能力

公開 `{id,dsl_version,ast}` 的 ast 承載巨集完全展開、通過 typed lowering 的 IR 樹；讀者不下載 companion、不自行展開巨集或降低DSL 語法。IR 須保有型別、決定時點、capture、receipt 型別、世代／follow、scope 與可發布 source 定位。公開節點編碼、program ID recipe、IR Schema `$id`／資源指紋映射、最低 reader 版本須在啟用格式時一起定義；本文不虛構可發布的節點 wire 清冊。

**傳輸 4.0.0 的 dsl-programs.entries 維持空陣列。** 啟用非空 entries 必須獨立升傳輸格式版本（至少 minor，具體版號不預留），依[傳輸 §3.4](../schema/export/snapshot-transport.md#34-dsl-程式包與版本准入)更新 required_capabilities、reader 准入、Schema 與固定案例；未滿足不得發布。未知 DSL 或未展開巨集整包拒絕；不偷加程式封套欄位，也不發布永久私有過渡 wire。

引擎語義能力由 IR 節點及參數形狀確定性推導，與傳輸 required_capabilities 分開。內部支援登錄恰為 `{dsl,ir_schema:{id,hash},engine_build,derivation:{id,hash},supported}`；supported 為排序唯一 `{code,version}` 陣列，version 是正整數。推導結果恰為 `{derivation,required:[{code,version,locations}]}`，locations 是非空 IR 節點路徑及對應來源；同能力合併位置，不合併不同版本。精確 `(code,version)` 集合包含才通過，不以引擎版本大小推定支援，不以新能力版自動涵蓋舊版。

以下為本版釘定的語義責任種子（版本皆 1），不是引擎已實作清單，也不是所有 63 族能力的全集。其餘責任須由 D0 同步增加封閉推導規則及正反例；任何未分類 IR 變體先拒絕，不退到通用 op。

| code | 由 IR 形狀推導的責任 |
| --- | --- |
| `value.live` | Live 讀值；包含動作開始讀集合數量 |
| `value.signed-arithmetic` | Int 算術；包含減法與負結果 |
| `value.capture` | 指定時點保存值／集合 |
| `action.draw` | 完整 draw 及有號要求量的規則處理 |
| `action.search` | search 的選取、條件證明、搬移、收尾洗牌程序 |
| `receipt.move-after` | 以實際 moved 成員建立結果引用 |
| `reference.follow-generation` | follow 到 receipt 授權的新世代及失效處理 |
| `effect.cost-delta` | cost modifier 的 delta 形狀，不涵蓋 set |
| `duration.turn` | 本回合期限與到期失效 |

需求是整棵樹所有節點與形狀的聯集，並包含節點展開隱含的語義責任；例如 search 依其程序至少要求 action.search 與 receipt.move-after，follow 另加 reference.follow-generation，明示 capture 另加 value.capture。不能只檢查 draw/search 字樣；BP08-038 至少需要前三者中的 Live／有號算術及 action.draw，CP04-060 另需減費／期限等所有相依能力。登錄只是實作支援宣告，不能代替用途的 reviewed／verified 或官方規則正確性。

## 8. 驗證時點與拒絕診斷

| 邊界 | 責任 |
| --- | --- |
| 產生器、獨立 validate、相關資料變更 CI | 共用嚴格入口驗 Schema、來源全文、binding／ports／域與版本閉包；執行變更影響的真實案例並交付結果 |
| 普通建置 | 只讀已驗資料及明示選用位置的當前已驗 DB 結果；重算當前依賴、資格與 meta 比對，不符只拒絕該卡自動執行；不回寫 authored、不全量重驗歷史來源、不重跑邊界案例 |
| 匯出、Python 完整 reader、上傳前離線檢查 | 驗完整 AST Schema、型別、scope、內容 ID 與閉包 |
| 瀏覽器下載／快取 | 驗 bytes hash／長度、格式、最低 reader、傳輸能力；自有已驗 JSON 不重跑 Schema |
| 引擎執行入口 | 型別化解碼、DSL／IR／engine 版本與語義能力准入；失敗拒絕，不補缺值或忽略未知節點 |

來源版本／ID recipe、分類、companion、逐用途輸入、裁定、展開配方、案例或測試 runner 變更都須觸發受影響驗證。尚未實作的入口與 CI 接線不得宣稱已保證。開發用未驗DSL 輸入也先走寫入驗證，不把 runtime 當另一個全文驗證器。

診斷至少帶 stage、reason、card_id／card_no、scope、owner／face、來源欄／section／line／fragment、DSL 節點路徑，錯型別帶 expected／actual，stale 帶 dependency，缺能力帶 code／version。無可解析候選時用 index 定位並附安全 parser 類別；不把整段卡文、私人路徑或秘密印進 log。內部診斷欄位編碼可改，定位與整張拒絕的責任不可省。
