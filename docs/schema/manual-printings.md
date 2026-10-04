# 人工限量序號版次：manual-printings-v1

本契約為 [#130](https://github.com/gbaian10/sve-kit/issues/130) 的 docs 單位，細化 [authored-layout §9](authored-layout.md#9-批次表記上下文與非官方條目)。收錄政策依維護者 2026-10-03 **更正後**決定；封套與下列邏輯欄位擴充仍須審核與實作，不表示候選資料、機器 Schema 或 reader 已就緒。官方卡文、第三方頁面與原圖不進此入口。

## 1. 收錄、歸屬與資料權威

人工入口涵蓋周年 SNC（含卡號前綴為 BP20 的周年卡及 EN）、數位版活動實體卡，以及既有官方 PR 的序號補充。候選整理範圍為 63 種；範圍決定不等於逐筆採納。候選留 authored 外，信心 high 不代表 confirmed。只收 jp/en，不按 EN 後綴、卡號、角色或畫師配對。

依 issue 最新更正，所有 SNC 編號周年卡（日英、含 `BP20-SNC01`）歸 `SNC`；數位版活動三張歸 `WB`；既有官方 PR 留 `PR`，補序號不改 home_set。`home_set_id` 解析到已登錄 product_family，首次值與 filing_key 必一致，不直接切卡號前綴；filing_key 始終為首次歸檔 code，後續明示改歸屬不搬歷史分片；歸檔不等於商品收錄或發售證據。`ANV` 是卡面稀有度標記，不是歸檔家族，不建立 ANN family。稀有度原樣存 rarity_raw；穩定 rarity_code 只有已採納的同區 exact binding 才可填，不能從系列、樣張檔名或前一則留言猜。後续新類別或改歸屬須明示採納與續版，不擴張本次已決定範圍。

本格式有兩種記錄，互不代用：

- `manual_printing`：新增官方卡表未收錄的 printing；catalog_state 固定 unlisted。至少有一筆已封存官方頁／官方卡圖上印的號碼證據時 card_no_state 才可為 official；只有第三方店家佐證或推定號碼仍為 provisional；兩軸獨立。
- `serial_supplement`：為已存在的官方 printing 補 `serial_total`、公開序號註記與證據，不重新配 card／face／printing／int_id，不改卡號、歸屬、卡文或官方收錄狀態。PR-350／PR-442／PR-544 使用此路徑；PR-442 的官方卡文照常顯示，不受「未收錄且未確認對應者不顯示卡文」限制。

## 2. 入口、封套、決定與續版

| 路徑（相對 authored） | 完整頂層欄位 |
| --- | --- |
| `manual-printings/index.yaml` | `manual_printings_format:1, kind:manual_printings_index, includes` |
| `manual-printings/<area>/<filing_key>/<sequence>.yaml` | `manual_printings_format:1, kind:manual_printings_shard, review_context, default_decision_id, records, decisions` |

area 恰為 `printings` 或 `serials`，分別對應上述兩種 kind；filing_key 是首次明示歸檔 family code，不是卡號前綴，續版保留原 filing_key。sequence 從 001 起按 area／filing_key 連續只增，至少三位十進位。每片非空、單一 kind、單一核對背景與決定，records 按 record_key 排序。

共用 [catalog 採納 §2–§2.1](catalog-route-adoption.md#2-入口分片與封套) 的嚴格 YAML 1.2、單檔 <1 MiB／512 KiB 目標、路徑安全、review_context、完整 decision 欄位及 canonical H recipe。includes 值為完整分片解析後的 canonical hash；本次建置另釘 index／分片 **exact bytes hash** 與 immutable authored revision。拒絕未知欄位／格式、重複鍵、symlink、跨入口／絕對／`..` 路徑、缺檔、未索引分片或 hash 不符。啟用時缺 index 必須失敗；明示空集合才使用 includes={}。先驗全部地區、全部歷史，再投影，不能過濾掉決定的成員。

record 恰為 `{record_key,kind,filing_key,data,evidence}`；data 恰為 `{subject,adoption_no,predecessor,value,review_context_hash,dependencies,reason}`。subject 恰為 `{printing_id}`；record_key 是 `[kind,printing_id,adoption_no]` 的 canonical JSON 字串。adoption_no 同 subject 從 1 起連續增加；predecessor 首筆為 null，後筆為 `{record_key,record_hash,decision_id}`，必精確指向前一採納。value 是完整替換值，不是 patch；首筆非 null，後筆 null 表示撤回人工採納，但永久 ID／配號／已發布網址仍保留。

`record_hash=H(完整 record)`，members 恰為本片全部 `[record_key,record_hash]` 的排序集合，`membership_hash=H(members)`，`decision.id="d:"+membership_hash 的完整 64 hex`。sample_ids 恰為已全筆核對的全部成員 key；少一、多一、重複或只列代表項均拒絕。一筆也是 batch。

| kind／area | category | policy_id |
| --- | --- | --- |
| manual_printing／printings | manual_printing_adoption | manual-printings-v1 |
| serial_supplement／serials | serial_supplement_adoption | serial-supplement-v1 |

decision 必為 confirmed／batch，reviewed_by 本版恰為維護者 `gbaian10`，reviewed_at／reviewed_precision 沿共用 day／instant 編碼，製作者另記 authored_by／authored_at。核對包括完整值、號碼狀態、歸屬、來源、全部依賴及每一對應面。工具不得捏造核對事件；身分、商品或其他序號記錄的決定不能代簽。此入口採全筆人審，較 build-db §2 的一般序號 sampled 門檻嚴格；不借用 approved_rules／approved_policy 例外。confirmed 表示核可按所記的不確定程度公開，**不會把 provisional 號碼或未知對應變成已證實**。

## 3. 值、對應與永久 ID

manual_printing.value 恰有以下欄位；可空欄仍須明示 null。

| 欄位 | 型別與約束 |
| --- | --- |
| `initial_anchor` | 第一次配號的 exact `region:card_no`；續版不可改。依 authored-layout §3.1 的既有 UUIDv5 namespace 與 printing recipe 產初始 printing_id，已有 ID 永遠優先 |
| `identity` | `{card_id,layout,faces:[{face_id,ordinal,side}]}`；card_id 必填，single／double_faced 與完整固定面集合沿既有契約。未確認對應者使用獨立 provisional card／face；不造官方文字 |
| `region,card_no,card_no_state,catalog_state` | jp/en、非空原樣字串、official/provisional、固定 unlisted |
| `home_set_id,listing_confidence,variant_key` | 已登錄 family、high/medium/low、本版固定 standard；歸屬依 §1，不從號碼推導 |
| `rarity_raw,rarity_code` | 卡面原樣稀有度字串，未知為空；code 可空，有已採納 exact binding 才填。原值主張須有來源，不能把 ANV 作 family |
| `manual_name` | `{lang,text}`；lang 等於該區原語 ja/en、text 非空。這是人工顯示名稱，不是官方 name_unit 或譯文 |
| `counterpart` | null 或 `{printing_id,card_id,face_ids}`；指同區已登錄的一般版，face_ids 為其固定 ordinal 順序的完整面集合 |
| `serial_total,serial_note` | 正整數或 null；非空人工註記或 null。total 僅記卡面序號分母，未知不是 0 |
| `references,inclusions` | §4 的完整參考集合；inclusions 是 `{product_id,inclusion_kind,date_precision,date_raw,available_on,note}` 陣列，只有真實、已採納同區商品可填 |

`counterpart=null` 時使用獨立 identity_state=provisional 的 card／face，仍有非空 printing.card_id 與完整 printing_face；每個 face 的 layout／ordinal／side 依人工核對實體面登錄，不由進化角色或樣圖順位猜。官方文字未知時不造 face_revision／face_current／printing_face_observation／source_face_map；printed_name_unit_id／printed_effect_unit_id／flavor_unit_id 為 null，printed_text_state=unknown、observations=[]。這表示未知，不表示無能力；不把人工卡名當原文。圖像仍可由 #210 掛在精確 printing／face 的 printing_image，依獨立來源／逐圖核可，不需要虛構卡文才能掛圖。

人工入口首次建立的 provisional card／face 依 [authored-layout §3.1](authored-layout.md#31-id-與身分) 的同一 UUIDv5 namespace 與 kind／NUL recipe 配發：card anchor 使用此版次的首次 initial_anchor（exact `region:card_no`），face anchor 使用該 card ID 與固定 source ordinal。只在首次配發時產生；已有永久 ID 優先，補正卡號或後續確認對應不重算。

counterpart 非 null 時 identity 的 card／所有 face 必精確等於經逐張、全體面核對的一般版。同區可讀其有效 current，序號卡自身 printed_text_state 仍 unknown，不假裝有官方卡頁；沒有 current 時按表記未定規則。一般進化前後仍是不同 card，樣圖是哪一面須核對。已先建 provisional card 者，後續確認對應要走既有 identity-transition 完整面／插畫移轉與墓碑收據，不在續版裡默默改 card_id；printing ID／int_id 不變。後續 carddb PR 須將身分修復的查核與重播擴充至「官方 registry＋完整 manual-printings 歷史」的聯集，涵蓋人工入口建立的 card／face／printing 及其插畫關聯，與下述整數配號的 lookup 邊界一致；不能只查 registry 而漏掉人工 provisional card。此擴充須同步 identity-repair 的入口範圍與來源／前件釘版，未實作前不得宣稱人工條目已有身分修復能力。

引用另一區時不允許直接填 counterpart；日英同卡仍另走既有全體面身分採納。補正卡號或歸屬不重算 initial_anchor／printing_id／int_id；暫定路由直接依 [build-db §15](build-db.md#15-網址搜尋預設版次與記號) 的 `/cards/_provisional/{int_id}` 與 provisional_corrected 永久 alias 規則，禁止劫持現役官方鍵，不另定另一套入口。後續官方卡表出現此版次須採明示身分／路由修復並保留原 printing ID，不能以新的號碼 recipe 重建第二筆。

serial_supplement.value 恰為 `{region,serial_total,serial_note,references}`。目標 printing_id 必須已存在且為 official，同區 region 必一致；references 的 roles 僅允許 number／serial，distribution 必為 null；serial_total 可正整數或 null，撤回已知值必須以續版明示並留原因。只投影上述序號補充，不改已有內容／觀測；既有三張 PR 不複製為新人工版次。supplement 不新增 int_id。兩種 kind 不可共同搶同一 subject；已存在的版次禁止 manual_printing create。全域永久 ID、配號、reference 與 FK 衝突整次回滾。

int_id 不由本封套自填／自建游標：配號由既有 `carddb/src/sve_carddb/registry/allocation.py` 的地區政策、cursors／共用配號工具統一負責。後續 carddb PR 擴充 caller 的 printing 集合查核為「官方 registry＋完整 manual-printings 歷史」，兩邊配發結果都追加至同一 `authored/ids/` ledger／index，不另建人工配號簿。各區游標必等於全部已配（含撤回／墓碑）printing 的最大 int_id＋1，無舊值取該區起點；region 讀 printing 真值，不按號碼推區。新人工 entry、ids 分片與 index 須一起驗妥、原子提交，衝突／耗盡整批停止；重跑已配 printing 重用舊值。候選不預占正式號段。這不擴張身分 registry v1 的 official observation kind，ids 的既有格式不變，但其 printing lookup／驗證 caller 必認得兩個入口的完整聯集。

dependencies 為排序、去重的封閉陣列；元素恰為 `{kind,id,region,basis_hash}`。kind 只允許 family／product／printing／face／current：family.id 為 home_set_id、region=null；其餘 region 為本筆地區。product 指 inclusions 的商品，printing／face 指 counterpart 的完整版次與面，current.id 是對應 face_id 的同區已選 revision。basis_hash 為 H(相關 typed projection)：family 恰取 `{id,family_code}`；product 恰取 `{id,region,family_id,product_code}`；printing 恰取 `{id,card_id,region,face_ids}`（固定 ordinal 序）；face 恰取 `{id,card_id,ordinal,side}`；current 恰取 `{face_id,region,name_hash,rules_hash}`，name_hash 取該區 current 的 exact 名稱，rules_hash 使用釘住的 registry observation recipe 對種類／數值／特性／效果／sections 的投影，不含日期／來源 ID。不取不相關標籤、revision 配號或抓取時間。當時無 current 時不造該 dependency；確認同卡的依據仍必由 evidence 說明。依賴少列、多列、區域不合、目標不存在或 basis_hash 不符均拒絕；新 current 的相關內容若改變，要按 §5 重驗，而不是省略 dependency 避過。原已採納依賴的精確決定／成員與來源由 review_context 釘住可驗。

## 4. 來源類別、日期與取得方式

reference 恰為 `{source_class,url,roles,locator,source_ref,checked_on,distribution}`；url 為可公開 HTTPS 原樣 URL，無 credentials／私有 host／本機路徑。roles 是非空、排序去重的 identity/number/serial/distribution/image 集合；locator 為非空人工定位或 null。相同 URL／role 不重複。distribution 非 null 時 roles 必含 distribution。

| source_class | 留存與 source_ref |
| --- | --- |
| official_archived | 官方網域的頁面／PDF／圖片必有原始封存版本與 hash。source_ref 必釘 `{batch_id,source_version_id,descriptor_sha256,first_receipt_id,raw_hash,parser_version,locator,result_hash}`，store 名稱與 root 由本機設定提供；文字／結構結果用實際 recipe，單純原圖核對用受版控圖像核對 recipe。驗 descriptor、receipt、raw bytes 與完整 F1 用途閉包 |
| third_party_url | 第三方店家頁只留 URL、人工定位及 checked_on，不封存頁面；source_ref 必為 null。此類的 reference URL 才可不同於 authored source_record.url（後者通常 null）。沒有 fetched_at／ETag／HTML hash，不能宣稱可重播第三方內容 |

來源類別按後續 carddb PR 放在 `carddb/src/sve_carddb/manual_printings/policy.py` 的受版控精確 host 白名單核驗（資料 caller 不能擴張）。依協調者核定的實際證據頁，初版官方 host 恰為 `shadowverse-evolve.com`、`en.shadowverse-evolve.com`、`products.shadowverse-evolve.com`（周年頁）與 `campaign.shadowverse-wb.com`（數位版活動頁）。以完整主機名比對，不含子網域萬用或同根網域推定；其他網域一律第三方，新增官方 host 須先修改受版控名單並審核。不能由 caller 把名單內的官方 URL 標為 third_party_url 來避過歸檔。官方 image 與頁面均受 raw 保存規則；official 網域樣圖不因不在 cardlist 就變成第三方圖，但是否可發布、含哪些樣張標示留 #210 逐圖核對。source_ref 的 parser/locator/result_hash 表示此用途結果，不能取代 raw_hash；純來源閉包核對的 recipe 固定 archive-closure-v1，result_hash 為 H(來源 descriptor)，不是卡片語義判定。其他 image 核對／結構 recipe 要有受版控定義並釘實際程式與設定，未支援的 recipe 拒絕。

每筆 evidence 恰為排序去重的 `{reference_url,role}`，必解到同筆 references 的 URL／role；採納號碼、分母、對應或配送值必有適用的 evidence，不能僅有無關連結。third_party_url 的人工核對與採納只證明維護者確認了當時記錄的值／不確定程度，不證明頁面目前未變；本入口不探測、不抓第三方頁。

匯入每片建立 kind=authored 的 source_record，sha256 為分片 **exact bytes**，parser_version=manual-printings-v1，保存 authored revision/path。待實作的 third_party_url printing_reference.source_id 指這一 authored 來源，**提案新增 url 欄保存參考 URL**；不得建立只有 URL 且假 hash 的 third_party_page，不把 H(URL) 或成員 hash 當第三方內容 hash。official_archived 的 source_id 指真實 raw 來源，且 reference.url 必 exact 等於已封存 source_record／descriptor 的 canonical URL；不得拿另個官網連結代替證據網址。decision_source 同時指完整封套與所有官方 evidence。新人工 printing.decision_id 指此入口的採納；官方 supplement 不覆寫 printing.source_id／原 identity decision_id，序號決定由釘住的 subject 記錄、authored source／decision_source 與 reference 追回，不能犧牲官方身分追溯。URL-only 不屬 raw ArchiveSourceUse；F1 仍釘完整 authored 輸入，已消費官方 raw 仍全列用途。

`distribution` 恰為 `{inclusion_kind,date_precision,date_raw,available_on,note}`，kind 沿 printing_product 的 pack/box/first_edition_campaign/qr_redemption/event_prize/other。沒有實體商品的獎品、數位抽選使用 distribution 參考，不建 product、不造 released_on；note 必非空。真實商品收錄使用 inclusions，沿用 product-authored 的同區、來源閉包與採納；不能用 URL-only 參考繞過人工商品證據門檻。

date_precision=day 才有非 null available_on 完整 ISO 日期；month/year 保留 date_raw 而 available_on=null；unknown 亦為 null。來源只知月份／年份不補一號／一月，取得日、公告日、抓取日不互代。reference.checked_on 是人工查核日期，不是發售／取得日期。沒有可信首次取得日的條目不能藉此取得表記的年代優先權。

serial_total 的語意是**卡面分母**，不等於每種實際製造張數或全球總量。EN 一周年依更正決定填 10，serial_note 記實際每種一張；正式資料須釘其官方公告／卡面證據，不從另一區借值，也不增加 actual_total 真值欄。不收個人持有的第 n 號。

## 5. 重建、freshness 與公開呈現

review_context 釘完整核對背景，dependencies 釘相關已採納 family、商品、版次身分／面與同區 current 依據；不能只看 context hash 換了就 stale，也不能忽略相關內容變化。每次重建驗原始封套／來源及當前相關依賴。來源完好而對應身分、面、號碼證據或其語義基礎變動時停止套用、列 stale，須續版重新核對；禁止退回更舊的採納假裝有效。來源損壞或不完整是整次交易失敗。第三方 URL 不具可機械驗新的頁面內容，保留 checked_on／查證限制，不自動更新或升級信心。

無一般版對應的 unlisted printing 仍引用 provisional card／face，僅顯示卡號、人工名稱、公開來源與狀態／信心，不顯示卡文。人工名稱 text_unit 的來源為 authored，不能作 official_name、printed_name、翻譯 own_source、構築 rules_name 或 DSL 的來源。搜尋可用人工名稱且標示來源；卡種／數值／效果未知，不以空字串宣稱無能力。confirmed counterpart 才由其同區 card／face 讀可用文字；官方 PR 的 card／current 不受此限制。

完全沒有文字、current 與觀測的面，其公開端表示留待後續格式 PR 定義；在那之前這類條目不能出貨。現行格式要求無 current 的面提供「表記未定」項目，且其中的候選須連回該版次面的觀測，observations=[] 目前無法同時滿足這兩項要求。不得偽造觀測／候選、借用人工卡名或略過表記未定項目來通過出貨檢查；內部 provisional 身分與卡圖登錄不等於可發布。

**待實作欄位方案，不改本次公開白名單／欄序**：公開 printing 擬新增末尾 nullable `manual_metadata`，其具名 tuple ManualPrintingMetadata 恰有 `manual_name_unit_id?,serial_note_unit_id?,references,distribution`；references 為 `{url,source_class,checked_on,roles,locator?}` 的排序陣列，distribution 為 `{source_url,inclusion_kind,date_precision,date_raw?,available_on?,note_unit_id}` 陣列。每筆來源公開官方／第三方類別及人工查核日，不能只出 URL 而隱藏未封存的限制；official URL 仍等於 frozen URL。正常官方列 manual_metadata=null；官方 supplement 可以有序號註記／來源，但 manual_name 為 null。manual_metadata 擬由 bootstrap 主儲存，需連同人工名稱／註記閉包量測啟動預算；其餘印刷原文欄仍沿原來 fragment。

後續格式 PR 須一次同步 snapshot-format、snapshot-transport 的末尾欄、types／Schema／golden／producer／各 reader，不能中間插欄或在舊 format 靜默換型別。尚未正式發布時依 snapshot-contract 的候選流程同步；已發布後追加 tuple 欄須按 snapshot-transport §1.1 升 format major，設最低 reader 版本與 required_capabilities 含 manual-printings-v1。未知 format／能力的 reader 只在 current／previous 或本機已驗 active 中選相容快照；沒有則提示更新，不搜尋永久歷史索引，不截斷新欄後猜讀。card_id 非空、card／face／image join 沿既有規則；在機器契約實作前不宣稱人工名稱／查核日已可出貨。

unlisted 頁面明示「非官方整理，可能不完整」、信心、暫定號碼狀態、人工名稱與回報入口。card_no_state=official 不會把 catalog_state 變成 official。圖片沿 mirror_reviewed、Decklog 沿 regional_decklog；參考 image URL 或本入口 confirmed 不等於圖片可鏡像，也不等於 Decklog 已查證。未查證的 unlisted 仍預設不能新加入／分享／匯出牌組，既有碼與條目按既有永久配號規則保留。

## 6. 下游驗收邊界

程式 PR 須以合成資料驗：候選／high／無關 confirmed 不能採納；官方 URL 不得假裝第三方；URL-only 無 raw hash 可採納但不造原頁；official 缺 archive pin 拒絕；SNC（含 BP20-SNC01）／WB／PR 歸屬正確且不按卡號前綴誤分 BP20；稀有度 ANV 不作 family；PR supplement 不增版次、不改 PR 歸屬且保留官方卡文（含 PR-442）；暫定號改正 ID／int_id 不變及 alias 無環；日英相近號／不同面不自動配對；人工名稱不進官方文字／rules_name；未對應條目以 provisional card／face 可見、可掛圖而無卡文；月年不補日期；分母與實際張數不混用；無商品配送不造 product；歷史續版／全成員 hash／相關內容 stale／交易回滾均有反例。

printing_reference 擬新增 url／source_class／checked_on 及配送欄位，唯一鍵擬為 `(printing_id,source_id,url,role)`，正式欄名／型別與 CHECK 在改表定義的 carddb PR 同步權威列。人工名稱／序號註記的建置欄位及公開 manual_metadata 亦須在後續 PR 同步 DDL、機器 Schema、types、合成 golden、producer／reader 及 ER 引用閉包；前端實作另拆 sim/web 單位。不能只改 Markdown 後宣稱已支援。新增 SNC／WB family 與真實記錄留 authored PR，先有正式來源及維護者逐筆核對；圖片留 #210，張數／EN 查證留 #211。本 docs PR 不授權來源抓取或資料發布。

公開卡圖及快照保留依 [snapshot-format §4.1](snapshot-format.md#41-發布窗口圖片新鮮度與回收)。
此限制不刪 predecessor／adoption 歷史、永久 printing.int_id 或人可見路由 alias；上述建置重播不是對局回放服務。
卡圖 2.0 使用 int_id＋永久 face.ordinal 的背景 URL 與卡包版本，不以 manual source image_id 當公開 key。
