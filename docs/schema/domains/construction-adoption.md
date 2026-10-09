# 構築規則與禁限採納：construction-adoption-v1

本文件為 [#40](https://github.com/gbaian10/sve-kit/issues/40) 的構築採納技術契約，細化 [build-db §6／§7](../build/build-db.md#7-禁限與構築)。維護者 2026-10-03 決定首批只做 **JP Standard、EN Standard**；首發先交付可查、可追溯的 profile／禁限資料，CR 條文引用等 #48；這段期間固定 ref 尚未兌現，必須明示未確認，不能當作原承諾已完成。其他賽制保持 unknown；整副牌合法性檢查留後續建牌器。契約、來源封存、逐筆採納、資料能力與算法能力是分開的門檻。

本文不提供來源抓取指令、不採納規則數值／公告內容。官方 HTML／PDF、CR 全文與轉錄結果留 repo 外；研究樣本不是正式證據，不能複製進 authored 或用它們的 hash 代替正式原檔。已獲同意抓回的原檔仍須完成 §2 登錄／封存，才能供正式採納。

## 1. 專用入口與採納封套

| 路徑（相對 authored） | 完整頂層欄位 |
| --- | --- |
| `construction-adoptions/index.yaml` | `construction_adoption_format:1, kind:construction_adoption_index, includes` |
| `construction-adoptions/<area>/<region>/standard/<sequence>.yaml` | `construction_adoption_format:1, kind:construction_adoption_shard, review_context, default_decision_id, records, decisions` |
| `construction-adoptions/<area>/<region>/<sequence>.yaml` | 同上，僅供整區共用的 roles／cr |

area 恰為 profiles／revisions／refs／restrictions／coverage／roles／cr；region 恰為 jp/en。roles／cr 是整區共用實體，不帶賽制目錄：角色沿既有 `PK(card_id,region)`，CR 可供裁定及多個賽制引用，不為每個賽制重建一列。其餘五個 area 帶 standard；sequence 按 area／region／賽制（共用 area 無賽制）從 001 起連續只增，至少三位十進位；一片非空、單一 kind／核對背景／決定。首批拒絕其他 format_code，不把 Crossover、Cross Craft、Gloryfinder 併成 Standard 或填合法；共用角色的覆寫不得假裝能表達賽制專屬差異，遇此需求先改契約。擴大 scope 須改契約與驗證，再採納真實資料。

本入口採用 [authored-layout](authored-layout.md#2-分片與來源) 的嚴格 YAML 1.2、單檔 <1 MiB／512 KiB 目標、canonical H、index 與安全路徑、本文件的 review_context 及 decision 欄位。includes 值為分片解析後的完整 canonical hash；本次建置另釘完整 immutable authored revision、index／所有歷史分片的 exact bytes hash。啟用入口缺 index 拒絕，空集合須明示 includes={}；未知欄位／格式、重複鍵、symlink、跨入口引用、未索引檔、缺檔或 hash 不符皆拒絕。先驗全部地區與歷史再投影，不能縮小決定成員。

record 恰為 `{record_key,kind,filing_key,data,evidence,review,adoption_review}`；filing_key 在賽制 area 為 `jp-standard`／`en-standard`，整區共用 area 為 `jp`／`en`，均符合共用 `[A-Za-z0-9_-]+`，不使用冒號。data 恰有 `{subject,adoption_no,predecessor,value,review_context_hash,dependencies,reason}`，subject 依下表；record_key 為 `[kind,subject,adoption_no]` 的 canonical JSON 字串。每個 subject 的 adoption_no 從 1 起連續增加；predecessor 首筆 null，後筆 `{record_key,record_hash,decision_id}` 必指前一採納。value 完整替換，不作 patch；續版 null 明示撤回。有效值先按全歷史續版解出，再匯入一份新的建置 DB，不把前版主鍵內容直接覆寫；相同列僅可逐欄 exact 重用。不能原地修改歷史／重用原決定，同一主體不得分成多條互相搶值的鏈。

| area／kind | subject（完整欄位） | value 範圍 | category |
| --- | --- | --- | --- |
| profiles／construction_profile | `{profile_id}` | `{region,format_code,name}`，name 為人工介面名稱 `{lang,text}` | construction_profile |
| revisions／construction_revision | `{revision_id}` | `{profile_id,effective_from,effective_until,cr_version_id,default_copy_limit,construction_rules_ref}` | construction_revision |
| refs／construction_ref | `{ref}` | §4 的有限 descriptor；ref 不可重綁不同內容 | construction_ref |
| restrictions／construction_restriction | `{restriction_id}` | `{profile_id,announced_on,effective_from,effective_until,kind,state,max_copies,max_selected_groups,members}` | construction_restriction |
| coverage／construction_coverage | `{profile_id,from_date}` | `{until_date,as_of,state,source_set,reconciliation}`，見 §5 | construction_coverage |
| roles／construction_role | `{card_id,region}` | `{role,basis}`，basis 是 H(§6 card 的全部實體面 typed projection)，不得用一面代表整卡 | construction_role |
| cr／construction_cr | `{cr_version_id}` | `{region,version,published_on,effective_on,source_version_id,clauses}`，見 §3 | construction_cr |

record_hash=H(完整 record)，members 恰為整片 `[record_key,record_hash]` 的排序唯一集合，membership_hash=H(members)，decision.id=`d:`＋完整 64 hex。members 涵蓋全部採納成員，一筆也是 batch。human／approved_policy 分片，不混用；政策批次亦不得混用 policy pin 或首輪決定引用。policy_id 是政策的永久版本 key，不按 kind 另配一套 ID；上表只定 category。approved_policy 的 decision.policy_id 必等於 adoption_review.policy.policy_id，human 的 decision.policy_id 固定為 `construction-human-v1` 人工核對規程鍵（比照翻譯的獨立人工 policy_id），不能借政策收據冒作已核可；此保留鍵不得登錄成 approved_policy 的政策 ID。各 category 的實際驗證由同一構築 checks_recipe 與釘版 validator 完成；human 決定亦須完整驗來源與本文件欄位，只是不引用尚未核可的政策。

approved_policy 決定必為 confirmed／batch，sample_ids 恰為全體 checked record_key，意思是**依政策機械全查**，不是本次逐筆人工確認。decision 不保存製作者／核對者姓名或時間，note 明示「政策核可」。不能把政策機械檢查宣稱為逐筆人工核可。human 批次沿共用 sampled／confirmed 門檻：sampled 的 sample_ids 為非空實際樣本，confirmed 恰為全體真人核對成員。禁限／角色覆寫的 human 批次只能 confirmed，批內每筆均由維護者實際核對、sample_ids 恰為全體；與既有 restriction_confirmed_decision 檢查及角色覆寫投影門檻一致。分歧的已採納成員必在實際 sample_ids 並有 maintainer_resolution。首輪 human sampled 或 human confirmed 不依賴未來政策授權，避免初始化循環。不接受 identity／catalog／其他區域／前版決定代簽，不借翻譯收據，不設略過來源／freshness 的旗標。

### 1.1 兩模型核對與維護者事件

維護者 2026-10-03 定案確認方式比照翻譯；協調者依既有交叉審核規則明示：**首輪須有維護者實際抽查才放行；之後兩個不同廠商模型各自對照官方來源核對、互審，無分歧者才可依政策採用**。兩方恰為 Claude 系與 Codex 系各一個模型；同廠商兩個型號、同一模型兩個 session 或僅版本不同不算。維護者仍可另行抽查，有分歧者交維護者，不要求後續每筆真人確認。此文件沒有捏造已發生的首輪抽查、政策核可或資料核對。

每筆 review 恰為 `{basis_hash,model_reviews,maintainer_resolution,maintainer_samples}`。basis_hash=H(本 record 的 `{record_key,kind,filing_key,data,evidence}`)，不含 review／adoption_review，避免循環；model_reviews 恰有兩項 `{vendor,model,version,reviewed_at,basis_hash,outcome,report_hash}`，按 vendor／model／version 排序。vendor 恰為 anthropic／openai 各一，實際 model／version 與受版控工具身分登錄相符，不能只換 vendor 字串冒充跨廠商。各有真實核對時間與完整報告 hash，basis_hash 都等於最終資料；outcome 只允許 agreed／disputed。任何模型更改值後，兩方都須核對最終 basis，不沿用初稿通過的 hash。

政策採納要求兩項均 agreed、maintainer_resolution=null，且沒有抽查拒絕／待處理事件。任一 disputed 不得走 approved_policy，即使真人已解決也須另分 human 批次。maintainer_resolution 是維護者真實確認最終 basis 的 `{reviewed_by,reviewed_at,reviewed_precision,basis_hash,outcome,note}`；outcome=adopted 才成為有效成員，declined／deferred 留候選與診斷，不寫正式 proposed 分片。maintainer_samples 是同形狀的實際抽查事件陣列，沒有事件則 []；不可把全體 checked 填進此欄。首輪或後續的真人核對與政策核可是不同事件，時間可不同，只有日精度時用 day 的 UTC 午夜編碼，不捏造時分秒。

完整模型報告留 repo 外的**核對證據 store**：執行者配置具名 store_id → 本機根目錄，不在 authored／公開資料保存絕對路徑。報告以 exact bytes 的 SHA-256 去重，只增不改，安全相對路徑固定為 `reports/sha256/<前兩碼>/<64hex>.json`；report_hash 是完整 `sha256:<64hex>`。每份報告是 UTF-8 JSON，恰為 `{construction_review_report_format:1,vendor,model,version,reviewed_at,basis_hash,outcome,source_uses,program_revision,dependencies,note}`；source_uses 恰列本筆全部 evidence.use，program_revision 為實際使用工具的完整 immutable SHA，dependencies 為按既有 review_context.context 規格釘住的程式／鎖定依賴。note 只記核對結論／缺口，不抄官方原文。私人原始互審輸入另保存，不進 Git；工具須採納當下驗回其 exact hash 及對最終 basis 的核對關係，不能用工具自行產出的摘要冒作已互審。

review_context.context 的釘版 configuration 的 `construction_review_stores` 只列核對證據的具名 store_id 排序唯一陣列，不放尚未形成的報告 hash、政策收據或本次分片，避免 context → basis → report → context 自我引用。執行者配置另把 store_id 對到本機根目錄；本次正式建置的 F1 configuration 中 `construction_review_reports` 則由已驗完整採納重建為排序唯一 `{report_hash,store_id}` 集合，恰涵蓋全部 model_reviews 引用；報告 exact hash 及實際讀取的使用集合納入採納／建置輸入閉包，不拿它們回填舊 review_context。採納當下及本機正式建置須唯讀取回全部報告、驗 hash／安全路徑／格式及 vendor／model／version／事件／basis／outcome／完整 source_uses／工具依賴逐欄相等，缺檔、未知 store、symlink 或矛盾拒絕。此機械驗證能證明報告 bytes 與宣告釘版一致，不能單靠 hash 證明模型真有看過公告；交叉審核與真人事件仍各自有真實證據。CI 沒有私人 bytes 時只驗 authored 結構，不宣稱已重播報告。證據 store 每批採納前須有獨立備份並 restore-check 通過，備份收據／完整 hash 清單在報告形成後釘入私人採納輸入；失敗不發布，歷史引用永久保留，不因新政策／新卡包刪舊報告。

### 1.2 政策、首輪抽查與核可收據載體

本節獨立定義構築入口的 index／policy／approval、不可變五欄 pin 與真實首輪語意；翻譯入口簡化不改本節的非翻譯採納條件。下列路徑均相對 authored，三檔及索引進 Git，只存 ID／hash／核對摘要與真人事件，不存公告原文或私人路徑。

| 路徑 | 完整頂層欄位 |
| --- | --- |
| `construction-policies/index.yaml` | `construction_policy_index_format:1,kind:construction_policy_index,policies` |
| `construction-policies/<policy_id>.policy.yaml` | `construction_policy_format:1,kind:construction_policy,policy_id,scope,initial_sample_rule,checks_recipe` |
| `construction-policies/<policy_id>.review-queue.yaml` | `construction_review_queue_format:1,kind:construction_review_queue,rows` |
| `construction-policies/<policy_id>.approval.yaml` | `construction_approval_format:1,kind:construction_policy_approval,policy_id,policy_hash,authorized_kinds,reviewed_by,reviewed_at,reviewed_precision,authorization_basis,initial_sample,note` |

policy_id 符合 `[a-z][a-z0-9_-]*`，是永久版本 key；檔名與檔內 ID 相同。policies 為 policy_id → `{path,hash,approval_receipt_hash,review_queue_hash}` 映射；path 固定為該 ID 的 `.policy.yaml`，其餘兩檔由同 basename 換副檔名推得。hash 對完整解析值計 canonical H，三種 Hash 均為完整 SHA-256；索引不含自身 commit SHA。此入口**不列 construction-adoptions/index.yaml.includes**，loader 另驗完整政策索引及全部歷史三檔閉包。空集合明示 policies={}；啟用但缺索引、未索引／孤立檔、缺檔、symlink、未知欄位／格式、不安全路徑或 hash 不符一律拒絕。format 只收整數 1，不收 bool／字串；共用嚴格 YAML 與單檔 <1 MiB 門檻。

scope 恰為 `{kinds,regions,format_codes}`，均為非空排序唯一集合：kinds 是上表已支持 kind 的有限子集，regions 只 jp/en，format_codes 恰為 standard；roles／cr 雖整區共用，這份政策僅授權其 Standard 用途，不授權其他賽制。不得 wildcard 或呼叫端擴充 scope。initial_sample_rule 恰為 `{minimum_count}`，為維護者實際核可的正安全整數門檻；不預填樣本數、不把同一 subject 多版本或 model agreed 湊作真人抽查。checks_recipe 恰為 `construction-two-models-v1`，這是**驗證 recipe key，不是另一個 policy_id**；驗證本文件全封套／來源／依賴、跨廠商對最終 basis agreed、首輪真實抽查及政策收據、完整 coverage／freshness。recipe／授權條件改動須新版本，不只比名稱便放行。

review-queue.rows 恰列首輪完整候選集合的 `{record_key,basis_hash,result}`，按 record_key 排序唯一，result=agreed/disputed；不先濾掉分歧、不存 value 或原文。採納當下由私人完整互審輸入重建，恰等於摘要；後續 authored 結構驗證可以驗此不可變摘要，不能宣稱重看私人原始輸入。政策核可之前，首輪實際核對項以 human sampled 或 human confirmed 決定建立；禁限／角色覆寫只能 human confirmed，批內每筆均由維護者實際核對且 sample_ids 恰為全體，其餘無分歧候選待政策核可後才走 approved_policy。每個 `(kind,subject)` 僅取首輪最終一個 adoption_no；真人至少實際核對 minimum_count 個不同 `(kind,subject)`，且有真實 maintainer_samples／maintainer_resolution；拒絕／暫緩可計實際看過，未回答不計、不進已採納成員。

approval 的 policy_id／policy_hash 必與索引／政策一致，authorized_kinds 恰等於 scope.kinds。reviewed_by／reviewed_at／reviewed_precision 是**真正核可此份政策的維護者事件**；authorization_basis 是非空正式決定定位／URL 與具體核可範圍，不能填模型或工具自行同意。note 記核可限制及初輪門檻。initial_sample 恰為 `{authored_revision,index_hash,decisions,sampled_items,review_context,review_queue_hash,disputed_items}`：

| 欄位 | 完整定義 |
| --- | --- |
| authored_revision／index_hash | 政策寫入之前已完成首輪 human sampled 或 human confirmed 的 immutable 40 碼 SHA／該版 construction-adoptions/index.yaml 完整 canonical hash；全入口與歷史仍完整驗，不引用自身未來 commit |
| decisions | 非空排序唯一 `{decision_id,membership_hash,sample_ids}`；均指首輪同 scope 的 human sampled 或 human confirmed 決定，禁限／角色覆寫只能 confirmed；sample_ids 恰等於該決定實際非空集合，confirmed 恰列全部真人核對成員，不借政策 checked 集合 |
| sampled_items | 非空排序唯一 `{record_key,basis_hash,outcome,decision_ref,reviewed_by,reviewed_at,reviewed_precision,note}`；每項是真人實際核對事件。outcome=adopted/declined/deferred；adopted 的 decision_ref 為 `{decision_id,membership_hash,record_key,record_hash}`，其餘 null。adopted 集合恰等於 decisions 全部 sample_ids，最終 basis 與 maintainer_samples／resolution、真人決定事件逐欄相符；declined／deferred 須非空原因，adopted note 可空 |
| review_context／review_queue_hash | 沿共用 `{context,source_batches}` 釘首輪完整模型與工具／來源輸入，不引用未來政策；摘要 hash 恰等於索引對應項，exact bytes 另釘不可變 revision |
| disputed_items | 恰列摘要全部 disputed，排序唯一 `{record_key,basis_hash,outcome,decision_ref}`，outcome 與 decision_ref 同上；adopted／declined 須在 sampled_items 有相符真人事件。未處理只能 deferred，不捏造 reviewer 或占正式 record；兩處同項的 outcome／hash／ref 必逐欄相等 |

初輪樣本及已解分歧的真人事件須發生於政策核可之前；未處理分歧留候選、不妨礙符合完整政策的其他無分歧項，但永不以政策採納該分歧。首輪 human 決定的實際 samples 與後續政策 checked 分開計數。批准整份政策的人與每個樣本 reviewer 可以不同，但事件均須真實；後續實際抽查仍記 maintainer_samples，不回填到舊收據。修改初輪集合／門檻／scope／互審要求、重核政策、換收據或改檔 bytes 均須新 policy_id／新核可，舊索引項與三檔只增不改。先完整驗證配對／摘要，最後原子追加索引；中斷不自動補收據或跳過孤立檔。

adoption_review 恰為 `{mode,policy,initial_sample_decisions}`。human 時後兩欄 null／[]；approved_policy 時 policy 非空，恰為 `{policy_id,authored_revision,path,hash,approval_receipt_hash}`，path 為 repo 相對 `authored/construction-policies/<policy_id>.policy.yaml`，authored_revision 是保存政策完整索引與三檔的 immutable SHA。所有 hash 與同 revision 索引／收據恰相符；initial_sample_decisions 是非空排序唯一 `{decision_id,membership_hash}`，恰等於 approval.initial_sample.decisions 的該兩欄，不從最新採納猜首輪。每片所有成員用相同五欄 pin／初輪引用，decision.policy_id 必等於該 policy_id。

F1 configuration 的 construction_policies 是由已驗 authored 索引與採納推得的 policy_id → 五欄 pin 映射，恰等於本次引用集合；設定鏡像不能代替 authored 授權。完整索引／政策／收據／摘要 canonical hash、各檔 exact bytes、初輪 revision／決定／來源、validator 與實際依賴都釘入閉包。此收據格式與完整 loader **到位前一律拒絕政策採納**；缺真實首輪、核可收據或必要來源／報告不能先寫政策 confirmed 等以後補證。構築公開資料與介面不得把政策採納顯示成逐筆人工確認；真人樣本、政策 checked 與待人工分歧須分開計數與標示，現行公開欄位不足時不另造本輪欄序或虛構人工標章。

### 1.3 來源重核與首發能力邊界

來源探索可補抓授權範圍的 JP 新聞分頁；已取得的頁數與時間範圍放登錄／探索收據，不用固定頁數當 coverage 規則。每次出新卡包時重抓入口與新聞並重核，得知新公告時亦可另行授權重核；資料只保證到最近核對且有來源支撐的日期，不把排程當延長 as_of。每次真實抓取仍須當次明示範圍與授權，本文件不授權 worker 連網。

首發**不在 #40 做 PDF 條文萃取，也不新增 PDF 依賴**；官版 PDF 可先按 §2 歸檔留存，條文實體與固定 ref 等 #48。其未就緒時，具備來源與上述核對的 profile revision／禁限可以先供查詢，cr_version_id／construction_rules_ref 留 null、inputs_state=unknown，公開明示「CR 引用未完成、固定 ref 尚未兌現」。不得把資料可查等同 ready／牌組合法，或填虛構引用掩蓋未交付。§3–§4 定義後續 #48 接入的契約，不是首發 PDF 實作授權。

每筆 profile／CR／ref 的 region、所有引用 profile 與依賴地區必等於路徑 region；profile／ref 的 format_code 必為 standard，整區共用 CR 不虛構 format_code 欄位；跨區／跨格式釘錯直接拒絕，不靠 URL／官版號猜。profile 的 name 只供介面，不是官方規則引文。default_copy_limit 是同名一般上限，不能把主牌上限塞進此欄。日期均為來源明示的完整生效日，半開 `[from,until)`，未知不以抓取日、公告日或第一天補值。revisions 同 profile 不重疊；coverage 同 profile 不重疊，相鄰可接。修正舊期間須續版原主體；新的生效期間使用新的 revision_id／coverage subject，舊資料與採納收據保留。

restriction.state 的 confirmed／announced／withdrawn 是限制的生命周期，與封套 confirmed 人工核對正交：可核對「尚未實施」或「已撤回」，不能因此視為 active。copy_limit 恰填 max_copies（0 表示禁止），choice_group 恰填 max_selected_groups；另一欄 null。members 是非空 `{rules_name_id,choice_option,deck_scope}` 陣列，按 `(rules_name_id,deck_scope)` 唯一；改 choice_option 不能讓同一名稱／scope 重複，與 DB 主鍵及 staging 一致。同 profile 地區，deck_scope=main/evolve/all。copy_limit 的 choice_option 固定 0；choice_group 的 option 使用連續 0 起編號，每組至少一成員，max_selected_groups 為正整數且小於選項數。選中一組指該 deck_scope 內任成員數量>0，限制的是選中幾組，不是所有張數之和。不同 printing 先按 card／構築名稱合計，雙面實物只算一次；不同卡名的同效果卡不自動合計。

來源未明示完整生效日的限制，不填猜測日期、不偽裝 active，也不丟掉已知公告：保留 evidence 與 reconciliation 的 conflict／unresolved、可查「生效日未確認」的候選及原始官方來源。此候選不是正式 restriction 列，coverage 不得 complete，日期查詢仍 unknown；後續補得正式日期與核對收據才匯入。候選查詢介面屬後續投影能力，未實作時至少由私有診斷列出缺口，不能宣稱已完整可查。

## 2. 一次性抓回原檔如何正式登錄與釘版

此節定義 carddb 已提供的**離線登錄能力**，入口為 [`source-import register`](../../../carddb/src/sve_carddb/ingest/archive/source_import/README.md)，預設只檢查，明示 `--execute` 才登錄；程式合併與本契約均不授權真實執行。輸入是維護者認可的一次性來源集合：index 記 URL／final URL／status／sha256／bytes／fetched_at／content_type／ETag／Last-Modified／chain，raw 以內容 hash 保存。原始 index 與 raw 保留不動，登錄不會對任何外部服務發請求，不開 live manifest 或以它補 metadata。

1. 唯讀驗完整 index 與全部 raw：唯一 canonical requested URL、200 成功狀態、原始 URL／最終 URL／chain 一致、HTTPS／官方 host、有效 UTC 時間、media type、exact raw hash／bytes。禁止 symlink 越界、未知欄位、缺檔、重複衝突、未結束鏈及不符合用途的 HTML／PDF。URL 只沿既有 canonicalizer，不推測 PDF 版號或用來源路徑配 region。把來源清單、原 index exact hash、原 metadata 與本次用途分類釘在獨立登錄收據；它留來源歸檔 metadata 閉包，不進 git。
2. 建立**全新的隔離 manifest／staging**，不複製或接管 live manifest。provider 依核對後來源用途明示 jp/en（EN PDF 即使由共享官方 host 供應仍是 en）；抓取 kind 使用現有 rules（規則入口／CR PDF）、limit（禁限入口）、news（新聞索引／公告），不把 PDF 虛構成 card。Resource.url 取 canonical requested URL；final_url／chain 保留登錄收據，不能把兩者互換或默默合併別的 URL 身分。
3. Resource 的 hash／raw_bytes／content_type／標頭沿原紀錄；stored_bytes 等於這份未壓縮 raw 的實際 bytes，並與 raw_bytes 相同。path 為隔離目錄中的安全相對 path（HTML 用 `.html`，PDF 用 `.pdf`），不得用 `.zst` 假稱壓縮，避免既有封存器依副檔名誤解壓。first_fetched_at／last_checked_at／last_changed_at 使用唯一這次已知 fetched_at，不冒稱官方首次發布或更早抓取，archived_at 在封存前為 null。raw 不轉碼、不正規化；原 ETag／Last-Modified 缺值留 null，缺用途所需 metadata 則拒絕，不自行補造。只登錄既有來源觀測，不造未實際發生的 HTTP fetch_log、重試、歷史或爬取 generation。重複匯入同收據可重用，衝突停止，不覆寫。
4. 使用 [source-archive §2／§3](../ingest/source-archive.md#2-內容來源版本與-inventory) 的既有 raw blob／source_key／source_version_id／descriptor／receipt／inventory／seal 格式，來源版本身份仍是 `{provider,kind,url}`＋raw hash，不能新增另一套「研究來源」ID。inventory 必帶隔離 manifest 的 SQLite backup API 自足副本，其 metadata 閉包含上述登錄收據。準備／重驗／fsync／最後原子發布 seal；沒有 seal 或任一 missing 不能當正式批次。
5. **備份並 restore-check 通過**才交付正式 batch pin／原 index hash／登錄收據 hash／用途分類／數量與 bytes 對帳給協調者。異常停止，原輸入與失敗收據保留，不以重抓／crawl／refresh 修補、不清理其他任務狀態。真實執行由協調者在工具審核／合併且取得當次範圍的明示授權後操作；登錄不自動執行 seal／備份／restore-check，也不採納規則含義。

隔離 manifest 保存 `source_import_receipt` 登錄紀錄，與 fetch_log 分開：保存原 index exact bytes（BLOB）、index_sha256、canonical `{source_mappings,program_revision,dependencies}`、該內容的 receipt_id 與 registered_at；此收據不是 archive observation receipt，不能代替 descriptor.first_receipt_id。source_mappings 恰列每個 canonical URL／provider／kind／raw hash／安全相對 path，原 final_url／chain／抓取 metadata 由 index bytes 保留。receipt_id 對 index_sha256 與 canonical 登錄內容計 H，不以私人路徑或時間配號；相同收據重跑保留原 registered_at、逐欄比對，不覆寫。此表由隔離 manifest 的 SQLite backup hash 一起釘住，故可驗原 index 與 raw 的登錄關係，不向 inventory／Resource 塞未知欄位。現行 carddb 已接入相應 schema／reader 驗證；只有舊 seal 工具或多放一個未被 hash 引用的旁檔不算完成此邊界。入口驗證明示的程式 revision 與列出的依賴 pins，這份固定清單不宣稱涵蓋完整執行閉包。

此隔離格式使用 manifest `PRAGMA user_version=2`，inventory.manifest.schema_version 亦為 2；不是在版本 1 偷加一張表。v2 完整 schema 獨立凍結於 `carddb/src/sve_carddb/ingest/archive/manifest_schema_v2.py`，不隨 live v1 schema 演進而重定義。現行 reader 分版本驗表／欄位／約束、收據與完整 source_mappings：版本 2 缺表、缺欄、缺收據或 Resource 無對應均拒絕，不以 CREATE IF NOT EXISTS 修補已釘副本。僅支援版本 1 的舊 reader 必須拒絕版本 2。現行工具保留版本 1 的讀取／live 寫入能力，live writer 拒絕版本 2；來源登錄的版本 2 writer 僅用於全新隔離 DB，本功能不自動升 live，也不遷移／重寫已 sealed 的版本 1 批次。兩種版本的副本都用唯讀 open_snapshot 驗證，不能只把全域 SCHEMA_VERSION 換成 2 而丟掉舊批次相容性。

seal、獨立備份及 restore-check 通過後，已引用的隔離 manifest backup、index／登錄收據與 raw 閉包永久保留；工作 manifest／其 sidecars 與原一次性輸入不自動刪除。只有協調者確認全部引用已在獨立備份可驗回、沒有未完成或失敗工作後，才可另行授權清理未被引用的工作複本，不能刪 sealed 副本／歷史收據。

被核可的新抓取原檔與舊研究樣本是兩類輸入；不能把「本機有 PDF」當可採納。相同官方 CR 版號、不同 raw hash 是不同 source_version／cr_version_id，歷史保留；同一內容不同 URL 亦依既有 source_key 分開。既有 manifest 格式或 reader 不支援這種登錄收據時直接停止，不能手改資料庫或略過驗證。抓取時間不代替 published_on／effective_on。

一次性取得的來源集合可含兩區 CR PDF、規則／禁限入口、歷次公告、入口漏列的新公告及 news 索引；集合數量與版本標記屬登錄收據，不代表已封存或可採納。明示未取新聞後續分頁／其他賽制不算抓取失敗，但必須按 §5 限制 coverage。登錄成功也不自動核可其規則含義。

## 3. 原始來源、CR 實體與必要引用閉包

evidence 是排序去重的 `{role,primary,use}` 陣列，primary 為布林；use 恰為既有 `SourceUse {source,usage,locator}`，source.kind 只允許 official_page／official_api／official_pdf，source.archive 帶 store_id／batch_id／descriptor_sha256／first_receipt_id。source.id 是真實 source_version_id，raw hash 是原 bytes；每片 review_context.source_batches 必涵蓋全部用到的 sealed 批次。禁止第三方 URL-only、研究 hash、未封存檔、本機絕對路徑或 caller 自稱 verified。角色 role 僅允許 profile／rule／restriction／coverage／role／cr／clause／reconciliation，須與記錄實際值相關。

每筆非撤回記錄恰一項 primary=true；其餘 false，不按陣列第一筆猜。profile／revision 選直接支持該設定／期間的規則來源，restriction 選直接建立或更改本次限制的公告／基線，CR 選該版 PDF，coverage 選其基線（缺基線的 partial 選直接說明缺口的官方入口），ref／role 選直接支持其主張的來源。source_id 為 primary.use.source.id；既有單值 source_url 等於該 frozen descriptor 的 URL，不造假的多來源 source_record。其他 evidence 全列 decision_source 與 F1，後續 source_urls 投影為全部適用 frozen URL 的排序去重集合；入口落後時 restriction 不選該入口當主要證據。primary 不代表其他證據可省略，也不以單值 URL 聲稱 coverage 證明完整。

usage 為 construction_profile／construction_revision／construction_ref／construction_restriction／construction_coverage／construction_role／construction_cr／cr_clause；各用途釘非空實際 parser pin。locator 是 canonical JSON 的 `{record_key,field,context_key,number,location,result_hash}`，可空欄明示 null；location 恰為 `{kind:projection_pointer,pointer,page_numbers}`，pointer 是在該受版控 parser 結果中的 JSON Pointer，PDF page_numbers 非空、排序唯一且從 1 起，HTML 則為 []。result_hash 對該 pointer 的 typed 結果計 H；逐字條文取 UTF-8 exact text hash，與 raw hash 分開。實際重播必驗 pointer 內容、格式上下文、原檔頁面／區塊及全部相關 fields，不能只驗 hash 字串看似有效。

construction_cr 的 clauses 恰為非空、排序的 `{id,number,context_key,locator,text_hash,referenced_clause_ids}` 集合；不含 text。number 是官版原樣字串，不當小數；context_key 為受版控萃取 recipe 產生的格式／章節上下文識別，首次 Standard 根節使用 standard，補充／例外須各有明示 context。實際文字由 frozen 原檔以釘住 recipe 萃取後建 text_unit／cr_clause，不在 authored 轉錄原文。同區、官版標記、來源版本與完整區塊必精確對上；單有 version label／locator／text_hash 或任一條文不算採納完成。

CR 的重複條號不跨上下文覆寫：後續 cr_clause 欄位方案新增 context_key，唯一鍵擬為 `(cr_version_id,context_key,number)`；機器契約實作後公開亦保留 context_key。同上下文重複而無可驗定位的結果拒絕。JP／EN 各用自己的原檔／版本／條文；不能用 JP CR、舊研究版或一段同號附加規則補 EN Standard 的缺口。

CR ID 共用固定 recipe：`cr_version.id="crv:v1:"+hex(H({recipe:"cr-version-v1",region,version,source_version_id}))`；`cr_clause.id="crc:v1:"+hex(H({recipe:"cr-clause-v1",cr_version_id,context_key,number}))`。hex 為完整 64 小寫 hex，version／number 保留官版原樣字串；不含採納時間、parser 版本、條文集合大小或排序。cr_version 的 source_id 即 source_version_id，既有 `UQ(region,version,source_id)` 保持：#48 首次必要閉包與後續完整萃取重用同一列／ID；其他已配 CR 實體也須按原登錄沿用，#48 只增補缺條文，不能重新配號。相同 clause ID 必須對到相同 exact 文字與上下文；衝突先停下修復，不能因換 parser 就覆寫已引用實體。新官版或同官版換 raw 產新 source_version／cr_version，舊 ref 仍能追回原列。

目前 `UQ(cr_version_id,number)` 尚不能容納不同上下文的同號；啟用本 recipe 前，實作 PR 須一起更新 DDL、權威列、驗證及公開機器契約為含 context_key 的唯一鍵，#48 沿用此鍵與 recipe，不另做賽制專屬 ID。不能把上下文偷偷拼入官方 number 避過現行 UQ，也不能用新 schema 回寫舊凍結輸入。recipe／context 分類後續若需更正，須保留已配 ID 並明示修復／版本遷移，不能靜默重新計算既有引用。

每個 ref 的 required_clause_ids 必為有限、非空、排序去重集合，涵蓋以下最低 Standard 查核集合與所有實際依賴：

- Standard 根節及其**全部下層子節（含巢狀）**，集合與數量以已封存的當區官版 Standard 上下文為準，不展開成固定連號，也不由研究版產正式引用。文末可能另有同號區塊，須由 context_key 區分；不能借同號補缺。若官版重編、缺節或適用範圍不同，先明示改版查核集合並審核，不任找固定數目的段落湊數。
- 這些條文實際引用的名稱／卡種定義、雙面／特殊角色／職業／作品條件與適用例外。referenced_clause_ids 列適用引用，以已走訪 ID 集合求有限閉包；允許條文互引，但未解引用不能跳過。每個具體規則欄位都須能連到必要集合的逐字依據或另釘的官方補充規則頁；後者不能假造 CR 條號，單純列無關條號不合格。

「適用」須由本入口 human 核對或所釘政策授權的核對者，依 construction-two-models-v1 驗證 recipe，並依該區官版 Standard 範圍、每個 descriptor 欄位與實際條文引用判定，不由 parser 猜。釘住 configuration 的 clause_scope 為排序唯一 `{cr_clause_id,state,reason,evidence}`，state 為 required／out_of_scope；從根節走訪遇到的每個引用都必解到實體，out_of_scope 亦須有完整採納收據與官方範圍依據，不能以未讀／尚未萃取為排除理由。required 對象的引用繼續走訪；未分類／無證據／循環中的缺節均為未解。若必要閉包實際擴大到大部分或全部 CR，就等待萃取完成而回 unknown，不設深度／數量截斷來湊首批範圍。

required_clause_ids 中每個 ID 必在本次 DB／快照具有同 cr_version_id 的實體 cr_clause，且由該版本 frozen evidence 驗回。僅有一條任意 clause／懸空 CR 字串／缺 parent／缺一子節／錯 context／缺例外，一律回 unknown 並列缺口。此必要閉包是後續固定 ref 的最低條件，首發 #40 不萃取、不宣稱已交付；PDF 萃取、歷史引用與換版影響留 #48。不能把部分輸入宣稱完整現行 CR。手工核對可確認選定區塊，但只有手寫 text_hash／locator 不能替代可重播的萃取；未支援 recipe 或無法重驗逐字結果時，該資料不放行。

## 4. 有限的 Standard 規則 ref

construction_rules_ref 不再是可任填的 URL／腳本／算法名稱。此版恰為 `sve-standard-v1:sha256:<64 lowercase hex>`，hex 取 **公開 descriptor** 的 H；descriptor 的欄位、排序與 canonical recipe 固定。相同 ref 只可有完全相同 descriptor；語義、值、地區／CR 引用變更都產新 ref，舊 ref 永久保留，不以新內容重綁。

| descriptor 欄位 | 完整型別／有限語意 |
| --- | --- |
| `schema,region,format_code` | 固定 sve-standard-v1、jp/en、固定 standard |
| `cr_version_id,required_clause_ids` | 真實同區 CR ID、§3 的完整有限閉包 |
| `deck_bounds` | `{main:{min,max},evolve:{min,max},leader:{min,max}}`，UInt、min≤max；不知道任一值則不能產完整 ref |
| `copy_counting` | `{default_limit,scopes,name_roles,physical_unit}`；UInt 正上限；scopes 固定 `[main,evolve]`，name_roles 是 primary/collab/treated_as 的非空排序子集，physical_unit 固定 card_once |
| `basis_modes` | class/title 的非空排序子集；可用模式必有依據，不表示可同時混用 |
| `neutral_class_codes` | 已採納的 class code 排序集合；不猜名詞／不從 JP 借 code 對應 |
| `exceptions` | 有限 `{kind,subject_code,allowed_class_codes,allowed_title_codes,clause_ids}` 排序集合；kind 只允許 class_basis／title_basis；subject_code 為已採納基準 class/title，兩個 allowed 集合按對應 kind 恰一非空，clause_ids 為必要閉包子集。未可表達的條件不得簡化成此例外 |
| `rule_evidence` | `{field,clause_ids,source_urls}` 排序集合，field 恰為 deck_bounds.main、deck_bounds.evolve、deck_bounds.leader、copy_counting、basis_modes、neutral_class_codes、exceptions；逐個欄位恰一項，clause_ids 為必要閉包子集，source_urls 為本採納 evidence 中官方補充規則頁的排序集合，兩者不可同為空 |

bounds、copy limit、基準與例外的**真實值**要逐區逐版核對，本文不採納研究樣本數值。name_roles 指向已採納 face_rules_name；受影響名稱更正後重驗，不由 fuzzy 名稱、同效果、EN 後綴配群。card_once 只定實物計數單位，不表示各種雙面算法已寫好。尚未知／不可表達的例外不得塞成空集合聲稱「不存在」；若必要一般條件不符合本版固定語義，改契約後才產 ref，不放 caller 自訂 predicate、regex、DSL 或 eval。

ref descriptor 保存在已釘建置 configuration 的 construction_refs（恰為排序的 `{ref,descriptor}` 陣列），待機器契約實作後公開投影到 `config.construction_refs:[{ref,descriptor}]`，按 ref 排序。所含 CR／clause／code 引用全在同快照可達，不出 raw locator、source hash、決定或私有配方。建置端另由本採納封套與 F1 原始 evidence 追回內容 hash／來源；不能只信公開 descriptor 自己宣告的 hash。profile revision 引用其中一個 ref，default_copy_limit 必等於其 default_limit，region／format／cr_version_id 亦一致，缺定義或不一致不得 ready。

資料 reader 只需能解本版的有限結構，可顯示 profile／數值／禁限與來源；算法 evaluator 的 supported_refs 另須確認確有實作。現有 resolver 接受 exact ref 集合，本契約不改此 API。未來 evaluator 可以先驗 descriptor 的版本、hash、引用與所有有限語義／例外，再為真正支援的內容產生 exact refs 集合；因此單純 CR 版號改變但規則語義仍完整受支援，不必只為列舉新 hash 發版。這是後續 evaluator 的驗證能力，不能只認 sve-standard-v1 字首就自動支援任意值／例外；未知語義仍 unknown。資料可查不代表算法已支援，不得將每個取得的 ref 自動塞進 supported_refs。

## 5. 來源集合、入口漏公告與 coverage

coverage.value.source_set 恰為 `{baseline,notices,discovery,excluded}`：前三欄是排序去重的 source_version_id 陣列，baseline 為完整現行限制基線或可重建起點，notices 為所有適用變更公告，discovery 為取得它們的入口／新聞索引；excluded 是 `{source_version_id,reason}` 的排序集合，reason 僅 unrelated_format／unrelated_topic／superseded／historical_same_url。四組互斥；每項有 evidence、sealed pin 與核對理由，不刪 raw 或隱藏尚未判定的來源。

**來源全集由程式獨立重建**：從 review_context 指定全部 sealed 批次的 inventory／descriptor 取得同區 provider、kind=limit/news 的全部來源版本，再加入本 coverage 實際使用的其他來源；不能由 caller 自選「相關公告」作分母。全集必恰屬四組之一；缺列的 ID 自動進 unresolved_source_ids 並使 complete 失敗，多列不存在的 ID／重複分組則輸入錯誤。已封存但入口未連結的公告同樣在分母內。來源全集完整只證明已取原檔都被分類，不能證明網路上沒有漏取，還須以下探索範圍與 href 閉包。

reconciliation 恰有 `{knowledge_cutoff,region_date_zone,baseline_through,discovery_ranges,discovered_urls,missing_urls,unresolved_source_ids,decisions}`：

- knowledge_cutoff 為核對知識截至的 UTC Instant，不用 snapshot／人審時間偽造更新；不得晚於本次 coverage 使用的 baseline、discovery 成功觀測時刻的**最早值**，尤其不能晚於最早 discovery 的成功觀測。一次性 index 用實際 fetched_at；後續重核若沿用同一 raw，須釘本批選定且可驗的成功 200／可信 304 觀測收據與 last_checked_at，不能拿 URL 首次抓取、descriptor.first_receipt 的舊內容時間、歸檔 observed_at 或無內容驗證的檢查時間補值。歷史 notices 的取得時間不拿來延長知識期限，也不迫使重抓未變動的歷史正文。
- region_date_zone 為有官方來源或明示核可依據的 IANA zone。官方完整生效日保留該公告所屬日期語意，JP 以 Asia/Tokyo；EN 公告日期時區仍待維護者確認，未確定前 EN coverage 不標 complete；須有來源或明示核可依據，不猜整個地區等於某個美洲時區。跨來源時區不一致須明示轉換；不明則 partial／unknown，不能自行套 UTC 當官方日期。
- baseline_through 是**基線已反映的最後一則公告之發布時間**的 UTC Instant，須由 baseline／其相關公告與該區時區驗回，與 discovery_ranges 同用公告發布時間軸，不用生效日或頁面最後更新日替代。只有日精度時用該日開始，探索須完整包含該日全部條目，不補假時間；發布時間或反映關係未知為 null，不得 complete。基線完整性須涵蓋此起點之前全部已發布公告，包括較晚才生效者；未證明已反映的公告須把探索起點退到可證明的發布範圍，否則 partial。較早發布而較晚生效的公告亦須逐項核對，不能因尚未生效或入口漏列而越過探索起點。
- discovery_ranges 是排序唯一 `{source_version_id,from_instant,until_instant,locator,complete_listing}` 陣列，closed interval、from≤until≤該來源依上述規則驗回的成功觀測時刻，每筆必對 discovery 來源。locator 釘 parser 的範圍／列表投影與邊界證據，complete_listing 是已依 §1 核對確認列表在此範圍沒有省略頁／同日條目，不是 parser 看到最舊標題就自動 true。頁面分頁鏈、重複邊界與日期精度須驗回；只知月份、同日界線不完整或標題不足判適用者，不宣稱範圍完整。
- discovered_urls 是 parser 從所有 discovery 結果獨立重建的原樣公告 href（沿既有 URL canonicalizer 後去重）及 historical_same_url 歷史列表投影的完整分類，恰為 `{url,source_version_id,state,reason,locator}`。state=required/excluded/unresolved；locator 釘原列表項目。列表上**全部 href** 都須分類，包括不是 `/news/post-N` 的其他網址，不把網址形狀當公告適用性或完整分母。required 必解析到已封存且納入 source_set 的同區公告，否則 URL 恰列 missing_urls；unresolved 一律保留缺口。無關賽制／主題可由兩模型依官方標題與明示範圍核對後 excluded，無法從標題確定的公告須列 required／unresolved，不猜內文。沒有原檔者 source_version_id=null；缺 href 的入口不憑空造網址，也不能當公告探索完整證據。
- missing_urls／unresolved_source_ids 是排序唯一的完整缺口，須包含上述機械重建結果，不接受 caller 清空以湊 complete。decisions 是 `{source_version_id,status,restriction_ids,superseded_by,historical_replacement,reason}` 的排序集合，status=baseline/applied/no_change/conflict/discovery/excluded，恰覆蓋四組全集；所有值與對應限制／期間／evidence 可重播。reason 一律為非空人工說明文字，不放結構化 JSON 字串；historical_replacement 只在 historical_same_url 的 excluded 項非 null，其餘一律 null。除 superseded 的 excluded 項外 superseded_by=[]。

historical_same_url 僅用於同區同 kind、canonical requested URL 完全相同的 limit／news **入口或列表頁**歷史版本；不得用於個別公告，亦不得因日期晚就判語意已取代。該 excluded 項須在 reconciliation.decisions 對應項的 historical_replacement 記結構物件 `{replacement_source_version_id,locator}`，指本次 baseline／discovery 中已核對的新版本與歷史版本投影差異定位；其 restriction_ids／superseded_by 為 []，不硬填禁限 ID。loader 驗兩版 frozen descriptor、URL／用途與該定位，並驗歷史版出現的全部 href 仍在 discovered_urls 分類，舊基線的限制／解除／更正／適用期間仍完整反映於有效 baseline／notices。新頁若漏了舊項、差異尚未核對或原版本含未解限制，不得如此排除，須 conflict／unresolved 或按下述 superseded 驗替代限制。所有歷史版本留來源全集、evidence、F1 及 seal，不刪原檔；每次新包重核可新增版本而不縮小分母。

被 superseded 排除的來源必有 reconciliation 決定：superseded_by 非空且每筆為 `{source_version_id,restriction_ids,locator}`，指有效 baseline 或已 applied 的替代公告，核對原限制及**全部解除／更正／生效期間**確已反映。僅版號新、日期晚或內容看似重複不足；找不到替代證據就 conflict／unresolved，不得排除。無關賽制／主題亦要核對分類，不能把「未讀」當 unrelated。

complete 必有完整兩模型核對及已解分歧、有效 baseline、四組分割與 href 分類全過、所有解除／更正統整、無 missing／unresolved／conflict。把 complete_listing=true 的 discovery 範圍按起點排序合併，必無縫涵蓋 `[baseline_through,knowledge_cutoff]`；範圍不銜接、必要後續分頁未取或公告日期邊界無法證明時為 partial。baseline_through 晚於 cutoff 或無範圍證據亦拒絕 complete；不以人寫「閉包完整」取代上述計算。原檔 hash 正確與來源數量只證明取得完整性，不能把數個正例推成「其他全部沒有」。

as_of 是已核對且來源能支撐的日期，保守上限為 min(knowledge_cutoff 的 UTC 日、knowledge_cutoff 在 region_date_zone 的當地日)，且不早於 from_date；不得用公告在 UTC 之後的當地日期擴大範圍。即時查詢能力若尚未實作，不能宣稱有時間粒度保證；後續支援時另不得超過精確 cutoff；日期型查詢只聲稱該日截止已核對的資料，不保證抓取後同一天也沒有新公告，介面須顯示 cutoff。until=null 只表示沒有已知結束日，讀取最多到 min(coverage.as_of,snapshot.as_of)，超出知識日期一律 unknown；每逢新卡包重核的排程不會自動延長保證。

來源缺口按通則判斷：入口落後於公告時，公告須納入 source_set／reconciliation，入口不作優先真值；無個別公告 href 時，可以經核對的完整入口作 baseline，但仍須另證適用期間的探索閉包。只有新聞第一頁而其最舊日期晚於基線時，範圍銜接失敗就是 partial；必要補抓須另有當次授權。已取得多頁且時間能銜接，也只具備核對條件，不能僅依標題判讀或頁數自動 complete。

complete 只支持可證明且已採納的窄期間，不代表完整歷史。更早／未證明期間保留 partial 或無 coverage；未查其他賽制不推定沒有禁限。profile 有資料而 coverage partial 時仍可查已知限制與缺口，不能回傳「無禁限／合法」。資料只保證到最近核對且來源所支撐日期，之後的日期標「未確認」。

## 6. Freshness、建置追溯與 unknown

dependencies 是排序唯一的 `{kind,id,region,basis_hash}` 集合，kind 恰為 profile／ref／cr／rules_name／card／vocabulary。profile basis 為 `{id,region,format_code}`；ref 為 H(完整 descriptor)；cr 為 `{id,region,version,source_version_id,clauses:[{id,context_key,number,text_hash}]}`；rules_name 為 `{id,region,official_name,faces:[{face_id,role}]}` 的穩定排序投影；card 為 `{id,region,layout,faces:[{id,ordinal,side,type_code}]}` 的穩定排序投影；vocabulary 為 `{kind,code,regional_raw_bindings}` 的 exact 已採納投影。basis_hash=H(該 projection)，region 必為本筆區域；vocabulary.id 為 `[kind,code]` 的 canonical JSON 字串，regional_raw_bindings 恰為該區已採納 `{lang,raw}` 的排序集合，沒有虛構 vocabulary.id 欄。值涉及的全部依賴恰須列出，不能只綁少數對象。舊核對背景可重播與當前適用性分開，不因無關翻譯／抓取時間換 hash 就要求重簽。

每次建置驗封套、精確決定／members、原始來源／recipe 與本次相關資料。#39 的 current／來源更正若改變同名對象、面／類型、角色、職業／作品或引用規則內容，列 stale 並停用該採納，不能只沿舊 identity decision 或原卡號套用。來源完好但本次來源基線／適用公告／CR／必要依賴改變時，重驗並追加核對；不得退用較舊採納假稱有效。來源/hash/結構損壞為整次交易失敗，stale／未知資料仍可按 preview 明示缺口，正式不得標其已支援。

匯入每片建立 authored source_record（exact bytes hash、revision/path、parser_version=construction-adoption-v1），decision_source 指完整封套及**全部**官方 evidence。內容來源的 source_id 保持真正 raw source_record，parser_version=null；不把人工決定偽裝成官網來源。CR、profile／revision、restriction、coverage 由釘住 subject 記錄與 F1 完整輸入追回採納；restriction／role 的 decision_id 指本類核對，不能只驗 confirmed 布林。建置釘 construction_adoption 的 index／revision／分片 bytes 與完整 staging／ref definitions，不以未知本機設定補值。

F1 收集上列各用途和逐條 cr_clause 用途，expected 集合由完整釘住採納／來源集合獨立重建，再與實際讀取用途及 DB raw 來源集合逐項比對。所有官方來源共用其 source_version_id，不為不同用途造 raw 列。缺／多用途、錯 recipe／locator／batch／region／descriptor／receipt、損壞來源或缺引用均在 caller 的整筆 transaction 內回滾；空 members 或任意條文不能繞過。

| 情況 | 可查資料／能力回應 |
| --- | --- |
| 未有該區 Standard profile／revision、日期區間外／重疊或超過任一 as_of | inputs_state=unknown，保留原因，不猜其他區域或格式 |
| 未封存、CR 缺必要集合、ref 缺定義／值不一致、default_copy_limit 未知、stale、partial coverage／未解衝突／有效但尚未確認限制 | inputs_state=unknown；已知公告可帶狀態查閱，不當完整可用輸入 |
| 資料來源與採納完整、ref 有有限定義，但 evaluator 未支援 ref／必要例外／角色計數 | 可查資料與固定 ref；resolver 的算法輸入仍 unknown，legality=unknown |
| 資料完整且該 evaluator 真正支援 exact ref | 只可稱 inputs_state=ready；本 API 未檢查牌組，legality 仍 unknown |
| 其他已知賽制 | unknown；不因未查到限制而推合法 |

Decklog 可用性與 profile／禁限正交，沿 regional_decklog 的版次目錄預設與查證。規則資料／空禁限不會變更 Decklog available，也不造查證日期。擬新增的 profile／限制公開 source_urls 由其 frozen raw evidence 投影；覆蓋的 as_of／state 與 CR 引用一併可查，不出完整決定、原始來源 store 或私有配方。

## 7. 邏輯擴充、後續單位與反例驗收

**待實作欄位方案只留本文件，不改現有權威表格／公開欄序**：cr_clause 擬新增 context_key 與其唯一鍵／公開欄位；restriction_coverage 擬新增 as_of／decision_id；config 擬新增 construction_refs；profile revision／restriction 擬新增公開 source_urls。不新增建置資料庫的表，不改已有原始來源 ID recipe（隔離 manifest 的新收據表見 §2）。正式欄位改動須與程式表定義在同一 carddb PR 更新 build-db 權威列，並同步 snapshot-format／snapshot-transport／Schema／types／golden／producer／reader，禁止 docs-only 先改欄位造成 CI 對照失敗。公開 tuple 擬在末尾追加 context_key／source_urls，不中間插欄；新增 config 與 coverage 欄位的機器形狀也須整套審核。格式未首發時依候選流程；已發布後 tuple 形狀改動升 major、設最低 reader 與 construction-ref-v1 required capability，舊 reader 只選 current／previous 或本機已驗 active 的相容快照，否則提示更新，不猜新欄。維護者真實規則數值／適用期間／逐筆採納不在這個 PR。

實作依元件分 PR，次序與進度留 issue：carddb 的入口／隔離來源登錄／交易／staging 接線是一個單位；authored 真實結構值、source pins 與兩模型／維護者決定另作單位；離線建置／公開投影另作 carddb 單位。來源型別、schema 版本與 reader 接線必在啟用前完成，不以 docs 合併當能力已實作。必要的 sim/web reader／整副牌 evaluator 分開，不在本入口偷偷補算法。

本件的受版控 parser 由後續 carddb 來源投影 PR 實作通用 HTML 區塊→exact 文字／欄位／日期／href 與探索範圍投影、可重播 locator 與結果 hash；語意主張由兩模型對照原公告採納，不是完整事件 adapter。重用既有 HTML 依賴，這輪不加 PDF 套件。PDF 頁／條文區塊、context 分辨、引用圖與必要 Standard 閉包均留 #48 的 carddb 萃取單位；如需 PDF 套件，先另開只改依賴的小 PR，再做萃取 PR。#48 須沿 §3 的 CR ID／UQ／recipe 與本來源 pin，不能重配既有實體。#49 做完整禁限語意事件與歷史 adapter，重用相同 frozen 投影與 ID，不把 #40 人工首批分類當完整歷史。#40 可在 ref／CR 為 null、能力 unknown 的狀態先交付已核對禁限，故不以 PDF 萃取阻擋 #40，也不形成 #40／#48 循環依賴；固定 ref 仍是未兌現交付，需 #48 接入後另驗。

反例須覆蓋：缺首輪真人抽查／模型名冒 reviewed_by／政策 sample_ids 少一成員／首輪未實際核對卻偽填 sample_ids 全體 checked／禁限或角色 human sampled 被放行／缺政策 index 或三檔／政策 pin 或 decision.policy_id 不一致／未實作 loader 卻放行／同廠商兩型號／報告缺檔或 hash、vendor、最終 basis 不符／缺證據備份還原；非構築 category／錯全成員／兩次同模型或不同最終 basis／未解分歧／冒充真人抽查／少 source use；隔離登錄碰 live、原 hash／時間／redirect／media 不符、少一 raw、重複收據衝突、seal／backup／restore 缺一步；CR 空／少一子節／錯上下文／跨區／同官版換 raw；ref 任意 URL／未定義／內容 hash 不符／少依據／未知例外／與 profile 不合；限制空 members／選項缺成員／混區／兩種 max 同填／日期重疊；JP 新公告不在入口卻被丟掉、EN 沒鏈接被當無限制、只讀 news 首頁便 complete、解除未統整、來源全集少列／探索範圍有洞／cutoff 超過觀測／EN 時區未定卻 complete／較早發布晚生效公告被跳過／列表非 post href 被漏分類／historical_same_url 跨 URL 或丟舊 href／superseded 無替代證據／成員同名 scope 重複／as_of 之外；名稱／型別更正後照套舊決定；以及「資料 ready 等於合法」的錯誤。全部用合成資料／MockTransport／localhost，不依賴正式原檔，不把一次性全量比對放進每次測試。

目前程式的 decision-backed 匯入仍 fail-closed；本契約合併不會自動放行、不授權本 worker 執行來源登錄／抓取／正式建置。沒有實作／來源／採納就如實回 unknown。
