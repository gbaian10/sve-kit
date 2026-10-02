# 風味文字的整段模板與譯本採納

本文件補充 [翻譯契約](translation-contract.md) 的 flavor 路徑。風味文字採整段對譯與寬鬆的譯本採納門檻；
符合 §4 的風味模板定義可逐筆機械全查後引用有效政策採納，不必每包人工抽查。
首輪譯本的實際抽查數量與集合由維護者決定，不預填已完成抽查或逐筆採納。
效果文字維持同語意同譯，單卡自由譯文先不做。
以下固定技術格式與驗收規則；契約合併不等於已有真實首輪抽查、政策核可收據或資料採納。
這是 docs 契約；模板清冊／匯入由 #52、渲染／選用／正式建置入口由 #53 提供，資料入庫由 #197 處理。

## 1. 重用的載體與來源

風味文字使用既有 `sentence_template` 與 `template_translation`，沿用 templates 分片、清冊、
`context→use/binding→translation→selection`、不可變修訂與批次決定，不增設自由譯文 kind 或第二套 renderer。
模板定義與譯本是兩種 kind、分檔採納；即使共用政策，也須各自有完整機械全查與決定，譯本不能代簽定義。

來源 owner 恰為 `printing_face` 的 `(printing_id,face_id)`，`field=flavor`、`ordinal=null`。
`context.source_unit_id` 必須等於這個 owner 的 `flavor_unit_id`。不能從 `face_revision`、另一版次、
同名卡或 current 效果欄取風味文字；文字相同可共用模板／context，各 owner 的 use 仍獨立。
版次與面歸屬先依有效身分投影驗證；表記未定不會單獨阻擋已知的名稱／風味來源。

`source_ref` 沿翻譯契約 §2，定位凍結 parser 完整投影中的**整個 flavor 字串**；
`text_hash` 驗該字串 exact UTF-8。來源 parser 與下節 normalizer 是不同的 pin，不能只換 recipe 名稱。
來源版本、raw、descriptor、receipt、parser 與 normalizer 的完整程式／設定依賴均進 F1。
不讀 latest、草稿或 live manifest 補來源，也不聯網重抓。官方原文與 normalized 字串不寫進 authored、
測試或報告；報告只列 ID、hash、位置與原因，確認頁從私人凍結來源顯示內容。

## 2. 整段分段與清冊

翻譯契約清冊及 `source_span.role` 新增 `flavor`。**只有此 owner／field 可使用**，不套到 effect、
section、name 或 label。除下述只有空白的欄位外，一個非空 flavor 欄位是一段，不以換行、空行、標點或括號再切段。

| 項目 | 確定規則 |
| --- | --- |
| 清冊 entry | 沿既有八欄；`level=sentence`、`line_ordinal=0`、`role=flavor`、`legacy_fingerprint=null`。0 表示整欄唯一段落，不表示第一行 |
| source_span | `{role:flavor,segments:[{start:0,end:L}],anchor:null}`，L 是整欄 Unicode code point 數；非 UTF-8 byte 數或 UTF-16 code unit 數 |
| binding | 進入翻譯的非空敘事欄恰一筆，`ordinal=0`、`params={}`；段落內含 CRLF／LF、行首尾空白、空行、括號與引號，沒有附加 layout／reminder／token_header binding |
| 驗回來源 | trace 取唯一 raw 區間，重組後 UTF-8 bytes 必須等於整欄來源；另以釘住的 flavor recipe 重算 normalized、payload 與清冊 hash |
| 譯文 | 一個已採納譯本可跨行重組語序；譯文換行由譯本本身決定，不能再額外貼回原文 layout |

這是翻譯契約 §4.1 的**flavor 專屬整段例外**：該節的一般卡文規則仍先分出 layout／提示文。
風味文字中的括號一律是敘事內容，不靠形狀分類為規則提醒；不得 trim 原文。
清冊來源 span 和所有實際使用處的 span 都須各自對完整來源驗證；不能只驗代表來源。

**只有空白的已知非空欄位**原樣顯示，不進翻譯清冊、不配模板／context／use／譯文，也不列缺譯。
其原始 text_unit、exact bytes、來源與 owner 仍保留，不能改成空字串或 null；報告另列 whitespace_only_rows。
判定集合固定為 Unicode White_Space 的以下 code points：U+0009–000D、0020、0085、00A0、1680、
2000–200A、2028、2029、202F、205F、3000；每個字元均在此集合才屬只有空白。
不依執行環境未釘版的字元分類猜測，U+200B 等集合外字元不能被當空白丟棄。
因該欄沒有翻譯 context，不是在非空 context 裡用零 binding 假裝覆蓋成功；混有任何敘事字元則整欄照前表驗證。

未知與 exact 空字串分開：`flavor_unit_id=null` 不建假來源、模板、use 或空譯文，列缺來源；
已知 exact `""` 保留其文字單元，沿通則用零 binding，不配 `T` ID、不憑空產生非空譯文。
空欄本身可原樣顯示，不能把缺失來源當成已完整翻譯。非空欄缺譯時回該版次原文並標示缺譯。

## 3. 獨立 recipe 與新 ID

正式實作使用獨立 `flavor-exact-v1` normalizer：輸入上述完整 raw 字串，輸出**相同 code points 與 UTF-8 bytes**。
不 trim、不 NFKC、不換行正規化、不移出括號／標頭、不把數字改 N，也不把引號內容換 X。
`normalizer_version=flavor-exact-v1`；清冊 `recipes` 沿既有六欄，config 恰為 `{}`，其 hash 為 canonical 空物件 hash。
程式、依賴與 config 必須已版控、以完整 commit／exact code hash 釘住；本文件不是可執行實作。
消費端須認得並驗證此 recipe，不能用舊卡文 normalizer 代跑；不支援就拒絕載入。

所有風味模板採新 ID；不繼承草稿流水號或舊 `T`＋10 hex。模板完整 payload 仍是六欄：

```text
{level:sentence, source_lang:<來源語言>, normalized_text:<由凍結來源重建>,
 normalizer_version:flavor-exact-v1, semantic_variant:default,
 parameter_schema:{format:1,slots:[]}}
```

上式只說明配方，`normalized_text` 不進 authored。`level` 由清冊提供，
`sentence_template.data` 仍沿既有九欄，不加 level 或 normalized_text 欄。
`content_hash` 對完整 payload 套 canonical-json-v1；`normalized_hash` 對 normalized 的 exact UTF-8。
新鍵為 `T`＋content hash 前 16 hex，沿歷史全清冊碰撞檢查及每次延長 2 hex 的既有規則，
相同完整 hash 仍比完整 bytes；完整 hash 撞異內容停止，不重配舊鍵。
數字、人名、字寬、換行或任何敘事內容不同就不是相同 payload。

`semantic_variant=default` 是本路徑預設。若確有已採納的同字異義指派，沿既有 context_assignment
及模板語義分叉規則另配新 payload／ID；不能用 variant 達成單卡換翻法。
原文／recipe／schema／語義改變須新模板；supersedes 只指已可驗的舊內容，不能造假父列。
相同 payload 的再錄可共用，新的來源證據仍保留並驗證。

零參數 schema **不綁人名、卡名、數字或術語引用**；這是風味文字專屬取捨，不放寬效果模板的引用要求。
譯本仍使用既有 text 語法：literal 反斜線與左右大括號須跳脫，不能把文字 `{{...}}` 當未知 slot。
風味譯文只用 LF，不得含 CR、首尾空白或結尾換行；各非空行不得以上述 White_Space 字元結尾，
內部空行可保留。驗跳脫後正式 text 與解碼後顯示字串，禁止在審核後偷偷正規化。
候選不符合規則時先改成最終合法譯本，再互審；兩模型意見須對相同 bytes。
互審的 `text_hash` 驗跳脫後正式 `template_translation.text` 的 exact UTF-8 bytes，
不是草稿、解碼後顯示文字或 YAML 排版 bytes。人名改譯等任何 text 變動須新 revision、重做互審與採納，
不得批次替換既有分片；工具可另列受影響候選。

## 4. 譯本政策與歸因

寬鬆的採納門檻不豁免來源、span、ID、譯本語法與採納閉包。
初始化順序為：先 human sampled 採納首輪定義，再取得其最終譯本的真實 human sampled 抽查，
最後建立政策與核可收據。之後新增風味定義／無分歧譯本才可引用該政策，沒有首輪譯本反過來代簽未採納定義的循環。

1. 模板**定義**若尚無有效風味政策，仍須 human `sampled` 批次採納，先建立首輪譯本需要的定義。
   **風味定義的政策採納例外**：只有 role=flavor、flavor-exact-v1、sentence、零參數、完整整欄且符合本文件的定義，
   可逐筆機械全查後引用同一份有效風味政策採納，不要求每包另做真人樣本。
   這不是效果模板定義、概念或一般分類的例外；任何非 exact recipe／額外 slot／未知角色均拒絕政策路徑。
   機械採納為 category=sentence_template 的 confirmed batch，全 members checked，reviewer／時間沿政策收據，
   note 明示「政策核可」，工具作者／套用時間另記。定義沒有譯文 origin 或 model_review，不能偽填兩模型審過定義。
   不改既有九欄 data；decision.policy_id 必須在 authored 內唯一對到**已索引且釘完整 hash 的政策檔與核可收據**，
   正式位置與索引依 [模板採納政策契約](translation-policy.md)，歷史 policy_id 不可換內容或換收據。author source_record／decision_source
   釘完整 authored commit、政策索引與檔案／收據 bytes，政策、收據及真實首輪 flavor 譯本 human sampled 決定均驗回。
   F1 configuration 可另列完整五欄 pin，僅複核上述 authored 有效採納，不能由呼叫端提供另一份同名政策。
   缺唯一索引／hash／收據、建置設定不符或格式／loader 未支援時不得機械採納。
2. 模板**譯本**由一個模型翻、另一個不同模型及版本審，對最終 bytes 記 `model_review`。
   `origin=machine`，互審同意時 `result=agreed`、`resolution=null`。
3. 另立風味譯本政策，與效果長尾政策區分 policy_id／範圍。首輪抽查數量由維護者實際選定，
   依正式清冊頻率優先排審；不能把建議段數或政策核可當成已完成樣本。
   初輪已看譯本用 `adoption_review.mode=human`、真正的 `sampled` 決定，保存精確 template ID／revision。
4. 首輪完成、有實際政策核可收據後，其餘無分歧譯本才用翻譯契約 §2 的 `approved_policy`。
   五欄 policy pin 與非空 initial_sample_decisions 不變；政策限 flavor 定義／譯本、目標語 zh-Hant，
   只按核可 kind 及逐筆 role 開放 flavor 定義／譯本，不給效果 sentence_template、glossary 或 catalog 代簽。
   政策檔／收據另有專用格式，不沿用 wording-rules 的核可。
5. 政策批次為 `confirmed`，全 members 完整機械 checked；reviewer／時間沿真實政策收據，note 明示「政策核可」。
   工具作者／時間另記；這個全查集合不是維護者逐筆人工樣本。報告分開列 human_sampled_rows、
   approved_policy_rows（譯本）、approved_flavor_definition_rows（定義）、待人工分歧與缺譯，
   不把品質門檻較寬鬆改成「已逐筆人工確認」。

風味定義／譯本的政策採納須符合 [模板採納政策契約](translation-policy.md)，具備可驗的 authored 政策索引、
政策／核可收據、真實首輪抽查及完整 loader 支援；**loader 尚未完整支援時不得以政策採納**。
loader 必須逐筆經 template_id 解析到已驗清冊，
檢查整個政策批次的 role 都是 flavor，並核對 recipe／零參數／完整段落，不能只看 filing_key 或 policy_id。
同一政策可以授權兩種 kind，但定義與譯本仍分批、分開計數，任何效果成員混入都拒絕。

有分歧或仍待處理的低信心候選留 authored 外並回原文，不用政策填滿覆蓋率。
若維護者實際處理某分歧，沿 human 批次、resolution 與 sample_ids 規則採納；
信心度只排序，正式格式不新增 confidence。機器譯本審過仍為 machine／unofficial，沒有風味專用官方 authority。
風味缺譯不阻擋核心查卡發布，也不阻擋獨立有效的名稱／效果欄位。

## 5. 格式相容、實作與發布

**context 不含欄位**：build-db §9／目前 DDL 的唯一鍵是 `(source_unit_id,semantic_variant)`，
同語言同 bytes 的 flavor、name 或 effect 可能共用 context；模板 payload 的 normalizer 不同不能分開 context。
由 flavor 模板產生的譯文只可被 field=flavor 的 use 選用，flavor use 也不可選 name／effect 譯文；
須沿精確 binding／模板清冊 role 驗證，不能只驗 context 命中、來源 hash 或 target_lang。
本限制由 #53 的建置選用與投影在輸出前完整驗證，不宣稱現有共用 selection 已有欄位資格檢查。
若一個 context 的混合用途不能以現有唯一 selection／binding 同時合法表示，計入 flavor_context_conflict_rows，
**名稱與卡文優先，只有衝突的風味側回原文**；保留其原本合法的名稱／卡文（含逐 owner 已驗官方名）選用。
由 #53 在報告列 flavor_context_conflict_rows 與對應 owner／context／理由，不讀官方全文估算或隱藏計數。
優先序不授權壞來源／錯 owner 的名稱或卡文；仍先通過原有資格檢查。
不把 flavor 選用散播到全部 use，也不因欄位不同虛造語義 variant 或改 context-v1 配方。
真正同字異義可走已採納 context_assignment；其他需要的選用擴充由 #53 另審，不以本文件偷偷改 DDL／公開格式。

`translation_authored_format=1`、`template_source_format=1`、record／decision／hash recipe 與既有 glossary 必填欄位不變。
新增的是清冊／span 的 role 值與對應嚴格驗證，不修改已採納分片 bytes、索引舊 hash 或舊模板 ID。
尚未支援 flavor role、模板／清冊或政策的 loader 必須拒收，不能忽略非空新資料後冒稱整份入口已驗證。
風味資料需等 #52／#53 相關能力完整接入才可正式載入、重建與發布；本文不修改程式或能力旗標。

沿單檔 **<1,048,576 bytes**、目標 524,288 bytes；對清冊和譯本分片的完整寫出 YAML 計算，
包含封套、recipes、evidence、收據引用、members／sample_ids，不能只量 text。
先計大小、分片，再算完整成員 hash／決定，最後原子更新 index；舊檔只增不改。
單一 record 加封套已達上限就拒絕，不能為繞大小限制把一段強拆成多模板，也不能帶原文入 git。

公開沿既有 `PrintingFace.flavor_unit_id`、`FieldTranslation(field=flavor,ordinal=null)` 與七欄 translation；
tokens 維持 null，風味及其譯文留詳情分片。不增集合、tuple 欄位或 required_capabilities，無公開格式升版。
跨區共用仍須翻譯契約 §7 的實際兩端版次／面／flavor 對照；同卡或同段落數不能取代核對。
既有 shared_jp_unchecked 也須其完整來源／身分／divergence 閘門，不因本文件新增 role 就宣稱 Schema／reader 已接入。
缺譯回原文；缺 raw／hash 錯／閉包錯則是建置錯誤，不能吞成一般缺譯。

## 6. 合成驗收清單

以下都是自撰輸入，不是官方卡文、採納收據或已跑過的測試。每列多個條件分開做最小案例；
反例從相應成功基例只改該條件，不讓外層格式錯誤遮蔽待測拒絕。

| 編號 | 合成正例／單條變更 | 預期 |
| --- | --- | --- |
| F01 | 自撰 `甲２\r\n（乙）\n\n丙`，整欄一 span、零參數、兩模型審正式跳脫譯本 | 原文逐 byte 驗回；譯文可整段換行重組 |
| F02 | 兩版次同一段敘事，來源與 owner 閉包各完整 | 同 payload 共用模板；兩個 use 各指自己的 flavor |
| F03 | `甲２`／`甲３`、`甲『乙』`／`甲『丙』`、CRLF／LF 各一對 | 各產不同 payload／新 ID，不被卡文 recipe 合併 |
| F04 | 一個已知空欄、一個未知欄、一個非空缺譯欄 | 分別零 binding／不造來源／回本版原文，不混同空與缺 |
| F05 | 最終 text 包含自撰大括號與反斜線，正式跳脫且重審 | 正確 literal 顯示，review hash 驗正式 text |
| F06 | human 抽查與風味 policy 收據完整，後續無分歧批次無新增真人樣本 | 可政策 confirmed，分開報人工與機械全查 |
| F07 | 少 CR、少空行、trim 前後空白、改用 UTF-16 offset 各一次 | 各自拒絕覆蓋／roundtrip；不得默默正規化 |
| F08 | 整段 flavor 再疊 reminder／layout，或 flavor role 用於 effect | 各自拒絕重疊／角色不適用 |
| F09 | 用另一版次 flavor、以 current 替代、改 owner 面歸屬 | 各自拒絕 owner／source／身分錯配 |
| F10 | normalizer 不受版控、pin 不符、raw 缺失、舊 10 hex 風味 ID | 各自建置失敗，不讀草稿／latest 補洞 |
| F11 | 同 ID 改敘事／recipe，完整 hash 撞異 bytes | 各自拒絕；合法改版需新 ID，不自動繼承譯本 |
| F12 | 零參數譯本多 slot、未跳脫大括號、字改後沿用舊 review hash | 各自拒絕；最終 bytes 重審不能省略 |
| F13 | 未做初輪抽查、借效果政策、借 wording 收據、分歧仍走政策、machine 改 project | 各自拒絕假採納／錯範圍／錯歸因 |
| F14 | 效果定義借風味政策、風味定義缺 pin／政策收據未授權定義 kind，或同檔混定義／譯本／review mode | 各自拒絕；核可且逐筆全查的 flavor 定義可引用同一風味政策，譯本不能代簽定義 |
| F15 | 單筆加封套恰為 1 MiB，或只測 text 大小 | 拒絕寫出；清冊／分片均測完整 bytes |
| F16 | 敘事人名改譯，只改已核可分片或沿用舊 revision／互審 | 各自拒絕；新譯本 revision 與採納可重建 |
| F17 | EN 同卡但該版 flavor 已知不同，仍顯示 JP 譯文 | 不共用此欄；獨立名稱／效果按各自閘門判斷 |
| F18 | 只有 `空格+CRLF+全形空白` 的已知欄位；加入 U+200B 或敘事字各一次 | 前者原樣顯示、另計 whitespace_only、不造翻譯也不報缺譯；後兩者不能當空白跳過 |
| F19 | JP／EN 各有自撰同一字串，所有來源 pin 合法 | source_lang 不同，兩個模板／context，不因同 bytes 合併 |
| F20 | 同語言同字串同時作 flavor／name／effect，故同 default context；各方向錯借譯文一次 | 不得互相選用；混合用途只能表示一種時，保留合法名稱／卡文，風味回原文並計數，不只憑 context 命中 |
| F21 | 一個政策譯本批次混 flavor／effect；定義批次混角色；只按 filing_key 放行各一次 | 逐筆清冊 role 檢查拒絕；政策閉包或 loader 未到位也不得政策採納 |
| F22 | 正式 text 含 CRLF、行尾空格／tab／全形空白、首尾空白、結尾 LF 各一次 | 各自拒絕；改成合法最終 bytes 後重審，不在審核後正規化 |

實作驗收另核對重跑冪等、完整 F1 閉包、交易失敗無半套產物與公開白名單；
真實入庫量、來源覆蓋與人工事件另報，不以這份清單宣稱完成 #197 或 #53。

## 與名字政策的授權邊界

獨立[數位名字政策](digital-name-policy.md)及same_name瀏覽不授權effect／flavor定義或譯本、首輪抽查／概念／語音。
本契約原封閉scope／kind、樣本與核可事件要求不變，不能因共用context借用另一份政策。
