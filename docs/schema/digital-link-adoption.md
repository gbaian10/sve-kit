# 數位對應採納與名稱證據入口

本文件細化 [build-db §8](build-db.md#8-數位對應與語音) 的 `digital_link`／`digital_link_coverage` 人工入口，及 [翻譯契約 §5／§6](translation-contract.md#5-概念選詞與數位證據) 的名稱使用條件。這是待審技術契約，**尚未實作入口、匯入器或遷入資料**；不代表維護者已核可任何新關係。永久身分、數位建置表及公開欄位沿既有契約，不增設平行卡名詞典。本文名稱與驗收情境皆為合成資料。

## 1. 範圍與既有接點

只收已採納的 SVE↔數位關係與查核覆蓋，不收數位卡文、圖片、語音或未決候選。`same_card` 指同一改編卡片概念，不要求兩遊戲效果一致；`effect_similarity` 與 relation 正交。`same_character`／`name_only` 可作瀏覽對應，但不能直接套官方數位卡名。相同日文、角色、畫師、圖或信心分數均不是同概念採納。

現有 `translations/digital.py` 的 `import_digital()` 可重建指定數位卡、父卡與名稱面／語言的最小閉包；`select_name()` 可篩已採納的同卡同面關係並依 svwb→sv1 選詞。`translations/names.py` 的 `populate_name_translation()` 可產生 JP→zh-Hant 的 translation 列，尚未支援語義指派、printed owner、use／selection。`translations/importer.py` 已能驗 glossary 的 `digital_name` 證據，但只讀 DB 關係，不建立它們。**這些函式目前都不是本入口的呼叫端**。

registry 固定永久身分；`curation/` 尚無格式／loader。已正式採納的 glossary format 1 及其全入口 loader 不能直接混入新 kind。本契約採獨立入口，共用既有 YAML、canonical hash、batch 決定、predecessor、SourceRef、凍結 parser 與 F1，不改 glossary 必填欄位或重寫舊分片。

## 2. 入口、分片與封套

路徑相對 `authored/`，完整頂層欄位如下；所有欄位必填，未知欄位拒絕。

| 路徑 | 完整頂層欄位 |
| --- | --- |
| `digital-links/index.yaml` | `digital_link_authored_format: 1, kind: digital_link_index, includes` |
| `digital-links/<area>/<filing_key>/<sequence>.yaml` | `digital_link_authored_format: 1, kind: digital_link_shard, review_context, default_decision_id, records, decisions` |

area 只有 `links`／`coverage`。filing_key 為 `[A-Za-z0-9_-]+`，只歸檔，不決定身分、商品或地區；可沿 card 的歸檔代號。sequence 從 001 起按 area/filing_key 連續只增，至少三位十進位。每片非空、一種 kind、一個決定，records 按 record_key 排序，decisions 恰含 default_decision_id 指向的決定。

沿 [authored-layout §1／§2](authored-layout.md#2-分片批次決定與來源) 的嚴格 YAML 1.2、canonical recipe、單檔 <1 MiB／512 KiB 目標。includes 映射完整分片路徑到**解析後 canonical 內容 hash**；完整 index／分片原始 bytes 另釘 F1。拒絕缺檔、hash 不符、重複 key、symlink、絕對路徑、`..`、跨入口引用及未索引檔案。先驗全入口／全部歷史，再按公開範圍投影；不能先濾 JP 或改決定成員。啟用此能力時缺 index 必須失敗，空集合只能明示 includes={}；未支援的新格式不能當空集合。

review_context 沿 [採納契約 §2](catalog-route-adoption.md#2-入口分片與封套) 的 `{context,source_batches}`，保存核對時的程式、依賴、設定與來源批次。核對背景不同須分片；它不引用尚未產生的自身分片。每筆 data 的 review_context_hash 綁共享背景，避免換背景沿用成員 hash。建置另外釘本次入口與來源；背景換版本身不使關係失效，相關內容依 §5 重驗。

record 恰為 `{record_key,kind,filing_key,data,evidence}`；data 恰為：

```text
{subject,adoption_no,predecessor,value,review_context_hash,reason}
```

| 欄位 | 規則 |
| --- | --- |
| subject | §3／§4 的完整選擇鍵；續版不得換鍵 |
| adoption_no | 同 kind/subject 從 1 起連續只增的正整數，不接受 Bool |
| predecessor | 首筆 null；後續恰為 `{record_key,record_hash,decision_id}`，指緊接前件 |
| value | 完整替代值；null 明示撤回，首次不得 null，不是局部 patch |
| review_context_hash | H(本片 review_context)，每筆必須相同 |
| reason | 非空理由，說明實際關係／查核或撤回，不含官方原文 |

record_key 是 `[kind,subject,adoption_no]` 的 canonical JSON 字串。全域唯一，subject 物件按 canonical 規則排序，不用字串拼接猜鍵。evidence 是按 canonical 值排序去重的 `{source_ref,role}` 陣列，SourceRef 沿翻譯契約；恰列 §3／§4 的全部名稱引用，roles 分別為 `sve_name`／`digital_name`，不額外收來源主張或圖片。撤回的 evidence=[]，歷史證據仍完整保留。§4 的目錄引用是結構性證據，另由 value 明示，不偽裝成 text_hash。

### 2.1 決定與續版

decision 恰有 `id,state,scope,category,policy_id,membership_hash,members,sample_ids,authored_by,authored_at,reviewed_by,reviewed_at,reviewed_precision,note`。scope 固定 batch，一筆也是 batch。

| record.kind（area） | category | policy_id | 最低門檻 |
| --- | --- | --- | --- |
| digital_link_adoption（links） | digital_link | digital-link-v1 | sampled／confirmed |
| digital_link_coverage_adoption（coverage） | digital_link_coverage | digital-link-coverage-v1 | sampled／confirmed |

sampled 須有實際真人審核者、時間及非空 checked 成員子集；confirmed 的 sample_ids 恰為全部 checked 成員。集合均排序唯一，不得有非成員。reviewed_precision=day 用 UTC 午夜編碼實際日期，不捏造時分秒；authored 身分／時間與審核事件分開。此門檻沿既有數位政策；不套 glossary 的 delegated_glossary 或模板的 approved_policy 例外。confidence、auto_ok、模型審查、來源官方及規格通過不代替真人事件。

令 H 為 authored-layout 的完整 SHA-256 canonical JSON recipe，Hash 帶 `sha256:`。`record_hash=H(完整 record)`，包含 evidence、review_context_hash 與 reason；members 恰為全部 `[record_key,record_hash]` 按 key 排序，membership_hash=H(members)，decision.id=`d:`＋membership_hash 的 64 hex。review 欄仍須依真實事件驗證，不能因 ID 相同就接受互相矛盾的收據。

每個 kind/subject 只有一條採納鏈。拒絕缺號、分叉、錯前件、重複 record、decision 指向或 hash 錯誤。修正 relation、名稱依據、覆蓋或撤回只追加新分片並原子更新 index，舊分片不動。末筆 null 或末筆對本次輸入 stale 都不回復較早值；stale 不等於撤回，歷史保留，重新確認須追加採納。

## 3. digital_link_adoption

subject 恰為 `{card_id,face_id,game,official_id,digital_phase}`。card_id／face_id 沿永久 registry；game=sv1/svwb，official_id 沿來源的 9／8 位 ASCII 數字字串，不能猜去後綴、丟前導零或由草稿名稱配卡。face_id 與 digital_phase **同時有值或同時 null**；前者驗 face.card_id，後者須是該數位卡實際存在的 phase。

此入口第一版只支援「精確面對應」與「兩端皆未定位面的 card-level 對應」。DB nullable 欄位較寬不表示入口接受單邊面對應。card-level 可以瀏覽，但永遠不供官方名稱。數位 phase 與 SVE front/back／ordinal 分別核對，不能以相同順位、normal=正面、evolved=背面推導。契約的 phase 枚舉沿 build-db；runtime 未支援的額外 phase 必須拒絕，不能丟棄後假裝閉包完整。

非 null value 恰為 `{relation,effect_similarity,sve_names,digital_names}`：

| 欄位 | 定義 |
| --- | --- |
| relation | same_card／same_character／name_only，按實際人工核對記錄 |
| effect_similarity | near_identical／core_kept／reworked 或 null；未知保持 null，不硬轉草稿 close/partial/redesigned |
| sve_names | 非空、排序唯一的 `{printing_id,face_id,name_ref}` 陣列，指核對時 SVE 的完整名稱字串 |
| digital_names | 非空、排序唯一的 `{phase,lang,name_ref}` 陣列，指核對時數位卡的完整名稱字串 |

name_ref 就是 SourceRef，locator 定位完整 name 欄，text_hash 驗 exact UTF-8，不 trim／NFKC／以搜尋名替代。sve_names 按 `(printing_id,face_id,canonical(name_ref))` 排序；digital_names 按 `(phase,lang)` 排序，同 phase/lang 不得重複。

每筆 SVE 證據必須從凍結 JP 卡頁重播：registry 的 source_face_map 明示來源面對應，printing_id 所屬 card 須等於 subject.card_id；精確面 link 的每筆 face_id 均須等於 subject.face_id。不能只驗名字相同、FK 存在或 card_id。card-level 的證據可包含該 card 的不同面，但不得冒充任一精確面 link。

每筆數位證據須定位同 game／official_id 的實際 API 項及明示 phase，provider／語言／phase／完整字串均驗回。精確面 link 只收 subject.digital_phase；card-level 可列實際核對的不同 phase。每個列出的 phase 必有 ja，並完整列入核對時凍結 API 在該 phase 所有非空的 ja/en/zh-Hant 名稱；缺語言維持缺少，不能填 null 名稱或借另一面。目前最小匯入器支援的名稱語言以這三種為限，不能把未知 layout／parser 靜默降格。

同卡判斷不由兩端名稱相等推出，也不要求 SVE 與數位日文名稱一定相等；人工決定須明示為何是同一改編概念。兩端各自 exact 名稱是可重驗的依據，不能以同角色或圖片相似取代。

匯入時以 `(game,official_id)`／phase 解析 digital_card／digital_face，不另配一份數位永久 ID。`digital_link.id="dl:"+H(["digital-link-v1",subject])` 的 64 hex；此為既有 ID 欄的內容定址配方，不另建 registry。relation／證據／decision 修訂不換 id；要換 card、面或數位目標須撤回舊鍵另開新鏈。相同 id 仍比完整 subject，遇 hash 碰撞拒絕。每個 subject 僅 materialize 末筆有效關係與其 decision_id，遵守 DB 同卡同對象同面唯一性。

## 4. digital_link_coverage_adoption

subject 恰為 `{card_id,game}`；非 null value 恰為 `{state,as_of,sve_names,catalogues,links}`。as_of 為實際查核範圍截止 Date，不由建置日、latest cache 或沒有 link 推出；不得晚於真實審核日期。state 沿既有 unreviewed/partial/reviewed_none/reviewed_matches，不是關係的 review_level。

coverage 的 sve_names 恰為排序唯一的 `{face_id,name_ref}` 陣列，保存各永久面出現過的相異完整名稱，不含 printing_id；link 的 sve_names 仍沿 §3 的版次／面證據形狀。每個 `(face_id,name_ref.text_hash)` 只留一項可重播的代表來源，按此鍵排序；不同面同名不合併。由核對時 registry/source_face_map 驗來源面的 card／face 歸屬，不能以去掉 printing_id 免掉來源驗證。catalogues 為排序唯一的**完整凍結目錄引用**，每項恰為：

```text
{store_id,batch_id,source_version_id,parser,inventory_hash}
```

前三欄沿來源歸檔版本引用，parser 沿釘住的數位 API recipe；引用的是完整 JSON 投影，不是任意 locator 或可執行查詢。inventory_hash=H(從該完整投影得到的排序唯一 `{official_id,phase,name_hash}` 陣列)，依 `(official_id,phase)` 排序，ID 原樣保留，name_hash 為該 phase 完整 JA 名稱的 exact UTF-8 hash，缺名稱則明示 null。只能用同 game 的 ja 目錄；重複 official_id／同卡同 phase 或不支援的面結構須拒絕，不能去重吞掉來源衝突。這是新的結構性引用型別，重用歸檔與 parser 驗證，不把 array/object hash 冒稱 SourceRef.text_hash；來源語言名稱仍用普通 SourceRef。

完整目錄也只表示**此次凍結來源的範圍**，不是未來或所有外部來源的全球無對應證明。不能從只匯入已對應卡的 digital_card 表、名稱搜尋命中子集或草稿已選候選算目錄 hash。review_context 及 F1 保存實際搜索的全部目錄；不能把一個局部清單說成涵蓋未檢查的來源。catalogues 按 canonical 引用排序；空目錄本身不能證明無對應。

links 為排序唯一的 `{record_key,record_hash,decision_id}` 陣列，恰指本 card/game 本次範圍內**全部末筆有效** link 採納，按 record_key 排序。不包含 null 撤回、stale 或舊前件，不借單純 confirmed 身分／glossary 決定。same_character／name_only 也算找到的關係，coverage 不只計可提供名稱的 same_card。

| state | 必須成立的查核事實 |
| --- | --- |
| unreviewed | 明示尚未查核；sve_names/catalogues/links 皆 []，也無已有效採納的 link，不宣稱無對應 |
| partial | 已做部分查核但不足以封完整範圍；sve_names/catalogues/links 至少一者非空，允許尚未找到有效 link |
| reviewed_none | 實際完整查核；sve_names/catalogues 非空且通過完整範圍重驗，links=[]，該範圍沒有任何有效 relation |
| reviewed_matches | 同樣完成完整查核；links 非空並恰等於全部有效 relation，不表示每張有官方譯名 |

完整 SVE 範圍含該 card 的所有永久面，在釘住的凍結輸入中各面出現過的相異 JP 完整名稱；wording 未定／勘誤及歷史名稱觀測仍納入，不能只看 current 或只查正面。工具以實際來源與 registry 面對應重算排序去重的 `(face_id,exact name hash)` 集合，與 sve_names 重播的集合完整比較；不以版次為範圍或計數單位。同卡同面同名再錄、另一 raw 版本或換代表來源不增加集合，無須重簽；但每次仍驗新來源與面歸屬。缺某面 JP 名稱、漏相異名字或尚有未決候選，都不能宣稱完成。printed 名稱 unknown 不捏造成 current 名稱；未知印刷資料也不被這份查核宣布為已知。完整查核只對已知凍結名稱集合成立，不聲稱未知資料查完。

完整數位範圍要能依核對時 review_context 重播所有 catalogues，檢查每個 SVE 面／名稱，並保存真人完成範圍的事件。目錄中某 phase 的 JA 名稱 unknown 時，本版名稱查核不能宣稱完整，維持 partial；不能略去該項縮小目錄。批次 sampled 不能把沒查過的 card 升成 reviewed_none；模型 confidence／漏列候選也不能證明完整。無 coverage 紀錄、撤回或對本次範圍 stale 時，公開只呈現缺少覆蓋／unknown，不能合成帶真人 decision 的 reviewed_none。

SVE 新面／相異名稱 hash 或數位目錄成員／名稱 hash 變動、link 續版／新增／撤回使原 links 精確集合不再相符時，舊完整 coverage 不適用本次範圍；保留歷史、列 stale 原因，不自動改寫成 partial／none 或沿用舊決定。名稱集合、目錄相關內容及 link 集合都未變時，新增同名再錄版次、raw 頁其他欄或建置程式版本改變不要求重新簽 coverage；本次來源仍完整驗回並納入 F1。重新查核後追加 coverage adoption。as_of 不保證其後新卡仍查完；sv1 仍 frozen，svwb 新來源只用另行封存的明示輸入，不在匯入時連外。

**首批只遷 link，不採納 coverage**，入口不產生 coverage 分片，不為每個 card/game 填 unreviewed 決定。沒有紀錄即未知；官方名稱的選取依 §6 的 fresh link，不以 coverage 是否存在／完成為前提。公開 required 集合仍輸出 `digital_link_coverage=[]`。發布按 [build-db §16／§18](build-db.md#16-發布閘門與投影邊界) 與 [snapshot-format §7](snapshot-format.md#7-發布閘門與變動報告) 驗已啟用能力、已有列及非空引用閉包，不要求每個 card/game 都有 coverage 紀錄。日後若要採納覆蓋，仍沿本節真人門檻；不先放寬 reviewed_none 成純機械採納。

## 5. 匯入順序、來源與原子性

本節規定後續實作的 composer，不聲稱已存在 CLI。

1. 驗完整入口、不可變 bytes／commit、決定與全歷史續版鏈；由 review_context 重播歷史來源。缺 raw／批次／recipe／hash、錯 locator／語言／父層或來源歸屬均是輸入錯誤，整次失敗，不把失敗行藏成 unknown。
2. 用既有 registry／TextPlan 的公開身分與名稱觀測解析 SVE 卡／面／版次，包含 pending wording；不因 current 尚未決定丟掉 identity。歷史目標合法退役或相關名稱／目標來源變動屬 freshness 問題，不能轉移到猜出的替代卡。
3. 從有效 link 的明示 targets 用 `import_digital()` 建最小數位閉包：全部必要父卡、實際名稱面／語言及凍結來源。只作歷史證據的來源保留稽核，不必把已撤回目標全數公開；coverage 目錄也不要求公開整個數位庫。
4. 驗 link 的兩端名字與相應源版本。本次數位輸入須覆蓋並吻合該 link 的全部 digital_names；若有效目標的新釘住 API 名稱不同或已不存在，該採納 stale，不恢復舊 link，也不以 latest 補名字。歷史 SVE 名稱證據保留；新 owner 名稱不同依 §6 個別判斷，不因同 card 就搬用。
5. materialize 末筆 fresh link 與決定；驗 coverage 的完整來源、範圍與 link 集合，再 materialize 適用的 coverage。引用未知永久 ID 或偽造來源不能當合法退役略過。所有 stale／withdrawn／partial 與待件原因列私人建置報告。
6. 此後 glossary `digital_name` concept_evidence 及名稱產生者才能引用它們；仍重驗精確 face、decision、JP／目標語 SourceRef，不以 FK 存在替代。所有 DB 寫入在 caller-owned transaction 內；任一步失敗 rollback，不能留下半批 link／coverage／翻譯。對外獨立匯入包裝才擁有 transaction。

author source_record／decision_source 保存完整 index、分片 bytes、authored commit、成員、收據、predecessor 與所有歷史證據；名稱和 coverage 的目錄 raw、batch descriptor／receipt、parser 程式／設定、來源 usage 全部納入 F1。loader 返回可按 link id／record key 查核的型別化採納結果，供匯入、glossary 與逐 owner 驗證共享；依據可由釘住的 authored／F1 重播，不新建一張平行名稱表，也不從 translation.id 解碼所有者。後續新 loader／validator／importer 須加進 recipe runtime 依賴閉包。最後由完整建置的 `record.verify` 驗實際 DB source 使用閉包，不能只用各子匯入器的 partial verify 宣稱完成。

官方原文／數位譯名只從凍結 SourceRef 重建，不抄入 authored、測試或報告；目錄只存 ID／phase hash 引用。研究草稿、網站 URL、latest cache、live manifest、網路補抓都不是 runtime 輸入。缺凍結前置先停止遷入，不擅自封存或更新 sv1。關係／coverage／稽核歷史留建置端；公開仍用既有 digital_link／digital_link_coverage 欄位白名單，不公開源文件或收據。

## 6. 每個名稱 owner 的使用閘門

關係採納不是 context 級別的永久名稱授權。後續 #53 對每個 `face_revision.name`／`printing_face.name` 都要重驗以下條件，再產生或引用 translation：

1. 取得 owner 自己的實際原文、source_unit／hash 與凍結來源；由 registry/source_face_map 驗同 card／同 face。face_revision 用該 revision 的來源；printing_face 只用該版已知 printed 字串與它的印刷依據，unknown 直接缺譯，不以 current 填補。
2. 找到本次 fresh、sampled/confirmed、relation=same_card 且精確兩面皆有值的 link。它的 card_id／face_id 必與此 owner 相同；digital_face 必屬宣告 digital_card。不同版次可共用同永久 card/face 的 link，但仍各自驗自己的來源，不能用另一張卡同名當身分證據。
3. 重播 link 的 SVE 名稱依據與 owner 自己的凍結名稱。owner 的完整 exact 名稱必等於此同 card/face 的至少一項已採納 sve_names；可由同卡同面再錄的另一來源證明相同名字，不要求原 raw 頁永不更新，但不能只比共用 context／顯示名。印刷名與 current 不同時，印刷名須有自己的已採納名稱依據，不能繼承 current 的結果。
4. 重播該 link 的 digital_names：同 game／official_id／phase 的 ja 及目標語名字須與本次數位 DB 完整字串相符；目標語缺少仍缺少。digital ja 與 SVE ja 不必相等，但這一對 exact 名稱必在同概念人工核對依據內。不得借另一數位面、base 卡或另一語言的名字。
5. 對此 owner **已合格的候選**沿 `select_name()` 的 svwb→sv1 規則取詞，重用 `populate_name_translation()` 的凍結名稱證據、origin/authority 與穩定 ID 配方。svwb 沒有合格目標語名可退 sv1；兩者皆缺則 pending／回原文。同 provider 有不同有效候選仍拒絕歧義，不按 ID、hash 或 confidence 任取。
6. 只有此 owner 的產生呼叫返回的 translation 才可建立 use／FieldTranslation；不能先有 translation_selection 就跳過前五步。即使其他 owner 共用 source_unit/context 且已有官方譯文，無 link 的第三張卡仍缺譯。

**現有函式的界線**：`select_name()` 只接 card/face，不接 owner 原文；`populate_name_translation()` 只接 JP revision，沒有本契約的 sve_names 入口，也沒有 printed/context_assignment 支援。實作本入口時先把 fresh 證據與 DB 關係接好；#53 的 owner-aware 接線必須在既有選詞／名稱產生路徑增加上述條件，不另實作一套排序，也不能把全域 DB 暫改後假裝只剩某 owner 的合格候選。入口匯入通過不能宣稱所有 owner 已可用。

兩張同 JP 原文、不同改編概念／官方譯名，須有已採納 `context_assignment` 釘每個 owner 的 source_hash／variant／concept_key 與同字異義理由，分開 context。沒有有效指派時，現有 `populate_name_translation()` 的「Ambiguous source name requires adopted context assignment」拒絕保留；不是按 card ID 自動開 variant 或提供自由譯文。#53 接線後才可產生兩份各有依據的官方名，第三張沒有同卡 link 仍不能借用任一份。

`glossary_choice.concept_evidence.digital_name` 沿既有 `{digital_face_id,sve_owner,jp_ref,target_ref,decision_id}`，其中 sve_owner 是永久 SVE face；jp_ref／target_ref 是數位 JA／目標語名稱，不等於 owner 的 SVE 名稱依據。匯入器須同時驗本契約的 fresh link 及其 SVE 證據；不能因 glossary choice 有官方 origin 就放行另一 owner。card_name 概念的預設指派另依翻譯契約／後續 intake，不在這裡靠字串生成 term ID。

單一名稱保留 official_svwb／official_sv1、digital_official；效果翻譯仍 unofficial，取了官方卡名不升整段效果權威。缺官方候選可走另外已採納的專案選詞或明示 pending／原文，不自動採納草稿譯名。printed 的缺譯也不回用 current 譯文。

## 7. 草稿遷入前置與反例驗收

研究資料只提供候選與先前比對線索。正式遷入須核對每個 SVE 永久 ID／面、數位 official_id／phase、凍結兩端名字、實際採納者／時間／checked 範圍；缺來源、面定位或真人事件就列待件。`needs_decision=false` 不自動生成 confirmed 或 sampled；自譯卡名的 glossary 委託不授權數位同卡關係。首批只遷 link，coverage 依 §4 留未知。風味文字、單卡自由覆寫與其他 intake 不在本契約範圍。

### 7.1 連結分兩層採納

分層只決定抽樣批次／逐筆確認的範圍，**不改真人 sampled／confirmed 門檻**，也不將同名機械結果當已確認同卡。先通過 §2／§3 的入口、身分、面、來源與閉包硬檢查；工具以釘住的凍結來源重算下列條件，不直接採信草稿文字或 confidence。

第一層須原候選為 same_card，且每個納入此候選的 SVE／數位面全部通過：

1. 兩端完整 JP 名稱 exact UTF-8 相等，不 trim／NFKC。
2. 職業相同，以明示、可重播的遊戲 enum 對照驗證；未知或不能證明相同就不通過，不能將不同職業合併來增加第一層。
3. 基本卡種相同（follower／spell／amulet）；進化／token 標記另驗，不能用卡種推面或 phase。未知代碼／其他卡種不通過。
4. 該 JP 名稱在**同遊戲完整凍結目錄**僅對到一個非空 zh-Hant 名稱。查全部 ID／面，包含其他候選之外的同名卡；有第二個譯名或未取得目標語的同名項，都不通過。目錄須驗全部頁及來源閉包，不能只取草稿 sources 或已對應卡的子集。
5. 原比對未標待確認；232 筆先前待決不得因重算條件通過而自動移入第一層。

第一層先機械全查，保存完整候選／正式成員對照、檢查 recipe／設定、目錄範圍與結果 hash；機械分層背景納入 review_context，不新增封套欄位。草稿 hash／待決狀態與轉換結果留遷入稽核，正式建置仍只讀採納記錄與 frozen 證據，不把研究草稿變成 runtime 來源。維護者親自看非空樣本、確認該批同卡關係後，整批以 sampled 採納；樣本數由他決定，工具不得預填 30／50 或假稱看滿固定數量。sample_ids 只列他實際看過的正式 record_key；工具可按穩定排序提供抽樣候選，最後集合／數量與事件依實際審核紀錄，不由模型代看。實際抽樣前只留候選，不能先寫 sampled。這一批可共用一次真實抽樣事件；因分片大小拆成多個成員決定時，各 sampled 決定仍須有其成員的實看非空樣本，不借另一片樣本，也不縮減／複製 checked 集合冒充。分片與樣本範圍一起安排，未涵蓋的分片先留候選；工具不能為填滿每片而擅增維護者決定的整體抽樣數。

第二層為任一條件不成立或無法驗證者，包含 232 筆待決、same_character、mixed、name_only／僅同名、兩端名字不同、職業／卡種不符或同名異譯。逐筆交維護者確認才採納，或先擱置；逐筆確認的正式成員全列 checked（可分 confirmed batch），未決的不進正式入口。擱置只表示暫無此來源的官方譯名，可顯示另行已採納的自譯名或原文，不影響其他卡的抽樣採納與發布。relation 仍依實際判斷，same_character／name_only 即使已確認也不能供官方名；mixed 必須拆成明確的各遊戲／目標判斷，不能寫入 DB enum 或整列升 same_card。

工具產生的 reason 如實寫「名稱／職業／卡種機械相符、同遊戲完整目錄繁中名唯一、原候選未標待決；隨批次真人抽樣採納」及可核對的 recipe／結果依據，不說「已逐筆人工確認」。是否實際樣本以 sample_ids 為準，source_ref／reason／review_context_hash 都參與成員 hash；抽樣不免除逐 owner 的 §6 重驗。分層筆數只報數字，不附官方名字／卡文；另註明計數單位及重疊項，不拿草稿行數冒充已採納 link 數。

### 7.2 最小反例

下面是**後續實作必備的合成反例，不是已跑測試結果**。I 為本入口匯入驗收；N 為名稱/use 接線驗收，含 #53 尚未實作部分；兩者須分別報告，不用 I 通過宣稱 N 完成。

| 編號／層 | 最小反例 | 必須得到的結果 |
| --- | --- | --- |
| D01／N | 卡 A／B 原文同為自撰「範例旅人」，各有已採納同面 link，數位譯名分別為「範例甲」「範例乙」 | 無有效 context_assignment 時拒絕歧義；具各 owner 的有效同字異義指派後分別產生／綁自己的名字，不任選或覆蓋共用 selection |
| D02／N | 第三張 C 與 A/B 共用原文／source_unit，但 C 無 link；context／官方 translation 已存在 | C 不得取得官方名，不建立官方 use，列待譯；現有同名未採納 owner 反例也要保留 |
| D03／I、N | 雙面 SVE 僅正面有 link，背面原文碰巧同名；或把背面指定為另一個明示 phase | 無背面證據不能借正面；有核對的背面／phase 才取該面的名，禁止 ordinal 推導 |
| D04／I、N | 已採納 svwb same_card，但該 phase 缺 zh-Hant；sv1 可有／可沒有目標語 | 不捏造語言；sv1 合格時按既有優先序退到 sv1，皆無則缺譯／回原文 |
| D05／I、N | 只有 same_character 或 name_only，官方名、角色、圖與原文都相似 | 關係可瀏覽；不能取 digital_official 名稱，不能升效果權威 |
| D06／I、N | link 後續 null 撤回；或新釘住數位來源的名稱／phase 已改而舊 relation 未重審 | 不恢復較早 link、不借舊 context 的名字；前者 withdrawn、後者 stale，歷史仍可重播 |
| D07／N | 同 card/face 的 current 名稱甲已有譯名，printing_face 印刷名稱乙／unknown | 乙須有自己的完整印刷來源與已採納名稱依據；缺任一條件不得套甲，unknown 也不得造甲原文 |
| D08／I、N | link 指 A 的 face，但 SVE frozen 名稱證據／名稱 owner 實為 B；或 digital 名稱屬另一 official_id／phase／語言 | 來源歸屬／父層錯誤在匯入或使用時拒絕，不只檢查字串／FK |
| D09／I | 沒有 link／只有待決候選，卻填 reviewed_none；或 coverage 只查正面、局部數位清單 | 無實際完整範圍／事件拒絕；有效 partial 或缺覆蓋維持 unknown，不偽裝已查無 |
| D10／I | coverage links 漏列一個 same_character、指舊前件或多列一個他卡 link | 完整集合／精確採納引用驗證失敗；不得因名稱不可用就不計該關係 |
| D11／I | 新增同卡同面同名再錄；另對照新增相異名稱／背面／目錄成員 | 同名再錄不使 coverage stale、仍重驗來源；相異名稱／新面／目錄變更依 §4 標 stale，不改 as_of 或自動造 partial 決定 |
| D12／I | 缺 predecessor、分叉、錯 record_hash、改 review_context 未改 hash、未索引分片 | 全入口拒絕；失敗交易不留半批 DB 寫入 |
| D13／I | 只改 frozen 名稱一個字但維持長度、錯 parser／batch，或刪掉某來源 usage | exact 名稱／來源 pin／完整 F1 使用閉包必須抓到，不以 byte 長度或子匯入器 verify 取代 |
| D14／I | needs_decision=true、confidence=high 或 model_reviewed 被工具直接寫成採納 | 缺實際真人採納收據拒絕；待決候選不取得永久採納 key／官方名 |
| D15／I | 草稿所選同名卡只有一個繁中名，但完整目錄另有同名不同譯名／缺繁中項 | 第一層不通過，留第二層逐筆確認／擱置，不以子集的唯一結果分批採納 |
| D16／I | 五個機械條件通過但未實際抽樣，或樣本來自另一分片 | 不產生 sampled 決定；reason 不假稱逐筆確認，checked 集合須有該片實際樣本 |
| D17／I、N | fresh 已採納 link 與名稱證據完整，但完全沒有 coverage 紀錄 | 官方名不因缺 coverage 被拒；公開 required coverage 集合為 []／未知，不補造決定；其他發布閘門照常驗證 |

docs 階段只審上述形狀與邊界；程式階段再測嚴格入口、交易、來源重播、閉包及名稱呼叫接線。真實遷入另報能重驗的採納數、232 筆待決及新增待件、覆蓋範圍／as_of，不用合成測試數或草稿信心當人工採納數。
