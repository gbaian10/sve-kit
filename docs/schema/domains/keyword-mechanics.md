# 關鍵字登錄與機制投影

查卡的關鍵字篩選分四種關係：卡片本身有（has）、給別的卡（grants）、效果提到（refers）、
依數量計算（counts）。本入口依[建置資料庫 §10](../build/build-db.md#10-dsl驗證與未實作卡片頁)與
[快照三態](../export/snapshot-format.md)提供查卡正標籤與已檢查範圍，不變更引擎支援狀態。

## 採納輸入

`authored/rules/keyword-registry.yaml` 是唯一入口；有界 YAML 讀取器拒絕 symlink、
未知欄位、非整數 format、重複 ID／code／term_id。形狀如下：

```yaml
format: 1
kind: keyword_registry
decision:
  id: decision:keyword-registry-v1
  state: confirmed
  note: Adopt the explicit existing keyword concepts for card mechanic projection.
records:
  - { id: "kw:ward", code: ward, term_id: "term:keyword.ward" }
suppressions: []
```

records 明示永久 `kw:<code>` ID、英文 code 與既有 glossary term ID。
預設詞彙類別為 `keyword`，也可透過 `term_category` 明示既有的 `ability` 或 `rule_term`。
資料庫仍使用 `kind=printed_keyword`，definition 為 null、actions 空。
缺 term、類別不符或多重概念不能靠拼寫補配。日文名稱及多語 label 使用同一
已採納 glossary 概念的選詞與 low_confidence，不另維護譯名。保留 choice 的 origin、
source_id 與來源權威；已驗的官方數位詞典選詞為 official／digital_official，
不得改成 project／unofficial，也不得冒充實體 SVE 官文。品質合併 term／choice 的 low_confidence。

decision 是 mapping 的明示採納，不是逐卡真人審核、DSL review 或 engine pass。
建置保存 registry exact bytes 的 source_record、decision 及引用；configuration 納入
registry hash 與各 adapter 版本。
共用關鍵字的日文規則名稱登錄在 `records[].rule_name`。指示物、能力標記與其他僅供引擎使用的名稱登錄在 `rule_names`。`ward` 是採納代碼，Rust 將它轉成對戰狀態既有的 `guard` 鍵。

整份登錄中的專案自有貢獻採 CC0-1.0，官方詞彙及其他第三方內容依 [LICENSING.md](../../../LICENSING.md) 排除。

## 由 DSL 推導的四種關係

`dsl-keyword-relations-v1` 讀已准入的整卡程式（`dsl_document.ast`，已展開），不比對卡文。
只處理八個持有性質的關鍵字：ward、storm、rush、bane、assail、intimidate、aura、drain。
其餘登錄概念（Quick、Stack、兩種 Drive、Starting Amulet、最終上限解放可能）在 DSL 以各自的
語意節點表達（例如增減 Stack、上限解放條件、Quick 篩選），哪些算「提到」尚無採納的定義；
這條 producer 不為它們產標籤，也不提交檢查，授予／提到／計數三種關係維持未知。

走訪每個能力（卡片層與各面），不讀 Token 身分與建牌規則。對照如下：

| 節點 | 條件 | 關係 |
| ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- | ----------- |
| 能力層 `keywords: [...]` | 卡片自己的宣告 | has |
| 授予：步驟 `grant.keywords`、`grant.instead[].keywords`；效果 `grant.keywords`、`keywords.list` | 對象是這張卡自己 | has |
| 同上 | 對象是其他物件 | grants |
| 物件篩選器的 `keyword` | 位於計數運算式內 | counts |
| 同上 | 其他位置（選擇、搜尋、費用、事件對象、存在條件、群組對象、減費條件） | refers |
| 效果 `ignore_ward` | 無視守護 | refers ward |

授予對象的判定：

- 明寫 `on`：`this`，或同一能力內對自己操作的步驟所產生的 `result`（含 `{follow: 名稱}`）是自己。
  步驟的 `ref` 是 `this` 或前一個這類結果時，它的 `result` 仍是自己，可以連續傳遞；
  變身（`transform`）的結果是另一個物件。綁定名、`it`、其他 `follow`、群組選擇器都是其他物件。
  群組可能包含自己時仍只算 grants，has 只認明確指自己的寫法。
- 省略 `on`（只有效果層可省略）依外層決定：卡片自己的 `static.effects`（含其中的 `if`）是自己；
  `apply` 看它的 `on`；移動（`move`）自己時的 `entry_effects` 是自己；
  產生、搜尋、變身及移動其他卡的 `entry_effects` 是其他物件。
- 裝備區的 static、`player_effect` 及其他未列出的外層沒有可判定的對象；
  這種授予只要含八個關鍵字之一就視為未分類。
- 被授予的能力有自己的綁定名稱，不與外層步驟互通。
- 授予給其他物件的能力（`grant.abilities`）內部的 `this` 指被授予者：其中的關鍵字授予一律是 grants，
  篩選器仍依位置算 refers／counts。

計數運算式為 `count`、`count_of`、`distinct_names`、`distinct_orig_costs`、`agg`、`history_count`、
`history_sum`。計數內的篩選器只算 counts，不另加 refers。分類以 DSL 的寫法為準：
以 `count` 比較門檻（即使是 1 以上）屬 counts，以 `exists` 判斷有無屬 refers。Token 原生的關鍵字屬於 Token 自己的程式，
不掛到產生它的卡。

未分類的用法不能被准入：能力樹內任何值等於八個 code 之一的位置必須已由上表處理；
鍵名以 `_` 切詞後含八個 code 之一的節點必須列在語意節點表（目前只有 `ignore_ward`）。
不符時作者工具以 lint `unclassified_keyword_use` 拒絕該卡，建置不會為它提交完整檢查。
這項檢查看不到鍵名與值都不含 code 的新節點；**新增 DSL 節點時須判斷它是否隱含提到這八個關鍵字**，
是的話同時補上對照規則。綁定名稱因此不能取成八個 code。

### 已檢查的範圍

- 身分已確認且有程式的卡：正標籤使用 `source_kind=dsl` 並記錄 `dsl_id`；
  對八個關鍵字各提交所有已選現行面的主文與每個補充段落 × 四種關係的 `whole_field=true`。
  程式是整卡的完整表示，所以沒有正標籤的關係即為已確認沒有。
- 缺現行面的卡仍交出已知正標籤，但缺的面無法提交檢查，該卡維持 partial，不授予否定結果。
- 沒有程式、身分已確認、各面現行主文皆為空且沒有補充段落的卡，沒有可推導的內容，
  同樣提交完整檢查（`source_kind=manual`、無 `dsl_id`）。有任何卡文卻沒有程式的卡不提交檢查。
- 身分未確認的卡不產標籤也不提交檢查。

程式的範圍是 shared，來源依[作者契約](../../dsl/author-contract-1.0.md#21-封套與分片)選取。EN 的自身關鍵字、能力種類與印刷機制由英文卡文各自檢查並寫入
`en_override`；consumer 與 manifest coverage 只要該卡有 `en_override` 列就選用它、不讀 shared，
因此 EN 的授予／提到／計數目前一律是未知。要讓 EN 沿用 shared 的推導，須先有英日卡文核對紀錄，
並讓範圍選擇能按關係分開；不能只因 JP 有程式就套用到 EN。

## 新鮮度與補正

`mechanic-source-bundle-v1` 對 card／shared 全部面（含缺 current 的面）、
所選 revision／來源、current basis、主文及有序 section kind／exact hash 計算 source_text_hash。
其他面、補充段落或 basis 變動因此使舊標籤失效；bundle 本身不是完整語意檢查證明。

producer_version 包含 adapter 名稱與八個關鍵字中已登錄的集合，不含中文呈現或無關 commit。
正標籤 ID 由 card／keyword／scope／relation、source hash、producer 與 `dsl_id` 決定；
程式內容改變即產生新的 `dsl_id` 與標籤 ID。對照規則變更必須更新 adapter 版本；
keyword universe 與 coverage 每次重建。fresh 正標籤依 `(card_id,keyword_id,scope)` 合併，
relations 排序去重，不公開原始證據或 unknown／absent 列。

選填 suppressions 每筆必有 `id,target_tag_id,source_text_hash,producer_version,reason,decision`；
decision 同上例且必須 confirmed。只有候選 ID、source hash 及 producer 完全相符才寫入並套用；
不相符報 stale_suppression，不遮蔽新候選。補正只移除正標籤，不改變已提交的檢查範圍。
補正使用獨立的 mechanic_suppression decision category；只有實際寫入 override
時才保存其 decision 與來源，不為 stale suppression 留下未使用的 confirmed decision。

## Coverage 與承載

producer 只提交正標籤與檢查，不提交結論；共用 coverage reducer 合併後才判定。
內部範圍與 producer 鍵見[建置資料庫 §10](../build/build-db.md#10-dsl驗證與未實作卡片頁)。
建置在程式入庫之後執行：DSL producer 儲存證據、套用 suppression、投影一次，
再由卡文 adapter 補上各地區的自身關鍵字、能力種類與印刷機制。
公開每 card／shared 一列，只有所有必要範圍均已完整檢查的 keyword 才進 complete 集合；
全部 keyword 都完整時使用 complete_all。八個關鍵字以外的登錄概念沒有四關係檢查，
所以有程式的卡通常以 exclude 清單列出這些概念。complete／partial 選較短 include／exclude，
同長選 include；只投影 fresh coverage。has present 不證 grants absent；缺標籤且 partial／缺 coverage 仍 unknown。
reducer 重驗內部 state 與 checked_ranges 一致，不能用手填 complete 冒充完整檢查結果。

分母依 manifest 全地區 non-retired，含 provisional。
公開 keyword／mechanic_projection／card_mechanic_coverage 使用既有欄位與分片配置；
前者在共用 catalog，後兩者在 card 原有 engine 檔案。不改格式、capabilities、band width，
不新增邏輯分片。report.mechanics 保存 adapter／producer、證據／投影／coverage 列數及
拒絕原因，不含官方句子。

## 本身有的日英全卡表檢查

`printed-self-keywords-v1` 在四關係 reducer 之後檢查所有非 retired 卡的各地區
current 主文，包含進化面與 token 卡自己的主文，不依賴 DSL。它涵蓋 EN 以及八個關鍵字以外的登錄概念；
JP 的八個關鍵字同時有 DSL 的結果，正標籤取聯集。沿 registry 的永久概念，
JP 用已採納的日文拼寫，EN 預設用同一 code 的英文能力名。多詞名稱可由 `english` 明示
官方表記，例如 Starting Amulet。Quick 另辨識日英各自的獨立 Quick 圖示。
JP 寫 shared，EN 只寫 en_override，兩區獨立檢查。

整行的能力宣告（可接括號 reminder，可含其他能力名），以及明示自身取得能力的有限句型
（JP「これは…【K】を持つ」、EN「Give this follower … K」／「this has K」，可含指定格式的攻防增幅）產生 has 正標籤。
不從引述內的相同句子產生正標籤；未識別的自身取得句型仍保守保留未知。
主文未提到某 keyword 時，可確認沒有明印該 keyword；出現在授予、條件、選擇、數量、
引號能力或未識別句型中的 keyword 保持未知，既有已採納 has 正標籤仍有效。

補充段落依[印刷機制](#職業與合作作品的印刷機制)的同一套段落歸屬判定處理：可辨識為本卡續文的段落
與主文相同，宣告產生 has 正標籤；Token 定義與 reminder 段落不產母卡的 has；無法確認歸屬的段落中
出現的宣告保留未知，不能當成沒有。非本卡續文的段落出現 Quick、Stack、Starting Amulet、
Single Drive 或 Twin Drive 的獨立宣告時，該關鍵字保留未知。跨行引述中的宣告也不能掛到母卡。
缺 current 面、未確認 identity 不授全卡否定完整性；任一已確認面有宣告即為 present。

結果合併到 `mechanic_projection`，完整性另放 `card_mechanic_coverage.has_complete_keyword_ids`。
此集合每次建置從 current 重新計算，不重用另一地區、印刷版或歷史 revision。
四關係 complete 與 manifest coverage 維持原義；「本身有」完整不能令它們變完整。
前端按地區的非 retired card 去重計數；有未知卡時顯示確切張數，為零才移除不完整提示；
unknown 不能命中否定搜尋。

## 能力種類

`ability-heads-v1` 與自身關鍵字檢查共用同地區 current 主文、缺面狀態及跨行引述判定，
辨識 Quick、入場曲、謝幕曲、進化時、攻擊時、進階起動、超進化時、起動、融合九種能力頭。
一般進化費用（12.2）不列入。任一面有標記就授予 card 層正標籤，含進化面與 token 卡自己的主文。

日文讀開頭連續的 `{…}`、`【…】` 或 `《…》` 標記；英文讀官方 icon token，
以及 On Evolve／On Attack／On Super-Evolve 等行首能力頭。融合只在日文能力頭或英文
起動費用開頭的 `Fuse …:` 辨識；本文的 fusion counter、生成 token 定義及引述內能力頭不授予母卡能力種類。
開頭能力頭後即使有引述本文，該能力頭仍屬母卡；跨行引述內的後續能力頭屬被引述的能力。

結果放同 scope coverage 列的 ability_types／ability_types_complete，不從 DSL 判斷，
不改 engine support。這些 code 與八種持有性質的 keyword 分開；後續 DSL 可做交叉核對，
但本入口不以 DSL 補上未辨認的能力頭。

## 職業與合作作品的印刷機制

`printed-mechanisms-v1` 讀各地區 current 主文與可辨識為本卡續文的 sections，
不依賴 DSL、職業或卡包推算；任一面使用即在 card 層級產生正標籤。
這裡的「使用」限自身印刷能力的條件、費用與原生宣告，不包含授予他人、引述、
Token 定義、reminder 或單純提及。能力種類與既有八種關鍵字仍各自獨立。

| 組別 | code | 日文／英文表記 |
| ------------ | --------------- | -------------------------------------- |
| 職業機制 | combo | コンボ／Combo |
| 職業機制 | spellchain | スペルチェイン、SC／Spellchain、SC |
| 職業機制 | stack | スタック／Stack（自身宣告） |
| 職業機制 | earth_rite | 土の秘術／Earth Rite |
| 職業機制 | overflow | 覚醒／Overflow |
| 職業機制 | necrocharge | ネクロチャージ、NC／Necrocharge、NC |
| 職業機制 | sanguine | 真紅／Sanguine |
| 合作作品機制 | serve | 食事／Serve（carrot 圖示 alt 為 feed） |
| 合作作品機制 | lesson | レッスン／Lesson |
| 合作作品機制 | starting_amulet | スタートアミュレット／Starting Amulet |
| 合作作品機制 | single_drive | シングルドライブ／Single Drive |
| 合作作品機制 | twin_drive | ツインドライブ／Twin Drive |

Combo、SC、NC 與 Lesson 必須有完整數字門檻頭；同概念的長短名稱歸一，
數字保存在 `thresholds`。Lesson 費用後可接逗號及其他費用。Earth Rite 讀支付表記，
不將選擇「有 Earth Rite 的卡」當作本卡使用。Stack 與兩種 Drive 必須是獨立宣告行，
可與其他關鍵字並列；加減其他卡的 Stack counter 不算自身宣告。真紅只讀 Sanguine，
不匹配 Crimson。食事只讀行首圖示與 PP 費用入口；EP 的費用替代說明不產生 Serve 標籤。

續段開頭為具名物件加卡片 metadata 時視為 Token 定義；以自身能力頭、關鍵字宣告、
自身條件或直接產生 Token 的動作開頭時可辨識為本卡續文。因此英文 ECP01 食事、
CP03 的 Twin Drive 續段仍可產生正標籤。其餘續段如出現候選機制而無法確認 owner，
該機制保留 unknown。主文內出現標記但未辨識為原生用途的提及，也保留 unknown；
例如 EP 費用說明、選擇有 Earth Rite 的卡與操作 Stack，第一波不為這些關係另作否定分類。
缺 current 面、身分未確認或引述未閉合，也不授予完整否定判定。

結果保存於同地區 coverage 列的 `printed_mechanisms` 與 `checked_mechanisms`。
前者是 `{code, thresholds}` 陣列，後者是已檢查的 code 集合；無正標籤且不在已檢查集合內
表示 unknown，不能顯示為沒有。JP 使用 shared，EN 必須使用自己的 en_override。
兩組各自支援任一／全部，組間取交集；前端列出未知卡數與已確認的正標籤結果。
