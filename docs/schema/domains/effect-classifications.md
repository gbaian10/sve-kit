# 效果分類候選

本契約保存 v0 分類標註的來源範圍與結構，供後續效果 DSL 工作核對；不是 DSL 語法、裁定、
可執行效果或公開搜尋分類。語法仍以 `dsl/` 為權威，frame 語義依[四層契約](four-layer-translation.md)。

## authored 入口

路徑為 `authored/rules/classifications/<owner>/<sequence>.yaml`。owner 保留首次歸檔卡包
大小寫，序號至少三位且只增；資料依[authored 共通規則](authored-layout.md)分片。
首次匯入時，分類以代表 member 的卡包歸檔，member 以自身卡包歸檔；兩者可跨檔引用。
首次匯入依卡號、face、v0 source 的 `/` 前綴、line、kind、ID 排序，section 在 effect 前；
分類取代表 member 的位置，member 取自身位置。這不是商品收錄證據。

分類封套恰有 `format: 2`、`kind: effect_classification_shard`、`namespace`、`basis`、`records`。
`basis` 包含轉換規則版本 `recipe` 與來源集合 `inputs_hash`。一份 catalog 的 namespace 與 basis
必須相同；混入不同輸入版本會失敗。來源、轉換程式與完整重現清單保存在匯入時的私有證據，
候選 authored 自身的逐筆內容雜湊則隨資料保留。
basis 描述原分類來源與轉換語義，不因儲存表示改變而重鍵；儲存編碼由 format 選定。

`records` 非空，只有以下兩種記錄；未列在下方預設表的欄位必填，沒有預設品質升級：

| kind | 欄位與用途 |
| --- | --- |
| `classification` | `id` 是 namespace 內分類 ID；`status` 恰為 `candidate`；`family` 是原分類代號；`normalized_hash`、`definition_hash`、`label_hash` 固定來源證據；`members` 是完整舊 UID 集合；`abilities`、`clauses`、`relations` 保存分類結構 |
| `member` | `id` 是舊 UID；`template`、`position`、`source`、`body_length`、`raw_length`；`token_header_hash`、`block`（舊 UID）、`evidence_hash`；`clauses` 保存全部 clause 的舊 template ID、body/raw 範圍 |

### 分類文字版本

文字分片位於 `authored/rules/classification-sources/<owner>/<sequence>.yaml`，封套恰有
`format: 1`、`kind: classification_source_shard`、`namespace`、`basis`、`owner`、`records`。
namespace／basis 必須與分類分片完全相同；owner 必須等於路徑中的 owner。
每筆文字版本恰有 `id`、`raw` 與可省略的 `body`；`records` 非空。
`raw` 是 v0 索引引用的原始文字，`body` 是當時分類的正文，不從目前卡表反推。
body 省略或 null 時等於 raw；兩者不同時完整保存 body，不假定它是 raw 的連續子字串。

文字版本 ID 為 `sha256:` 加上 core canonical JSON `{"raw": raw, "body": body}` 的完整 SHA-256，
其中 body 先套用預設值。相同 owner 內同一文字版本只存一次，跨 owner 可以各自保存。
首次寫入按 ID 字典序分片；後續新增依共通規則追加序號。每檔小於 1 MiB。
member 的 `source` 引用自身卡號第一個 `-` 前的 owner 內文字版本；member 分片 owner 也必須相同。
文字分片與分類分片皆為必要入口，不接受找不到文字時退回舊 hash 的讀法。

reader 重算文字版本 ID，驗證獨立保存的 body／raw 長度，再核對子句區段。
不存在的引用、hash 不符、長度不符、區段越界、同 owner 重複版本、跨 namespace／basis 都失敗；
診斷不附卡文。raw／body 不進回傳 Catalog；Catalog 保留由精確文字算出的原有 hash。

### 可讀識別碼與省略值

member 的 id、block、分類的 members 使用原始 UID；分類 clause 的 id 與 relation 端點的 clause
使用原始 CID。member 子句不重複保存 ID：依陣列順序以零起算序號組成 `UID + "~" + 序號`。
reader 對這些 UID／CID 的精確 UTF-8 bytes 作 SHA-256，還原原本 Catalog 的所有識別碼；
不解析名稱、不正規化 Unicode、不換成局部序號。文字版本 ID 不取代 member 身分。

| 位置／欄位 | 省略或 null 時的值 |
| --- | --- |
| 文字版本 `body` | 同筆 `raw` |
| member `raw_length` | 同筆 `body_length` |
| member `block` | 同筆原始 `id`（UID） |
| member clause `raw` | 同筆 `body` 區段 |
| member `token_header_hash`、position `ordinal` | null |
| ability `cost_timing`、`note_hash` | null |
| classified clause `decision`、`capture` | null |
| relation endpoint `ability` | null |

其餘資料不作省略推算；完整 members 集合、各類證據 hash、子句與 relation 順序皆保留。
wire format 1 的 hash-only 分類分片不再接受；reader 的內部 resolved Shard 保留原格式，
供 Catalog 與 matcher 使用，不是另一個 authored 入口。

分類的 clauses／relations 描述一個代表 member，分類 clause ID 集合必須恰好涵蓋該 member
的全部子句，且唯一屬於它；其他 member 的舊 C 樣板序列必須相同，依序對應。
`members` 的順序沒有意義，不能用第一個 member 推斷代表；reader 驗證上述兩項不變式。

`position` 恰有 `card_no`、`region: jp`、零起算 `face`、`field`、`ordinal`、`line`。
effect 的 ordinal 為 null；section 的 ordinal 為非負整數。卡號保存原樣，不靠字串改寫推算身份。
body/raw 範圍為 Unicode code-point 半開區間；雜湊則使用原始 UTF-8 bytes，不作 Unicode 正規化。

ability 恰有 `id`、`head`、`ir`、`cost_timing`、`note_hash`。
classified clause 恰有 `id`、`ability`、`decision`、`capture`、`stage`、`ir`、`author`、`note_hash`。
timing 可為 null；保留舊原值，不將未知轉為 Defined。這些代號只是原分類的註記，不是通過 DSL schema 的 AST。

relation 恰有 `type`、`sources`、`targets`、`note_hash`。type 可為 sequence、branch、cost-of、
result-of、capture-of、scope-of、simultaneous。兩側為非空、有序端點列表，每個端點恰有
`clause` 與 `ability`；ability 可為 null，但該 clause 必須在本分類中只有一個 assignment。
同一 clause 可分屬多個 ability；此時端點必須明示 ability。邊的順序與端點分組均保留，
不把 relation 壓成只剩型別的計數，也不因同端點而去重。

reader 驗證跨檔唯一性、逐分類完整 member 集合、clause 來源範圍、ability／relation 局部引用、
block 的同卡／同面／同欄位／同 token header 範圍。重複 ID、位置、漏 member、未認領 member、
懸空 clause 或不明確端點都失敗。未知欄位、品質旗標、未知路徑、symlink 與不合法 YAML 亦失敗。

## v0 匯入規則

namespace 為 `classification-v0`，recipe 為 `v0-classification-candidates-1`。
來源集合涵蓋 templates、index、clauses、summary、catalog-tools 選用的 labels，以及原索引與選檔工具。
`inputs_hash` 是以 classification 目錄為基準的 POSIX 相對路徑（例如 `v0/templates.jsonl`）到
`sha256:<hex>` 字串值的對照表的 core canonical JSON 雜湊。
完整檔案 hash 清單保存在私有匯入證據；不將本機路徑寫入 authored。
文字分片只保存此 v0 分類引用的日文 raw／body；這是本批匯入範圍，不定義其他卡片資料的存放政策。
官方文字與識別碼內的官方名稱不在專案授權範圍，依 [LICENSING](../../../LICENSING.md)。

- T、C 與 family 代號原樣保留。authored 保存原始 UID／CID（可能含官方 token 名稱）；
  resolved member、clause、block ID 仍為其精確 UTF-8 SHA-256。
- definition、label、member evidence 與 token header 的 hash 來自完整原始物件的 core canonical JSON；
  normalized、raw、body、note hash 來自各原始字串的 UTF-8 bytes。所有 hash 使用 `sha256:` 前綴。
- 原始 variants 由完整原始 definition hash 固定。原始 label 的自由文字、scope、gaps、
  evidence、exam、variants_checked、split、各 status、
  escalate、confidence、notes、provenance 與 review_verdict 仍由完整原始 label hash 固定。
  其中包括 disagree 118 筆與 fix 314 筆；這些訊號不在候選欄位中，需要解讀時查私有來源，
  不視為已採納語義。relation note 也不可因只保留 hash 而忽略其限制。
- 原本未限定 ability 的端點保留 null；單端點轉為一元素列表，原列表順序不變。
- block／token header 是來源追蹤資料，不表示已完成跨行或 token owner 的語義分析。

這批資料保存原 v0 分類及其已分類範圍，由 AI 標註與審查，未經維護者逐筆確認。frame 對照屬可重算產物，不放 authored；
一次性轉換程式也不進 repo。每次重算必須另外保存使用的凍結 DB／建置 recipe 與程式版本。

## 精確來源對照

`sve_carddb.domains.effects.reader.load(authored_root)` 載入完整候選 catalog。
`matching.FrameOccurrence` 接受 adapter 已解析的 card ID、face ID、JP 卡號、revision owner、
欄位位置、binding ID、frame ID、normalizer、SourceDescriptor、SourceSpan 與完整 raw field。
建構時重新核對 field hash、body role、span 邊界及每個 segment 的 line；raw bytes 不保存在結果物件。
adapter 必須從凍結來源的 card／printing／face／revision／current 與 binding 關係解析身份，
不可只憑同卡號文字猜 card ID。此 API 不代替 DB 的外鍵與 frame 定義驗證。

`Matcher` 先限定完整 position，再比 body bytes hash。不搜尋其他 face、field、section ordinal 或 line，
不去空白、不作 NFKC。body match 只證明原索引 body 與新 binding 拼接 bytes 一致，
**不宣稱舊整個 field、提醒文或 token owner 語義相同**；新 field 的完整 hash／source ref／span 始終保留。

`current` 指該 JP face 的依據版本（`face_current`）；`historical` 指其餘卡文版本，
不代表時間先後，也不表示版本編號較小。

| status | 判斷 |
| --- | --- |
| `current` | 同位置唯一 current exact 候選 |
| `historical` | 沒有 current exact，但有唯一 historical exact 候選 |
| `source_changed` | 同位置有候選，但沒有 exact body bytes |
| `no_candidate` | 沒有該完整位置的候選 |
| `ambiguous` | 優先層有多個 exact 候選，或同位置對應多個 card／face 身份；不任選其一 |

每筆結果保留 `candidates`（同位置全部證據）、`exact`（含所有歷史 exact）、`selected`
（current exact 優先，否則所有 historical exact）。selected 是待審候選集合，並非已選定語義；
歧義仍保留所有候選。相同 binding 的重複輸入可去重，矛盾 binding 證據則拒絕。
歷史候選不會因 current 存在而被刪除；多個歷史 frame 不可縮成唯一對照。

此入口沒有連到公開快照、查卡 family 選項、裁定採納或執行 capability；後續採納須另走各自契約。
