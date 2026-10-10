# 公開 annotation 與 JP 依據契約

本文件定義 [#496](https://github.com/gbaian10/sve-kit/issues/496) 的 **3.0.0 目標契約**。
它承接 [ADR-0021](../../adr/0021-four-layer-translation.md)、[四層共用契約 §8](../domains/four-layer-translation.md#8-建置-dbrender-projection-與依賴)
及 [#198](https://github.com/gbaian10/sve-kit/issues/198)，不修改 frame、render 或 annotation 的身分配方。
機器定義見 [Schema](public-annotation.schema.json)，固定正反例見[案例規格](public-annotation-cases.md)。
producer、Python／TS reader 與 read_api 已切換為 3.0.0，Web 依精確 owner 讀取標註與來源對照。
本次 #498 已完成 N0 的原生四層 authored workflow、全庫機械重鍵與 3.0.0 producer／reader／Web 接線；固定案例本身仍不是實作驗收證據。既有容量依維護者豁免交付；基本目錄設計與容量量法依 [size-budget](size-budget.md)，不代表分段載入或手機效能已驗收。

## 1. 版本與驗證邊界

`format_version=min_reader_version=3.0.0`。新增必要 annotation，改變 Translation／FieldTranslation／cr_clause
欄序並移除 basis 值，依[傳輸契約 §1.1](snapshot-transport.md#11-版本與相容判斷)升 major。
所有 manifest、config、container、programs、text_all、changes 與 index entry 宣告同步；
`index_format=2` 不變。空 annotation 或無繁中列的快照仍須宣告全部能力。

required_capabilities 恰為以下已排序且不重複的集合：

```json
[
  "column-partition-v1",
  "digital-same-name-links-v1",
  "fragment-container-v1",
  "image-entity-buckets-v1",
  "image-id-url-v1",
  "jp-source-translation-v1",
  "public-annotation-v1",
  "rules-name-on-demand-v1"
]
```

3.0 reader 只接受明示支援的 3.0.0；不把 2.0、未知 minor／major 或未支援能力降級成普通文字。
新版 producer 不產 `shared_jp`／`shared_jp_unchecked`，reader 亦拒絕這兩個舊值。
切換時一次替換 producer／reader／共用 golden，不保留永久雙格式路徑，不回寫歷史快照。
更新失敗維持已驗且相容的本機 active；不把不相容 previous 自動升版。

| 驗證者 | 責任與失敗時點 |
| --- | --- |
| P：producer | 投影前重驗固定來源、同卡同面、freshness、逐 occurrence 與選詞；投影後驗 Schema、全部公開閉包、hash／分片／容量；錯資料不完成匯出 |
| R：Python／TS reader | 各自驗 canonical bytes、版本、Schema、欄序、引用、text identity、座標與公開 owner 關係；不得依賴 producer 已驗而放行 |
| W：Web | 只使用 R 已驗的文字與 annotation；依 UI／卡面／模式選列，不因 basis 推導 aligned、counterpart 或 DSL 支援 |
| U：read_api／publish | `sve_carddb.export.read_api.load_export` 沿 Python reader 驗完整公開根；publish 只消費此邊界結果，拒絕未支援版本／能力與不完整文字閉包 |

reader 能驗公開資料中的一致性，不能重做未出貨的來源封存／人工對應／counterpart 決定。
那些真實性由 P 驗；不能為了讓 R 重演採納而新增證據層。P-only 反例與 R 的可觀察反例在案例中分開。
部分載入的 TS store 分別回報基本文字、日文依據與 annotation 的就緒狀態，依 §4.1；普通文字可先提供，完整來源／標註 view 在所需閉包驗畢後提供。
完整下載後缺引用是錯誤，不是缺譯；Python 完整 reader 與 U 必驗全閉包。

## 2. 公開型別、欄序與欄位責任

以下所有列與巢狀 tuple 均固定長度；每格必填，`?` 僅表示可 null。
所有 object 都封閉，拒絕重複鍵與額外鍵。UInt 為 0..9007199254740991 的整數，Bool 不當 UInt。
ID 是非空公開識別；Code 沿既有 ASCII 規則；Lang 須在本快照 language 登錄。
未知 enum、字串數字、非 Bool 品質旗標、缺欄都拒絕。基礎 canonical、日期等仍依既有傳輸契約。
本節的 PA 編號對應固定案例 group；表中 R 一律同時指 Python 與 TS。

### 2.1 tuple 白名單

| 型別／公開集合 | 固定 columns（依序） | 鍵與分割 |
| --- | --- | --- |
| AnnotationRange | start, end | 巢狀 tuple |
| Annotation | ordinal, reference, ranges, bold | 巢狀 tuple；ordinal 於 set 內唯一 |
| annotation_set | id, text_unit_id, occurrences | PK id；按列分 bootstrap／detail；只出被 field_annotation 或 translation 引用的列 |
| PublicTextPointer | owner, field, ordinal | 巢狀 tuple；owner 是下述封閉 JSON |
| field_annotation | owner, field, ordinal, annotation_set_id | 複合 PK 前三格；原文用途唯一 |
| annotation_concept | id, category, explanations, card_ids | PK id；只有實際 annotation 引用的概念 |
| Translation／translation | id, source_unit_id, target_lang, text_unit_id, origin, authority, low_confidence, annotation_set_id | 原七欄後加一格；兩個 partition 均完整八欄 |
| FieldTranslation | field, ordinal, target_lang, translation_id, basis, source, counterpart | 原五欄後加兩格；receiver 由父 owner 決定 |
| cr_clause | id, cr_version_id, number, text_unit_id, translations | 原四欄後加 `[FieldTranslation]`；global detail |

其餘表／巢狀型別欄序不變。新增三個文字集合，文字集合總數為 43；加三個影像集合後為 46。
`AnnotationReference`、`PublicTextOwner`、`ExplanationReference` 是本版新增的三個具名有限 JSON 型別，
descriptor 使用 `{"json":"型別名"}`；不是可執行 schema 或任意 object。
`x-columns`／`x-types` 釘在本文件 Schema，資料的 types 必與之完全相等；不能由下載資料更改欄序或型別。

### 2.2 annotation 欄位

| 欄位 | 型別／required-nullable／合法域與引用 | P／R 檢查及固定案例 |
| --- | --- | --- |
| annotation_set.id | ID；必填非 null，`ann:`＋64 位小寫 hex | 重算 §3 配方；同鍵異內容、冒用另一 set 拒絕；PA-03 |
| annotation_set.text_unit_id | ID；必填非 null，指公開 text_unit | 必須等用途／translation 的 exact text ID，不只比較文字長度；PA-03／PA-05 |
| annotation_set.occurrences | 非空 `[Annotation]`；必填非 null | 只公開已解析 occurrence；空集合不產生公開列，禁止漏掉非空集合；PA-02／PA-04／PA-10 |
| Annotation.ordinal | UInt；必填非 null | 依第一個 range.start、最後 range.end、reference 的 canonical bytes 排序後從 0 連續；PA-04 |
| Annotation.reference | AnnotationReference；必填非 null | 依 §2.4 驗公開目標、category 及必要閉包；PA-06 |
| Annotation.ranges | 非空 `[AnnotationRange]`；必填非 null | 依 start/end 升序，每項及所有 occurrence 彼此不重疊，允許相鄰；PA-04 |
| Annotation.bold | Bool?；必填可 null | true 可加粗，false 不加粗，null 未定而不加粗；不得把 null 寫成 false；PA-02／PA-10 |
| AnnotationRange.start／end | UInt；必填非 null | exact text 的 codepoint 半開區間，0≤start<end≤長度；PA-04／PA-09 |
| field_annotation.owner／field／ordinal | PublicTextPointer 三格；必填；只有 ordinal 可 null | §2.3 的 owner／欄位／段落必存在且唯一；不憑相同 text 借用用途；PA-05 |
| field_annotation.annotation_set_id | ID；必填非 null | 指非空 annotation_set，text ID 等於該 owner 欄位；原文有 occurrence 時即使無譯文也必有這列；PA-05／PA-10 |

公開投影只為非空集合產生 annotation_set／field_annotation 列。
完整載入後未被任何 field_annotation 或 translation 引用的 annotation_set 列由 P 移除；R 遇到即以 reference 拒絕（PA-06）。
已知原文字段沒有 field_annotation 時，
其公開 occurrence 為空；空字串亦如此。這不宣稱原文沒有術語，只表示沒有可公開的已解析 occurrence。
未知 null 原文不造空字串／集合／用途列；缺列不免除 owner、field 與 exact text 的存在檢查。
P 逐用途比對來源／render occurrence，驗證所有非空集合及其引用均已投影；漏列由 P 拒絕，
R 不能從公開資料判斷被省掉的非空集合。部分載入時仍為未備妥，只有依已驗 manifest 確認該定位無分片，
或相關 field_annotation 分片全部載入並驗畢，才能把缺列當空集合；不能從尚未下載推論為空，
也不按相同 text 借另一用途的 set。
section ordinal 依原 owner 的 Section.ordinal，
action_label ordinal 依 keyword.actions 的原次序；其他 field 的 ordinal 必為 null。
原文用來源 raw occurrence，譯文用 render occurrence；省略來源的葉不造 raw range，
若在譯文實際呈現則有譯文 range。重複概念各留一筆，多段同一次引用用多個 range，不事後搜尋同字。

### 2.3 owner 與原文定位

PublicTextOwner 只准以下 object。除了表內明列鍵，其他鍵拒絕；這些 ID 必須在同份快照的公開表存在。
它不是 Fragment.owner（home_set／global），也不是 SourceBinding／translation_use 的 DB FK。

| kind／精確 object | 合法 field → 原文 | P／R 拒絕條件／案例 |
| --- | --- | --- |
| `{kind:"face_revision",id:ID}` | name→name_unit_id；effect→effect_unit_id；section→sections[ordinal].text_unit_id | revision 的 face／region 必與使用處相合，不把 current 指標套到另一 revision；PA-05／PA-07 |
| `{kind:"printing_face",id:ID,face_id:ID}` | name→printed_name_unit_id；effect→printed_effect_unit_id；flavor→flavor_unit_id；section→sections[ordinal].text_unit_id | printing.id 與真實 face 成員；不借 current、observation 或另一版次字串；PA-05／PA-07 |
| `{kind:"qa_version",id:ID}` | question→question_unit_id；answer→answer_unit_id | 版本不能偷換最新 QA；PA-05 |
| `{kind:"cr_clause",id:ID}` | effect→text_unit_id | 精確 clause／cr_version 閉包；PA-05 |
| `{kind:"vocabulary",vocabulary_kind:Code,code:Code}` | label→label_unit_id | 雙鍵查 vocabulary，不單靠 code；PA-06 |
| `{kind:"product_family",id:ID}`；`{kind:"product",id:ID}` | label→name_unit_id | 不用同名字串配商品；PA-05 |
| `{kind:"keyword",id:ID}` | label→name_unit_id；effect→definition_unit_id；action_label→actions[ordinal].label_unit_id | definition=null 不造原文用途；越界 action 拒絕；PA-05 |

printing name 只取自己的已知 printed 字串；effect／section／flavor 只要自身有原文即能標註與翻譯，
不以 printed_text_state 已確認為先決條件，但照原狀態標示。current、printed 與 observation 的 owner 不互換；
觀測 revision 可用它自己的 face_revision 用途展示，不能標成已知印刷文字。

### 2.4 概念、詞彙、卡名與說明

AnnotationReference 保留共用契約的邏輯形狀，讓 annotation-v1 的 hash 不因 wire 投影改變：

| 精確 object | 公開解析目標／用途 | P／R 拒絕條件／案例 |
| --- | --- | --- |
| `{kind:"glossary",key:ID}` | annotation_concept.id；category=keyword／ability／trait／rule_term | 缺概念、把 card_name 當術語或裸 DB FK；PA-06 |
| `{kind:"vocabulary",key:[Code,Code]}` | vocabulary(kind,code)；顯示該標籤 | 缺任一複合鍵、錯 vocabulary 域；PA-06 |
| `{kind:"card_name",term_id:ID}` | annotation_concept.id；category=card_name | 必整體卡名引用；不在名字內另套同字術語；PA-03／PA-06 |

`glossary`／`term_id` 是保留的引用拼法，**只解析到公開 annotation_concept**；不要求消費端存建置 glossary 表。
概念 ID 沿既有概念識別，不能由顯示中文字重配；card_ids 只從已確認的概念到卡片關係投影。

| annotation_concept 欄位 | 型別／required-nullable／合法域 | P／R 檢查／案例 |
| --- | --- | --- |
| id | ID；必填非 null | 公開概念 PK，唯一且有使用；PA-06 |
| category | keyword/ability/trait/rule_term/card_name；必填非 null | 與 reference 相合；PA-06 |
| explanations | `[ExplanationReference]`；必填非 null，可空 | 按 canonical bytes 排序唯一；引用公開說明，不載入採納經過；PA-06 |
| card_ids | `[ID]`；必填非 null，可空 | 排序唯一，全部指公開 card；只有 card_name 可非空；不得由同名猜 card、face 或 token；PA-06 |

ExplanationReference 恰為 `{kind:"keyword"|"cr_clause"|"ruling_revision",id:ID}`。
keyword 目標須有非 null definition_unit_id；CR 用精確 text_unit；裁定用精確版本的 decision_unit_id
及既有 strength／review_state 警示。語言及來源標示沿目標自己的 translations／資料，不把裁定稱為官規。
沒有可公開說明就 `[]`；不得塞入內部 note、decision ID、URL 腳本或整份來源報告。
詞彙引用可開原 vocabulary label；沒有說明不自造。card_ids=[] 不代表沒有那張卡或已確認 EN-only。
說明頁內容屬 #181，本契約只定必要連結；加粗開關不改 reference、range、複製文字或說明入口。

bold 沿[glossary 的加粗規則](../domains/glossary-adoption.md#5-可修訂加粗)：rule_term 保留當前 Bool／null，
其他 glossary category（含 card_name）及 vocabulary class/type 的引用固定 true。
P 從同一概念設定產生所有原文／譯文 occurrence，R 驗固定類別與同 reference 的 bold 一致；
未定值只不加粗，不刪掉引用。這項資料規則與使用者關閉視覺加粗分開（PA-06／PA-10）。

### 2.5 Translation 與 FieldTranslation

| 欄位 | 型別／required-nullable／合法域與引用 | P／R 檢查／案例 |
| --- | --- | --- |
| translation.id | ID；必填非 null | 沿 render-v3／官方 counterpart 的既有識別，公開 PK 唯一；不出 context／dependency_key；PA-02／PA-06 |
| translation.source_unit_id | ID；必填非 null | 必等 FieldTranslation.source 的原文 text ID；jp_source 為 JP，不改成 EN；PA-07 |
| translation.target_lang | Lang；必填非 null | 等 FieldTranslation.target_lang 及輸出 text_unit.lang，必為已宣告語言；PA-07 |
| translation.text_unit_id | ID；必填非 null | 公開譯文 text_unit；有非 null annotation_set_id 時必等該 set.text_unit_id；PA-03／PA-06 |
| translation.origin | official/project/machine；必填非 null | 沿有效選用值，不能改寫機器來源；PA-02／PA-07 |
| translation.authority | sve_official/digital_official/unofficial；必填非 null | 沿來源權威；jp_source 繁中不能冒充 SVE 官文；PA-07 |
| translation.low_confidence | Bool；必填非 null | true 仍顯示、標待校對；false 不授人工核可；PA-02／PA-10 |
| translation.annotation_set_id | ID?；必填可 null | null 表示譯文無公開 occurrence；非 null 唯一明示非空集合，不按文字反查猜集合；PA-02／PA-03／PA-06／PA-10 |
| FieldTranslation.field／ordinal | Field／UInt?；皆必填 | 父 owner 的合法欄位；section/action_label 才可且必非 null；PA-05 |
| FieldTranslation.target_lang／translation_id | Lang／ID；必填非 null | 唯一鍵仍為 field/ordinal/target_lang，引用上述 translation；PA-06／PA-07 |
| FieldTranslation.basis | own_source/jp_source/official_counterpart；必填非 null | 封閉三值，依 §4 驗來源／receiver；PA-02／PA-07 |
| FieldTranslation.source | PublicTextPointer；必填非 null | 指存在且非 null 的精確來源原文，無 occurrence 時不要求 field_annotation；不能用相同 text 借另一 owner；PA-05／PA-07 |
| FieldTranslation.counterpart | PublicTextPointer?；必填可 null | 只有 official_counterpart 非 null；其 exact 原文必等 translation.text_unit_id，原文 set ID（缺列為 null）必等 translation.annotation_set_id；PA-07 |
| cr_clause.translations | `[FieldTranslation]`；必填非 null，可空 | effect/null 的 own_source；不跨區借 CR；PA-05 |

translation 只出被有效 FieldTranslation 使用的列；source／target text、annotation 及說明均須有公開閉包。
annotation_set_id=null 不免除 text／來源檢查；非 null 卻缺目標仍拒絕，不退回空集合。
official_counterpart 雙端 annotation 都為空時可相等，但原文 exact text 與 owner 檢查不變。
同一 source text 可以有多個 translation／annotation，所有 receiver 分別驗 owner；不全域挑第一筆。
公開 ID 中即使包含內容 hash 也不另公開逐列 source_hash、binding、render dependency 或選用歷史。

## 3. exact 身分與 Unicode 座標

text ID 沿 `t:{lang}:{sha256(exact UTF-8 text)[:16]}`；碰撞拒絕，不自動重配。
annotation ID 精確沿共用契約：`ann:`＋H(`{recipe:"annotation-v1",text_unit_id,occurrences}`)。
H 是 canonical-json-v1 的完整 SHA-256 小寫 hex，無 `sha256:` 前綴。
算 hash 時先將 Annotation tuple 還原成 `{ordinal,reference,ranges,bold}`，range 還原 `{start,end}`；
reference object 原樣保留。不是對 tuple bytes 或 text 單獨取 hash。
建置端依 #495 仍以同一配方保存空 set；公開投影省略空 set 的 ID 與列，不改建置身分或 hash 配方，
reader 也不需重造空 set ID。公開 annotation_set 列若含空 occurrences 即拒絕。
同文字／相同 ranges 而 reference 或 bold 不同，ID 必不同；更換 owner 而 exact set 完全相同可共用，
但非空集合的兩個 field_annotation 仍各自存在。explanations 是概念外部呈現資料，不塞進 annotation hash。

所有位置針對 text_unit 的 **exact Unicode scalar sequence**（不接受未配對 surrogate），不用 NFKC／NFC、
搜尋索引、HTML、圖示替換後文字或 UTF-16 code unit 座標。CRLF 是兩個 codepoint，組合字元亦各自計數。
先驗 `start<end`、整數／範圍／不重疊，再呈現。空文字不可有 range；不裁切超界、不排序修復壞輸入。

TS 先用 `for…of` 走 exact 字串，建前綴表 B，初值 B[0]=0；每個 scalar `ch` 追加
`B[i+1]=B[i]+ch.length`（BMP=1，非 BMP=2）。codepoint range `[s,e)` 對應
`text.slice(B[s],B[e])`；禁止直接 `text.slice(s,e)`。Python 在已拒絕 surrogate 後可直接以同區間切字串。
表可按當頁文字建立並釋放，不全庫常駐。DOM 若將符號換圖，先在 exact 文字上分段，再產生純文字／圖示節點，
不能以 innerHTML 重新計 offset；複製仍為原字串。range 不保證 grapheme cluster 對齊。

固定向量（PA-09）使用自撰 `A😀Ｂé手牌`（é 是 `e`＋U+0301 兩個 scalar，不可先合成）：codepoint 長度 7、UTF-16 長度 8，
`B=[0,1,3,4,5,6,7,8]`。手牌 range `[5,7)` 轉 `[6,8)`。
`㍑手牌` 的 raw 手牌為 `[1,3)`；NFKC 後長度不同，不得套 `[4,6)` 回 raw。
建置的 trace／normalizer 驗證仍依 #495；公開只保 exact raw／render 的位置，不出 canonical_source 或 trace。

## 4. 逐 owner 的來源、語言與模式

依 **2026-10-10 日文唯一來源決定**（[#495](https://github.com/gbaian10/sve-kit/issues/495)）：
JP 自身繁中用 own_source；EN 接收有效 JP 繁中用 jp_source。
JP 有效且同卡同面已確認即可顯示，無 aligned 或已知 divergence 均不阻止；來源缺失／過期、
錯 owner／face 或映射未確認仍拒絕。純 EN 無 JP 的卡由 #500 個別處理，不建立一般 EN normalizer。

| basis | P 與 R 的必要條件 | 顯示及固定案例 |
| --- | --- | --- |
| own_source | source 等於父 owner＋field＋ordinal；counterpart=null；來源 text 與 translation.source_unit_id 相同 | JP 繁中、各非卡 owner 的自己來源；PA-07 |
| jp_source | receiver 是 EN face_revision／printing_face，source 是同 kind 的 JP、同公開 card／face、同 field，兩區 mapping_state=confirmed；target_lang=zh-Hant；counterpart=null | field 限 name/effect/flavor（flavor 僅 printing_face），ordinal=null；PA-07／PA-08 |
| official_counterpart | receiver/source 相同，counterpart 為另一區同卡同面、同種 owner；target_lang=ja/en；origin=official、authority=sve_official；target 等 counterpart 原文與 set | P 另驗 fresh display_checks、精確欄位／ordinal 配對且無相關 divergence；R 驗公開雙端及 enum，不能重造人工核可；PA-07／PA-08 |

jp_source 的譯文來源須為 ja，輸出須為 zh-Hant；本契約中 card effect／flavor 的 origin=project/machine、authority=unofficial。
name 另容許 origin=official、authority=digital_official 的既有數位名字選用，但仍不能當實體官文 counterpart。
未採納 JP 的暫顯 revision 不自動成為跨區 donor；P 依 current／歷史用途的來源有效性驗證，不把顯示可讀當規則或採納證據。

EN 的 effect **一律引用完整 JP effect 翻譯**，即使段落數相同也不輸出 jp_source 的 section 列；
這是 wire 的固定呈現方式，不改 JP 本身 section 的 own_source 選用。
EN 原文區依 EN 的 field_annotation／sections；JP 繁中與「日文依據」原文對照依 source 的 JP owner／effect
及各自 set。需要細看 JP 段落時載入 JP owner 的 sections，不把 EN ordinal 拼接到 JP，亦不翻譯 JP 沒有的 EN 段落。
原文與譯文的逐 occurrence 關係由建置時的同一引用產生；公開不聲稱兩邊 ordinal 是一對一配對鍵。
一個來源引用可重複渲染或被省略；Web 可按 reference 顯示關聯，但不能按同字或 ordinal 造等位置對齊。

printed 接收端的 source 必是精確 JP printing_face；current／revision 接收端必是精確 JP face_revision。
即使字串相同，兩類不能混用。切換 current／printed 時重新選 FieldTranslation 與 field_annotation，
unknown printed 不能拿 current 譯文冒充；derived 保留推定標示。
非卡 owner 不使用 jp_source／official_counterpart，也不按商品／QA 同名或同編號推跨區對應。

卡面 region 決定卡圖及主原文，UI 語言決定翻譯列，指定 printing 不因語言改變。
zh-Hant＋EN 有 jp_source 時顯示英文主原文、完整 JP 繁中及「日文依據」；無有效翻譯則保留英文並標缺譯。
ja／en UI 不回退繁中；原文單獨顯示仍有自身 annotation。來源類別、authority、low_confidence 與 JP 依據分開標示。
關閉視覺加粗不刪除 set 或說明引用。

`jp_source` 不建立 aligned、不解除 region_blocks，不讓 shared DSL 自動可用，不授機制或官方 counterpart。
W 仍按 card_engine_support 的 region override／shared／block 順序求資格。
不能在公開 FT 加 `aligned`／`automatic`／審核 ID 來宣稱核可；未知欄位拒絕。

### 4.1 三種就緒狀態與部分 reader

就緒按「用途＋根目錄＋manifest／data_version＋地區＋精確 owner／field／ordinal」判定，不是全 store 一個 ready。
載入狀態不增添公開 tuple 欄位、basis 值或 capability，也不降低完整 reader 的來源驗證。

| 狀態 | 合成例與 reader／Web 行為 |
| --- | --- |
| 基本文字 ready | EN receiver 的名稱／基本欄位、FieldTranslation 七格、translation 八格、source_unit_id 指向的字典文字及目標文字已驗；`jp_source` 的 JP source owner 尚未載入。可搜尋／呈現普通字串與 origin／authority／low_confidence，標「日文依據」，不聲稱已核完 JP owner 或完整標註 |
| 日文依據／annotation pending | 上例仍有完整 source pointer 或非 null annotation_set_id，但所需 JP owner／兩區 mapping slice、用途 set／concept 尚未下載驗畢。顯示「日文依據載入中／標註載入中」，不畫未驗 ranges、不造加粗／說明入口；不能當作缺譯、空 set、無術語或已確認 counterpart |
| 完整檢視缺來源（錯誤） | 所需 owner、字典與標註候選檔及其依賴已全部載入驗畢，仍找不到 pointer 指定的 JP revision、原文或非 null set；完整 view 回 reference／owner／text_identity 錯誤，不降級 pending／缺譯或借 current／同字串用途補洞 |

前兩種可同時成立：名稱搜尋 ready，而日文依據／annotation pending。已載入資料顯示錯 card／face／語言、座標或 hash 時立即拒絕，不等全量才報錯。
基本目錄的整區名稱／卡號／基本 facet 索引完整後才開放搜尋，不以來源對照待載阻塞普通文字，也不將部分卡片當完整結果。
效果全文與進階 facet 的搜尋須等各自整區閉包驗畢；未完成顯示該用途載入中，不傳部分結果。

部分 reader 的 async 公開介面須能取得：某用途所需 File keys／依賴、已驗 bytes／已解析資料進度、基本文字結果、來源與標註各自的 pending／ready／error。
查找回傳「未載入」與「完整查找後不存在」兩種不同結果；呼叫完整 annotated／來源 view 前載入並驗其精確閉包。
所需分片由已驗 manifest 與固定配置定位，不從 ID 猜 revision 的 current／history，也不借另一 owner 的 ranges。
這些是介面語意要求，不宣稱現有 TS store 已完成分段 API；Python `read_snapshot`、`read_api.load_export` 與 publish 仍驗全部公開閉包，缺來源即拒絕。

`official_counterpart` 維持嚴格：P 的 fresh display_checks、同卡同面與精確用途、aligned／無相關 divergence，以及 R 的雙端原文、text identity、annotation 一致性均保留。
未完成 counterpart 雙端與標註驗證時不能顯示為已確認的官方對照；可顯示接收端自身原文，對照用途保持 pending。
`jp_source` 的普通字串可讀不授予 counterpart、aligned、機制或 DSL 資格；完整來源／標註 view 不放寬任何檢查。

目前容量 fixture（3.0.0，`preview-20261010T032000Z-0001`）的 FieldTranslation 用途列為 own_source **9,950**、jp_source **5,423**、official_counterpart **0**。
這是實際各文字 File 的用途列計數，不是唯一 translation 數，也不是三種 UI 語言完整驗收。
零 counterpart 未驗真實雙端官方對照；本 fixture 的語言／owner／效果標註覆蓋不能當全三語或手機驗收，合成正反例另依[案例規格](public-annotation-cases.md)。

## 5. 分片、閉包與消費表

N0 的 N=64、band widths 與 base／row_index 沿既有配置；基本目錄的穩定封存／最近包／共用檔及共享字典要求依[傳輸 §4.4](snapshot-transport.md#44-基本目錄容器共享字典與-base-身分)。
role 表示下載類別，partition 表示欄位分割身分，兩者解耦；不因改裝檔省略公開欄位或永久 ID。
annotation_set／annotation_concept 用 global owner，以 `[id]` 算 bucket；首次可見名稱／facet 所需列歸 bootstrap，
其餘歸 detail。同 ID 只存一處，不能隨最後使用者搬 owner。
field_annotation 的 fragment owner／bucket 沿它所指的來源 owner：face_revision 用所屬 card、printing_face 用 printing，
其餘 global 並以 `[owner,field,ordinal]` 的 canonical 值算 bucket；完整列，不用 row_index／base。
partition 跟來源欄位：display revision name 為 bootstrap，其餘 detail；非 display revision 為 history；
printing 字段為 detail；keyword／vocabulary／商品為 bootstrap；QA／CR 為 detail。
這是 field_annotation 可用 history 的唯一情形，不將 history 誤解為 current。

基本目錄的原文、譯文與 facet 字串及必要字典計入基本目錄冷載；來源／annotation 可按 §4.1 延後，實際成必載依賴者仍整檔計入。
說明正文、JP source 比較或 printed／history 的非首屏欄位可以按需載入，不藉 reference 強迫 bootstrap 依賴 detail。
這些邏輯引用由 P／完整 R／U 驗閉包，W 在使用來源對照／說明前載入且驗到目的欄位。
完整文字包包含三個新集合及所有必要文字／說明閉包；分片與 text_all 的 decoded logical view 相同。
所有新集合納入 row_counts、types、changes 的 PK／changed_fields 白名單、檔案 hash 與完整文字離線 ready 判定。
row_counts 只計實際非空集合及用途列；空集合不補列、不產生僅為保存空集合的 fragment。
非空集合變空時，原文用途以 changes 記錄用途列的 retired；譯文則記錄 translation.annotation_set_id 改為 null，
並依既有閉包規則移除不再被引用的集合／概念；不能省略仍有其他用途的共享集合。

| 欄位／資料 | P | Python R | TS R／W | read_api／publish |
| --- | --- | --- | --- | --- |
| format／minimum／capabilities／types | 同步產生並拒未知值 | 准入、完整 descriptor／tuple 檢查 | 准入及按需相同檢查；未支援不渲染新資料 | load_export 完整驗，publish 不另解碼或忽略能力 |
| owner／field／ordinal／source／counterpart | 原來源、用途、face、freshness、採納門檻 | 公開鍵、模式、文字及語言交叉驗證 | 相同；按 receiver 取用，原文比較載入 source | 保留全部公開引用，驗全閉包 |
| annotation id／text／ordinal／reference／ranges／bold | 從 occurrence 產生並重算 ID | scalar 座標、hash、排序、引用／重疊 | 相同；轉 UTF-16 後切字；bold 三態、使用者開關 | 同 Python 驗證，不以未渲染為由略過 |
| concept.category／explanations／card_ids | 必要公開投影，無 DB FK | 複合引用、種類、排序及目標存在 | 說明／卡片入口，未下載保持未備妥 | 連正文閉包一起計入完整文字 |
| translation source／target／origin／authority／confidence | 保留選用、品質與各自 annotation | exact 字串與來源語言交叉驗證 | 顯示來源／待校對／JP 依據；不猜規則資格 | 由同 reader 保證品質格未被省略 |
| region_blocks／support／官方 counterpart | 沿既有區域資格投影 | 不從 basis 補造資格 | 依 support 合成有效狀態，JP 翻譯不解除 block | 不因 JP 翻譯重寫支援狀態 |

不得公開 source_record／source_ref／source_hash、use_id／context_id／binding_id、node_path、trace、
frame／template、dependency_key、adoption_no、decision／review 收據或候選選用歷史。
本版出貨的是公開 owner、text、概念及必要說明引用；壞結構不以 low_confidence、缺譯或丟列補救。

## 6. 拒絕順序、固定案例與驗收界線

P、完整 R、U 的共同錯誤分類如下；訊息文字不要求相同，不可用任意例外當預期拒絕。
W-only 的語言／資格案例驗具體成功結果，P-only 的來源真實性案例驗來源檢查，不能誤稱 R 重驗了封存證據。

| 分類（依驗證先後） | 拒絕內容 | 固定 group |
| --- | --- | --- |
| wire | 非 canonical、重複鍵、浮點表示、surrogate | PA-02／PA-09 |
| format／reader_version／capability | 未支援版本、minimum、能力未知／缺少／亂序／重複 | PA-01 |
| shape／enum／tuple／descriptor | 必填／nullable／額外欄、未知 basis、長度、欄序 | PA-02 |
| duplicate_key | 重複公開主鍵或複合鍵（含父 owner 內 FieldTranslation 的 field/ordinal/target_lang） | PA-03／PA-05／PA-06 |
| reference／owner／text_identity | 缺公開目標、未被引用的 annotation_set、錯 card／face／模式、text 不相等 | PA-03／PA-05／PA-06／PA-07 |
| range／ordering | 負值／空／超界／重疊、ordinal 或集合排序錯、排序唯一陣列的重複元素 | PA-04／PA-06／PA-09 |
| identity | exact text ID 或 annotation-v1 hash 錯配 | PA-03 |
| basis | own_source／jp_source／counterpart 的語言、來源或品質組合矛盾 | PA-07／PA-08 |
| source_invalid | P 才能驗的缺來源、過期或未確認來源身分 | PA-08 |
| annotation_incomplete | P 比對來源／render occurrence 後發現非空集合或用途被省略／改為 null | PA-05 |

Schema 可先定位結構型錯誤；各案例只引入一個目標違規，已壞的負數 range 統一歸 range 而不是任意 shape。
Schema 的 enum／tuple／形狀錯誤按對應分類；owner／range／basis 的專屬條件即使由 Schema 捕獲，仍回該責任分類。
公開主鍵／複合鍵重複一律歸 duplicate_key，無論同鍵內容相同或不同，在引用與 hash 檢查前拒絕；
陣列內重複元素（card_ids、explanations 等排序唯一陣列）歸 ordering。
JSON object 的重複鍵仍歸 wire，occurrence ordinal 與 range 次序仍依 ordering／range。
多重語義違規先驗公開引用與 text，再驗座標／身分與 basis；同類按公開主鍵及欄序，不先重算 hash 掩蓋錯 owner 或 range。
案例 mutation 可以故意不重算 ID，用來證明先抓目標錯誤；完整 reader harness 必重釘有效的外層檔案 bytes/hash
以到達該語義邊界，不能因外層 hash 失敗就算通過。

容量依 [size-budget](size-budget.md#30-annotation-與-jp-來源的計帳)逐項計入；
N0 已量測既有完整文字、單片與 JP／EN 啟動，超標依維護者豁免交付；修訂後按單區完整來源閉包、基本目錄容器及其他資料檔量測，不沿用舊帳當新配置通過。
本單只驗 Schema 與案例可讀／一致；不得把合成小樣本或既有 2.0 數字當成新格式容量通過。
