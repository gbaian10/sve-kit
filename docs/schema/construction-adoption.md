# 構築規則與禁限採納：construction-adoption-v1

本文件為 [#40](https://github.com/gbaian10/sve-kit/issues/40) PR A 的待審技術契約，細化 [build-db §6／§7](build-db.md#7-禁限與構築)。維護者 2026-10-03 決定首批只做 **JP Standard、EN Standard**；交付可查、可追溯的 profile／禁限資料與固定 ref。其他賽制保持 unknown；整副牌合法性檢查留後續建牌器。契約、來源封存、逐筆採納、資料能力與算法能力是分開的門檻。

本文不提供來源抓取指令、不採納規則數值／公告內容。官方 HTML／PDF、CR 全文與轉錄結果留 repo 外；研究樣本不是正式證據，不能複製進 authored 或用它們的 hash 代替正式原檔。已獲同意抓回的原檔仍須完成 §2 登錄／封存，才能供正式採納。

## 1. 專用入口與採納封套

| 路徑（相對 authored） | 完整頂層欄位 |
| --- | --- |
| `construction-adoptions/index.yaml` | `construction_adoption_format:1, kind:construction_adoption_index, includes` |
| `construction-adoptions/<area>/<region>/standard/<sequence>.yaml` | `construction_adoption_format:1, kind:construction_adoption_shard, review_context, default_decision_id, records, decisions` |

area 恰為 profiles／revisions／refs／restrictions／coverage／roles／cr；region 恰為 jp/en。sequence 按 area／region 從 001 起連續只增，至少三位十進位；一片非空、單一 kind／核對背景／決定。首批格式拒絕其他 format_code，不把 Crossover、Cross Craft、Gloryfinder 併成 Standard 或填合法。擴大 scope 須改契約與驗證，再採納真實資料。

共用 [catalog 採納 §2–§2.1](catalog-route-adoption.md#2-入口分片與封套) 的嚴格 YAML 1.2、單檔 <1 MiB／512 KiB 目標、canonical H、index 與安全路徑、review_context 及完整 decision 欄位。includes 值為分片解析後的完整 canonical hash；本次建置另釘完整 immutable authored revision、index／所有歷史分片的 exact bytes hash。啟用入口缺 index 拒絕，空集合須明示 includes={}；未知欄位／格式、重複鍵、symlink、跨入口引用、未索引檔、缺檔或 hash 不符皆拒絕。先驗全部地區與歷史再投影，不能縮小決定成員。

record 恰為 `{record_key,kind,filing_key,data,evidence}`；filing_key 固定 `region:standard`。data 恰有 `{subject,adoption_no,predecessor,value,review_context_hash,dependencies,reason}`，subject 依下表；record_key 為 `[kind,subject,adoption_no]` 的 canonical JSON 字串。每個 subject 的 adoption_no 從 1 起連續增加；predecessor 首筆 null，後筆 `{record_key,record_hash,decision_id}` 必指前一採納。value 完整替換，不作 patch；續版 null 明示撤回。有效值先按全歷史續版解出，再匯入一份新的建置 DB，不把前版主鍵內容直接覆寫；相同列僅可逐欄 exact 重用。不能原地修改歷史／重用原決定，同一主體不得分成多條互相搶值的鏈。

| area／kind | subject（完整欄位） | value 範圍 | category／policy_id |
| --- | --- | --- | --- |
| profiles／construction_profile | `{profile_id}` | `{region,format_code,name}`，name 為人工介面名稱 `{lang,text}` | construction_profile／construction-profile-v1 |
| revisions／construction_revision | `{revision_id}` | `{profile_id,effective_from,effective_until,cr_version_id,default_copy_limit,construction_rules_ref}` | construction_revision／construction-revision-v1 |
| refs／construction_ref | `{ref}` | §4 的有限 descriptor；ref 不可重綁不同內容 | construction_ref／construction-ref-v1 |
| restrictions／construction_restriction | `{restriction_id}` | `{profile_id,announced_on,effective_from,effective_until,kind,state,max_copies,max_selected_groups,members}` | construction_restriction／construction-restriction-v1 |
| coverage／construction_coverage | `{profile_id,from_date}` | `{until_date,as_of,state,source_set,reconciliation}`，見 §5 | construction_coverage／construction-coverage-v1 |
| roles／construction_role | `{card_id,region}` | `{role,basis}`，basis 是 H(§6 card 的全部實體面 typed projection)，不得用一面代表整卡 | construction_role／construction-role-v1 |
| cr／construction_cr | `{cr_version_id}` | `{region,version,published_on,effective_on,source_version_id,clauses}`，見 §3 | construction_cr／construction-cr-v1 |

record_hash=H(完整 record)，members 恰為整片 `[record_key,record_hash]` 的排序唯一集合，membership_hash=H(members)，decision.id=`d:`＋完整 64 hex。sample_ids 恰列全部 checked 成員；少一、多一、重複、抽樣代簽均拒絕，一筆也是 batch。決定必為 confirmed／batch，本版 reviewed_by 恰為維護者 gbaian10，真實時間及 day／instant 精度沿共用封套；製作者／確認者分開。不接受 identity／catalog／其他區域／前版決定代簽，不借用 approved_rules／approved_policy，不設略過來源／freshness 的旗標。

每筆 profile／CR／ref 的 region、所有引用 profile 與依賴地區必等於路徑 region，format_code 必為 standard；跨區／跨格式釘錯直接拒絕，不靠 URL／官版號猜。profile 的 name 只供介面，不是官方規則引文。default_copy_limit 是同名一般上限，不能把主牌上限塞進此欄。日期均為來源明示的完整生效日，半開 `[from,until)`，未知不以抓取日、公告日或第一天補值。revisions 同 profile 不重疊；coverage 同 profile 不重疊，相鄰可接。修正舊期間須續版原主體；新的生效期間使用新的 revision_id／coverage subject，舊資料與採納收據保留。

restriction.state 的 confirmed／announced／withdrawn 是限制的生命周期，與封套 confirmed 人工核對正交：可核對「尚未實施」或「已撤回」，不能因此視為 active。copy_limit 恰填 max_copies（0 表示禁止），choice_group 恰填 max_selected_groups；另一欄 null。members 是非空、去重的 `{rules_name_id,choice_option,deck_scope}` 陣列，同 profile 地區，deck_scope=main/evolve/all。copy_limit 的 choice_option 固定 0；choice_group 的 option 使用連續 0 起編號，每組至少一成員，max_selected_groups 為正整數且小於選項數。選中一組指該 deck_scope 內任成員數量>0，限制的是選中幾組，不是所有張數之和。不同 printing 先按 card／構築名稱合計，雙面實物只算一次；不同卡名的同效果卡不自動合計。

## 2. 一次性抓回原檔如何正式登錄與釘版

此節定義後續 carddb 的**離線登錄能力**，不是宣稱已有 CLI，也不是本 docs 單位的執行授權。輸入是維護者認可的一次性來源集合：index 記 URL／final URL／status／sha256／bytes／fetched_at／content_type／ETag／Last-Modified／chain，raw 以內容 hash 保存。原始 index 與 raw 保留不動，登錄不會對任何外部服務發請求，不開 live manifest 或以它補 metadata。

1. 唯讀驗完整 index 與全部 raw：唯一 canonical requested URL、200 成功狀態、原始 URL／最終 URL／chain 一致、HTTPS／官方 host、有效 UTC 時間、media type、exact raw hash／bytes。禁止 symlink 越界、未知欄位、缺檔、重複衝突、未結束鏈及不符合用途的 HTML／PDF。URL 只沿既有 canonicalizer，不推測 PDF 版號或用來源路徑配 region。把來源清單、原 index exact hash、原 metadata 與本次用途分類釘在獨立登錄收據；它留來源歸檔 metadata 閉包，不進 git。
2. 建立**全新的隔離 manifest／staging**，不複製或接管 live manifest。provider 依核對後來源用途明示 jp/en（EN PDF 即使由共享官方 host 供應仍是 en）；抓取 kind 使用現有 rules（規則入口／CR PDF）、limit（禁限入口）、news（新聞索引／公告），不把 PDF 虛構成 card。Resource.url 取 canonical requested URL；final_url／chain 保留登錄收據，不能把兩者互換或默默合併別的 URL 身分。
3. Resource 的 hash／raw_bytes／content_type／標頭沿原紀錄；path 為隔離目錄中的安全相對 raw path。first_fetched_at／last_checked_at／last_changed_at 使用唯一這次已知 fetched_at，不冒稱官方首次發布或更早抓取，archived_at 在封存前為 null。raw 不轉碼、不正規化。只登錄既有來源觀測，不造未實際發生的 HTTP fetch_log、重試、歷史或爬取 generation。重複匯入同收據可重用，衝突停止，不覆寫。
4. 使用 [source-archive §2／§3](source-archive.md#2-內容來源版本與-inventory) 的既有 raw blob／source_key／source_version_id／descriptor／receipt／inventory／seal 格式，來源版本身份仍是 `{provider,kind,url}`＋raw hash，不能新增另一套「研究來源」ID。inventory 必帶隔離 manifest 的 SQLite backup API 自足副本，其 metadata 閉包含上述登錄收據。準備／重驗／fsync／最後原子發布 seal；沒有 seal 或任一 missing 不能當正式批次。
5. **備份並 restore-check 通過**才交付正式 batch pin／原 index hash／登錄收據 hash／用途分類／數量與 bytes 對帳給協調者。異常停止，原輸入與失敗收據保留，不以重抓／crawl／refresh 修補、不清理其他任務狀態。真實執行由協調者在工具審核／合併且確認當次範圍後操作；本輪只定契約。

隔離 manifest 新增 `source_import_receipt` 登錄紀錄，與 fetch_log 分開：保存原 index exact bytes（BLOB）、index_sha256、canonical `{source_mappings,program_revision,dependencies}`、該內容的 receipt_id 與 registered_at；此收據不是 archive observation receipt，不能代替 descriptor.first_receipt_id。source_mappings 恰列每個 canonical URL／provider／kind／raw hash／安全相對 path，原 final_url／chain／抓取 metadata 由 index bytes 保留。receipt_id 對 index_sha256 與 canonical 登錄內容計 H，不以私人路徑或時間配號；相同收據重跑保留原 registered_at、逐欄比對，不覆寫。此表由隔離 manifest 的 SQLite backup hash 一起釘住，故可驗原 index 與 raw 的登錄關係，不向 inventory／Resource 塞未知欄位。後續 carddb 必提供相應 manifest schema／reader 驗證；只有舊 seal 工具或多放一個未被 hash 引用的旁檔不算完成此邊界。

被核可的新抓取原檔與舊研究樣本是兩類輸入；不能把「本機有 PDF」當可採納。相同官方 CR 版號、不同 raw hash 是不同 source_version／cr_version_id，歷史保留；同一內容不同 URL 亦依既有 source_key 分開。既有 manifest 格式或 reader 不支援這種登錄收據時直接停止，不能手改資料庫或略過驗證。抓取時間不代替 published_on／effective_on。

一次性取得的來源集合可含兩區 CR PDF、規則／禁限入口、歷次公告、入口漏列的新公告及 news 索引；集合數量與版本標記屬登錄收據，不代表已封存或可採納。明示未取新聞後續分頁／其他賽制不算抓取失敗，但必須按 §5 限制 coverage。登錄成功也不自動核可其規則含義。

## 3. 原始來源、CR 實體與必要引用閉包

evidence 是排序去重的 `{role,use}` 陣列；use 恰為既有 `SourceUse {source,usage,locator}`，source.kind 只允許 official_page／official_api／official_pdf，source.archive 帶 store_id／batch_id／descriptor_sha256／first_receipt_id。source.id 是真實 source_version_id，raw hash 是原 bytes；每片 review_context.source_batches 必涵蓋全部用到的 sealed 批次。禁止第三方 URL-only、研究 hash、未封存檔、本機絕對路徑或 caller 自稱 verified。角色 role 僅允許 profile／rule／restriction／coverage／role／cr／clause／reconciliation，須與記錄實際值相關。

usage 為 construction_profile／construction_revision／construction_ref／construction_restriction／construction_coverage／construction_role／construction_cr／cr_clause；各用途釘非空實際 parser pin。locator 是 canonical JSON 的 `{record_key,field,context_key,number,location,result_hash}`，可空欄明示 null；location 恰為 `{kind:projection_pointer,pointer,page_numbers}`，pointer 是在該受版控 parser 結果中的 JSON Pointer，PDF page_numbers 非空、排序唯一且從 1 起，HTML 則為 []。result_hash 對該 pointer 的 typed 結果計 H；逐字條文取 UTF-8 exact text hash，與 raw hash 分開。實際重播必驗 pointer 內容、格式上下文、原檔頁面／區塊及全部相關 fields，不能只驗 hash 字串看似有效。

construction_cr 的 clauses 恰為非空、排序的 `{id,number,context_key,locator,text_hash,referenced_clause_ids}` 集合；不含 text。number 是官版原樣字串，不當小數；context_key 為受版控萃取 recipe 產生的格式／章節上下文識別，首次 Standard 根節使用 standard，補充／例外須各有明示 context。實際文字由 frozen 原檔以釘住 recipe 萃取後建 text_unit／cr_clause，不在 authored 轉錄原文。同區、官版標記、來源版本與完整區塊必精確對上；單有 version label／locator／text_hash 或任一條文不算採納完成。

CR 的重複條號不跨上下文覆寫：後續 cr_clause 欄位方案新增 context_key，唯一鍵擬為 `(cr_version_id,context_key,number)`；機器契約實作後公開亦保留 context_key。同上下文重複而無可驗定位的結果拒絕。JP／EN 各用自己的原檔／版本／條文；不能用 JP CR、舊研究版或一段同號附加規則補 EN Standard 的缺口。

每個 ref 的 required_clause_ids 必為有限、非空、排序去重集合，涵蓋以下最低 Standard 查核集合與所有實際依賴：

- Standard 構築根節及九個子節。調查列出的 `6.1.1` 與 `6.1.1.1` 至 `6.1.1.9` 是首批核對的十個標記；正式兩區資料須從**已封存的當區官版 Standard 上下文**確認完整集合，不能由研究版字串自動產生引用。若官版重編、缺節或適用範圍不同，先明示改版查核集合並審核，不任找十段湊數。
- 這些條文實際引用的名稱／卡種定義、雙面／特殊角色／職業／作品條件與適用例外。referenced_clause_ids 列適用引用，以已走訪 ID 集合求有限閉包；允許條文互引，但未解引用不能跳過。每個具體規則欄位都須能連到必要集合的逐字依據或另釘的官方補充規則頁；後者不能假造 CR 條號，單純列十個無關條號不合格。

required_clause_ids 中每個 ID 必在本次 DB／快照具有同 cr_version_id 的實體 cr_clause，且由該版本 frozen evidence 驗回。僅有一條任意 clause／懸空 CR 字串／缺 parent／缺一子節／錯 context／缺例外，一律回 unknown 並列缺口。#40 只承諾此必要閉包，不能把部分輸入宣稱完整現行 CR；完整 PDF 萃取、歷史引用與換版影響留 #48。手工核對可確認選定區塊，但只有手寫 text_hash／locator 不能替代可重播的萃取；未支援 recipe 或無法重驗逐字結果時，該資料不放行。

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

資料 reader 只需能解本版的有限結構，可顯示 profile／數值／禁限與來源；算法 evaluator 的 supported_refs 另須確認確有實作。資料可查不代表算法已支援；不能為了讓 resolver 回 ready 而把每個取得的 ref 自動塞進 supported_refs。

## 5. 來源集合、入口漏公告與 coverage

coverage.value.source_set 恰為 `{baseline,notices,discovery,excluded}`：前三欄是排序去重的 source_version_id 陣列，baseline 為完整現行限制基線或可重建起點，notices 為所有適用變更公告，discovery 為取得它們的入口／新聞索引；excluded 是 `{source_version_id,reason}` 的排序集合，reason 僅 unrelated_format／unrelated_topic／superseded。每個來源均有 evidence、sealed pin 與核對理由；三組集合互斥、excluded 不與前三組重複；列 excluded 不可刪 raw 或隱藏尚未判定的公告。

reconciliation 恰有 `{knowledge_cutoff,missing_urls,unresolved_source_ids,decisions}`：cutoff 是所核對來源集合截至的 UTC Instant，不以 snapshot 發布時間偽造更新；missing_urls 是明示需要但未取得的公開 URL，unresolved_source_ids 是尚未解決的來源；decisions 為 `{source_version_id,status,restriction_ids,reason}` 的排序集合，status=baseline/applied/no_change/conflict，每個 baseline／notice 都必有一項。補登來源不自動採納，公告解除／更正亦必核對確切目標及期間；已撤回不是刪掉舊公告。

complete 必同時有維護者全量核對收據、有效 baseline、可證明的**適用公告集合閉包**、全部生效／解除／更正已統整、無 missing／unresolved／conflict；不得把數個禁限正例當「其他全部沒有」。as_of 是核對知識截至日，不能晚於 knowledge_cutoff 的 UTC 日，也不能早於 from_date；請求日不證明此前或此後所有來源。until=null 只是沒有已知結束日，讀取最多到 min(coverage.as_of,snapshot.as_of)，未來一律 unknown。complete 也不代表完整歷史；只能支持已核可、可證明的窄期間。

來源缺口的判準：

- **JP 入口落後公告**：入口內容不是優先真值；適用期間需將入口尚未列的 `post-554` 納入 source_set／reconciliation 與限制續版，按公告明示日期處理。封存入口加八則舊公告不構成完整當前限制；不能因新公告在頁外就忽略，也不能不看區域／Standard 適用性就機械套用。入口與公告衝突未解則 partial。
- **EN 入口無個別公告連結**：沒有 href 不等於沒有限制或已查完公告。入口可作 current baseline，前提是完整內容／生效日期經核對；`post-167` 另作適用公告，不把 JP 結果套入 EN。缺公告歷史時不宣稱歷史完整；若無法證明本次日期所需的完整基線／變更集合，就 partial，即使入口與一則新聞都取得 200。
- **只取得 news 第一頁**：必須核對其可證明的時間範圍與較早基線的銜接；未取得後續分頁、其他被引用必要公告或無法證明沒有漏列時，缺口留 missing／unresolved、coverage=partial。整批原檔 hash 正確只證明取得完整性，不能單憑檔數／備份／封存把它轉為 complete。後續取得缺漏仍需當次授權。

如果能以正式的完整當前基線與變更核對證明首發窄日期，才可為該日期採 complete；更早／未證明期間保留 partial 或無 coverage。未查別的賽制不推定其沒有禁限。profile 有資料但 coverage partial 時仍可查已知限制與缺口，不能回傳「無禁限／合法」。

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

**待實作欄位方案只留本文件，不改現有權威表格／公開欄序**：cr_clause 擬新增 context_key 與其唯一鍵／公開欄位；restriction_coverage 擬新增 as_of／decision_id；config 擬新增 construction_refs；profile revision／restriction 擬新增公開 source_urls。不新增表，不改已有原始來源 ID recipe。正式欄位改動須與程式表定義在同一 carddb PR 更新 build-db 權威列，並同步 snapshot-format／snapshot-transport／Schema／types／golden／producer／reader，禁止 docs-only 先改欄位造成 CI 對照失敗。公開 tuple 擬在末尾追加 context_key／source_urls，不中間插欄；新增 config 與 coverage 欄位的機器形狀也須整套審核。格式未首發時依候選流程；已發布後 tuple 形狀改動升 major、設最低 reader 與 construction-ref-v1 required capability，舊 reader 留最近相容快照，不猜新欄。維護者真實規則數值／適用期間／逐筆採納不在這個 PR。

後續 B carddb 做 strict loader、隔離來源登錄、封存／重播、交易與既有 staging／resolver 接線；C authored 放維護者核對的結構值／source pins／決定，官方全文仍 repo 外；D carddb 接雙區離線建置與公開投影。必要的 sim/web reader／整副牌 evaluator 各自拆單位。#48 做完整 CR 萃取／版本更新，#49 做完整限制事件／歷史 adapter；不要求它們全部完成才能核對首發必要閉包，也不宣稱 #40 已完成它們。

反例須覆蓋：非構築 category／錯全成員／其他人名／少 source use；隔離登錄碰 live、原 hash／時間／redirect／media 不符、少一 raw、重複收據衝突、seal／backup／restore 缺一步；CR 空／少一子節／錯上下文／跨區／同官版換 raw；ref 任意 URL／未定義／內容 hash 不符／少依據／未知例外／與 profile 不合；限制空 members／選項缺成員／混區／兩種 max 同填／日期重疊；JP 新公告不在入口卻被丟掉、EN 沒鏈接被當無限制、只讀 news 首頁便 complete、解除未統整、as_of 之外；名稱／型別更正後照套舊決定；以及「資料 ready 等於合法」的錯誤。全部用合成資料／MockTransport／localhost，不依賴正式原檔，不把一次性全量比對放進每次測試。

目前程式的 decision-backed 匯入仍 fail-closed；本契約合併不會自動放行、不授權本 worker 執行來源登錄／抓取／正式建置。沒有實作／來源／採納就如實回 unknown。
