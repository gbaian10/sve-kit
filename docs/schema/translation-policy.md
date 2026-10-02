# 模板採納政策、首輪抽查與核可收據

本文件固定 [翻譯契約 §2](translation-contract.md#2-人工採納入口) 既有 `approved_policy`
的實體政策格式、核可收據、唯一索引與驗證閉包；重用五欄 policy pin，不另造模板採納系統。
政策格式不是政策本身的核可，文件合併也不等於首輪抽查已完成。本輪不新增真實政策、收據或採納資料。
模板載入由 #52、正式建置／渲染／選用與報告由 #53 接入，未支援時明確拒收。

效果政策僅適用模板**譯本**。風味政策可列譯本，以及 #195（1/3）提出的 exact flavor **定義**例外；
**定義例外放寬既有門檻，待維護者確認且 1/3 契約合併後才能授權**，不把「可以寬鬆」的方向回覆代作此確認。
風味首輪段數亦由維護者實際決定，不在政策預填已看過 30／50。

## 1. 重用與新入口

重用 authored-layout §1／§2 的嚴格 YAML、canonical-json-v1、完整成員 hash、日期精度及
§9.5 的**政策／收據配對與不可變 pin 做法**。`wording_rule_policy`／`wording_rule_approval` 的欄位與
matcher 授權仍屬表記，不是翻譯核可收據；不呼叫 wording 的固定 hash 白名單來放行翻譯。
glossary delegation／catalog 的真人確認也不借用本例外。

新增獨立入口，路徑均相對 authored 根：

| 路徑 | 完整頂層欄位 |
| --- | --- |
| `translation-policies/index.yaml` | `translation_policy_index_format:1,kind:translation_policy_index,policies` |
| `translation-policies/<policy_id>.policy.yaml` | `translation_policy_format:1,kind:translation_policy,policy_id,scope,initial_sample_rule,checks_recipe` |
| `translation-policies/<policy_id>.approval.yaml` | `translation_approval_format:1,kind:translation_policy_approval,policy_id,policy_hash,authorized_kinds,reviewed_by,reviewed_at,reviewed_precision,authorization_basis,initial_sample,note` |

policy_id 是 `[a-z][a-z0-9_-]*` 的永久版本 key；ID、檔名與檔內值完全相同。
`policies` 是 policy_id → **`{path,hash,approval_receipt_hash}`** 映射，path 固定為該 ID 的
`translation-policies/<policy_id>.policy.yaml`；收據路徑只能同 basename 將 `.policy.yaml` 換 `.approval.yaml`。
兩個 Hash 都是完整 `sha256:<64 lowercase hex>`，釘整個解析值，不能只 hash scope 或 note。
此索引不含自身 commit SHA；消費端由完整 authored revision 取索引與兩檔，沒有 hash 自我引用。

政策／收據不列 translations/index.yaml.includes，不藏在 glossary／templates 的檔案閉包內；
正式 loader 必須另外完整驗這個入口。啟用時 `policies={}` 明示空集合；
缺入口不能滿足任何引用它的 policy_id。存在未索引檔、孤立 policy／approval、缺檔、symlink、
不安全路徑、未知欄、未知格式或 hash 不符全部拒絕。format 欄只收整數 1，不收 bool／字串。

舊 policy_id 的 path／hash／approval_receipt_hash 和兩檔 exact bytes **只增不改**，
重新核可、改條件、改樣本、換收據或改字均另配新 policy_id／檔名／核可事件；不能原地換收據。
先完整驗新 pair，再原子追加索引；中斷留下未索引檔就停止並核對完整批次，不猜恢復。
單檔完整 YAML（索引／政策／收據各自）均 <1,048,576 bytes、目標 524,288 bytes；
超限拒絕，不私自截短樣本或成員以塞入一檔。

## 2. 政策的封閉範圍

`scope` 恰為 `{domain,kinds,roles,source_langs,target_lang,normalizer_versions,semantic_variants}`：

| 欄位 | 規則 |
| --- | --- |
| domain | effect 或 flavor，兩者分 policy_id，不能用單一不限類別政策 |
| kinds | 非空排序唯一陣列；effect 恰為 template_translation；flavor 恰為 template_translation，或在定義例外獲確認後同時列 sentence_template／template_translation |
| roles | 非空排序唯一陣列；effect 只可 body／reminder／token_header，flavor 恰為 flavor；不得 name／label／layout 或未知角色 |
| source_langs | 非空排序唯一陣列，只收 ja／en；每筆仍須來源選用及 parser 能力通過，列 en 不等於 EN 路徑已接入 |
| target_lang | 恰為 zh-Hant；定義沒有 lang 欄，此值指同政策的首輪／譯本目標語，不是假造定義語言 |
| normalizer_versions | 非空排序唯一 Code 陣列，恰列已支持且釘版的 recipe，不允許 wildcard；flavor 恰為 flavor-exact-v1 |
| semantic_variants | 非空排序唯一 Code 陣列，恰列 default／已實際採納的語義 variant；不得因 owner／檔案名自行擴張 |

符合 scope 只是必要條件。loader 逐筆經 template_id／inventory_id 解析核可模板及完整清冊，
檢查來源語言、role、recipe、語義與來源／span／ID／引用閉包；不以 filing_key、ID 前綴、confidence 或宣告的 domain 替代。
flavor 定義另驗 sentence、完整整欄 `[0,L)`／anchor=null、零參數、exact recipe，
不收空欄／只有空白的來源，不把兩模型審譯文當定義的機械驗證。
任何效果成員混入 flavor 批次，或效果定義借 flavor 政策，整批拒絕。

`checks_recipe` 恰為 `template-policy-checks-v1`；它是以下完整驗證規格的 recipe key，
不是目前已存在的可執行函式。實作須版控並釘完整依賴，且消費端明確認得該版本才可用：

1. 全入口 hash／不可變鏈／模板清冊與 source_ref／完整 raw span／參數／recipe／來源選用政策通過。
2. 譯本 `origin=machine`，兩個不同模型與版本對**最終正式 text bytes** 的互審 `agreed`、resolution=null；
   slot 語法／跳脫完整，review text_hash 相符；flavor 譯文再驗 LF／空白規則。
3. 定義只在 flavor、已核可 kind 與 exact 邊界下機械全查；無 origin／model_review 欄，不能偽填它們。
4. 真實首輪 sampled、政策／收據及當批精確成員皆能驗回；不同 review mode 分檔、同政策分批。
5. 原文或譯文變動須新 ID／revision／新當批決定與重審，政策範圍不自動擴張。

不得只比 recipe 名稱／類別便接受任意程式。matcher 行為或邊界變動須新 checks_recipe 版本、
新 policy_id、完整實作及核可收據；舊版本仍可按當時 F1 重播。

`initial_sample_rule` 恰為 `{order,minimum_count,disputed}`：

- order 固定 `frequency_desc_template_id_asc`；正式清冊拆 ID、補提示／標頭後重算頻率。
  依釘住來源集合每個版次面欄位的有效觀測計，重複歷史抓取不多算；同頻按 template_id。
- minimum_count 為正安全整數，記維護者對這份政策實際選定的首輪不同 template_id 數門檻，不固定填 100、30 或 50。
  效果仍遵循前約 100 與全部分歧的既有政策，差異需在真實核可依據說明，不由工具縮小範圍。
- disputed 在 effect 固定 `present_all`，flavor 固定 `keep_pending_unless_human_resolved`。
  有分歧者從來不能政策採納；人工處理才走 human／resolution／sample_ids。
  風味低信心亦留候選，confidence 不寫正式譯本。

## 3. 初輪樣本與核可收據

先有已採納模板**定義**，再用最終譯本做實際 human sampled 抽查，最後形成政策收據。
首輪定義沒有有效政策時仍真人 sampled；風味定義免逐包抽樣的例外只用於後續，沒有初始化循環。

approval 的 policy_id／policy_hash 必須與索引與 policy 全部一致；authorized_kinds 非空、排序唯一、
**恰等於 scope.kinds**。reviewed_by 是真正核可政策的維護者，reviewed_at／precision 為真實事件；
只有日期用 day 的 UTC 午夜編碼，不捏造時分秒。authorization_basis 是非空的正式決定定位／URL 與具體核可範圍，
不是模板作者、模型 reviewer 或 coordinator 自行同意。note 記核可限制及初輪門檻，不貼官方原文。
含 sentence_template 的收據須有**維護者明確同意 flavor 定義門檻放寬**的依據；
方向回覆、同意譯本或 glossary 委託皆不能代替。未有這個事件就不寫含定義授權的收據。

`initial_sample` 恰為 `{authored_revision,index_hash,decisions,sampled_items,review_context,review_queue_hash,disputed_items}`：

| 欄位 | 完整定義 |
| --- | --- |
| authored_revision | 首輪已完成 human sampled 的 immutable 40 碼 Git SHA；須在政策／收據寫入前存在，不引用自身未來 commit |
| index_hash | 該 revision 的完整 translations/index.yaml 解析值 canonical hash；全分片／清冊仍需完整驗證，不只讀被抽樣的片 |
| decisions | 非空、按 decision_id 排序唯一的 `{decision_id,membership_hash,sample_ids}` 陣列；sample_ids 是該封套**實際**非空 sample_ids，恰相等，不能寫全部 members 冒作看過 |
| sampled_items | 實際看過的精確譯本與真實事件，完整欄位及與已採納決定的對照如下；不是所有 model agreed 或 checked 的集合 |
| review_context | 沿 authored-layout §9.2 的 `{context,source_batches}`，釘實際首輪頻率／模型審核輸入、程式／依賴／設定與凍結來源；不引用尚未寫出的本政策收據 |
| review_queue_hash | 首輪模型審核佇列摘要完整 exact bytes 的 SHA-256 Hash，摘要輸入與驗證 recipe／bytes 另在上述 F1 釘住，用於辨識全部分歧；不是只 hash 當前 sampled 子集 |
| disputed_items | 排序唯一的 `{template_id,lang,revision,text_hash,outcome,decision_ref}` 陣列；outcome=adopted/declined/deferred。decision_ref 只在 adopted 時為 `{decision_id,membership_hash,record_key,record_hash}`，其餘 null |

所有 referenced decision 必須為 template_translation 的 **human sampled**、同 domain／target_lang，
由真實人工事件建立，不能指政策 confirmed、模型同意、模板定義 sampled 或另種 glossary 決定。
樣本的 origin 必須 machine，model_review 為兩個不同模型／版本對最終 text_hash 的 agreed，
或 disputed 且已由人處理的 resolution；不借 project 譯文樣本建立機器長尾政策。
逐個 sample_ids 解析 record_key／record_hash／membership；驗 template ID、lang、revision、最終 text hash，
`sampled_items` 是按 `(template_id,lang,revision)` 排序唯一、非空的陣列，每項恰為
`{template_id,lang,revision,text_hash,outcome,decision_ref,reviewed_by,reviewed_at,reviewed_precision,note}`。
outcome=adopted/declined/deferred，decision_ref 沿 disputed_items 的四欄，僅 adopted 非 null。
每項都須人實際看過且有真實人名／事件／非空核對 note；deferred 在此表示看過而暫緩，
不能把未回答的佇列當看過。day 精度同前，不捏造時分秒；不同樣本可有不同 reviewer／事件。
adopted 項逐一對到上述 human sampled 決定的實際 sample_ids，兩邊集合恰相等；
譯本、最終 text_hash、resolution 及決定 reviewer／事件皆須相符。
declined／deferred 不占正式 record_key，保留私人候選與 exact 審查證據，receipt 只存 ID／hash／事件。未採納項的精確候選譯本、互審及人工核對證據仍以私人工具輸入
釘進 review_context 的 exact bytes dependencies，須可重取、逐項對 hash；不把摘要當原證據。

正式清冊重算 scope 內頻率與順序；首輪佇列每個 template_id 只取當時待審的精確譯本 revision。
實際 sampled_items 的不同 template_id 數不小於 minimum_count，且包含排序的前 minimum_count 個。
不能以同模板多個 revision、重複成員、低頻項替補或 model agreed 數湊門檻。
人看過而拒絕／暫緩的高頻項仍計入實際抽查，但不轉成已採納 sample_ids；
未回答的項目不計。如此未採納分歧不會因最高頻而阻擋其他合法無分歧長尾。
全部分歧另依下述佇列列出，不能把未採納分歧當已採納 sample_ids。
不得要求核可政策的人與所有樣本 reviewer 同日才有效，但兩個事件須各自真實、可追溯，不能把工具時鐘當人工事件。

首輪審核佇列摘要是私人、可重播的工具輸入，**不放 translations 正式入口或 Git**。
其固定格式恰為 `{template_review_queue_format:1,kind:template_review_queue,rows}`；
rows 按 `(template_id,lang,revision)` 排序唯一，format 僅整數 1、每項均在已驗 scope，
revision 為正安全整數、lang 與 target_lang 相符、Hash 為完整 SHA-256；每項恰為
`{template_id,lang,revision,text_hash,result}`，result=agreed/disputed；沒有原文、譯文或 confidence。
消費端驗 exact bytes hash、格式／閉包與輸入釘版；樣本／正式譯本的模型紀錄須與它相符；未採納候選亦核對釘住的私人互審輸入，
須恰列首輪互審輸入的全部候選，不能先濾掉分歧再 hash，
摘要也不能取代正式 model_review 的兩模型與版本證據。
effect 的 disputed_items 必須恰列該 scope 佇列的全部 disputed；flavor 亦完整列出分歧，
未處理就 deferred 並排除採納。adopted 須指首輪 human sample 的 exact 成員／resolution，並在 sampled_items 有相同核對項；
declined／deferred 不占正式 record_key、不產生 proposed 分片；receipt 的列舉不把它們變成已採納。declined 須在 sampled_items 有相符的真實人工拒絕事件；
deferred 如人已看過須有相符 sampled_items，否則表示尚未處理，不捏造人名或決定。
effect `present_all` 另須 note 保存已呈現全部分歧的真實事件；這不是所有項目均已同意，
未定項保留候選、不阻擋無分歧長尾符合完整政策後採納。
缺私人摘要／凍結輸入或不能重算時不能走政策路徑，不從 live、草稿最新檔或只憑人名補洞。

收據、首輪樣本 commit、清冊、review context、佇列摘要與兩種 recipe 全部是閉包。
政策 ID／收據不可重用於另一個範圍；後續仍符合原 scope 的無分歧譯本可引用首輪收據，
不冒稱維護者看過新字。修改樣本集合／最低門檻／互審要求須新版本與真實核可，不沿用舊收據 hash。

## 4. 五欄 pin、定義引用與 F1 複核

譯本 `adoption_review` 仍恰為 `{mode,policy,initial_sample_decisions}`，既有五欄 policy 仍是：

```text
{policy_id, authored_revision, path, hash, approval_receipt_hash}
```

path 是 repo 相對 `authored/translation-policies/<policy_id>.policy.yaml`；
authored_revision 是保存**政策索引與完整 pair** 的 immutable commit，
兩檔從此 revision 重取，policy hash／receipt hash 與索引 entry 必須一致。
`initial_sample_decisions` 仍是排序唯一的 `{decision_id,membership_hash}`，
**恰等於** approval.initial_sample.decisions 的這兩欄，不額外塞 sample_ids 或改既有必填形狀。
真正 sample_ids、來源與頻率從上述首輪 revision／receipt 解析，不從後續最新入口猜。
同批 pin／初輪引用相同，decision.policy_id 必須同 policy_id。

風味定義仍用九欄 sentence_template.data，不加 adoption_review。其 confirmed 政策 decision.policy_id
必須在本次 **authored 的完整 translation-policies index** 唯一對到 pair，
該 ID 永久綁不可變內容與收據。author source_record／decision_source 釘完整 authored revision、索引／兩檔，
再驗首輪、授權 kinds 與 flavor exact 邊界；僅有同名政策或呼叫端五欄 pin 不算有效採納。
human 定義採納用獨立人工 policy_id，不能把政策 mode 與 human mode 混在一片。
本例外須以已合併 1/3 與維護者確認為前提，未到位則拒絕，不先寫政策 confirmed 等以後補證。

使用政策時 F1 configuration 必含 `translation_policies` 的 policy_id → 五欄 pin 映射，
值由**已驗 authored 索引與採納**推得，與 record pin／來源 bytes 複核，不是呼叫端可替代的採納來源。
完整索引和所有 pair 歷史須驗，引用集合與設定若漏／多政策、hash／收據／revision 不符就拒絕。
索引 canonical hash、政策／收據 canonical hash、各檔 exact bytes hash 分開保留；
F1 dependencies 含完整版本的 validator／source parser／normalizer、鎖定檔、政策輸入與首輪可重播工具輸入。
review queue 使用工具輸入的 canonical 相對名稱，保存於私人不覆寫 bundle，不記私人絕對路徑，
完整來源使用集合仍依 source-archive F1 驗證，不以一份自算 report hash 取代獨立 expected 集合。

## 5. 當批採納、報告與相容

政策套用仍是原 templates 分片，定義／譯本、human／policy 各自分檔。
機械 confirmed 的 sample_ids 恰為全部 checked record_key、members／membership_hash 都重算；
reviewed_by／reviewed_at／precision 等於該 policy approval 的事件，note 明示「政策核可」。
authored_by／authored_at 是此次工具與時間，不改譯本 machine origin、不稱真人逐筆確認。
任一成員不合法則整批不寫，候選保留在 authored 外；交易／index 原子更新、不留半套產物。
解決 disputed 仍須 human 決定與真實 resolution，不能混入 approved_policy 當批。

驗收報告至少包含下列**建置期**資訊，不進公開快照、不存官方原文：

| 欄位 | 計數／內容 |
| --- | --- |
| human_sampled_rows | 本次 human sampled 決定的實際樣本數；初輪另列 receipt.sampled_items 的不同 template_id 數及 adopted／declined／deferred，不把 checked 當人工 |
| approved_policy_rows | 當批政策採納的 template_translation 數，不等於機器候選總數 |
| approved_flavor_definition_rows | 當批 exact flavor 定義機械全查數，與譯本分列；效力前提不符時為 0 並拒絕採納 |
| pending_disputed_rows／failed_rows | 未處理分歧／缺譯或機械缺口分理由；壞 hash／closure 另作建置錯誤，不能藏成缺譯 |
| policy_receipts | 使用的五欄 pin、政策索引 hash、初輪 revision／decision／membership／真正 sample 集合，與當批精確 members hash |
| generated_rows／changed_rows／sampled_rows | 沿翻譯契約渲染報告；當批真人樣本可為 0，但初輪不得假造，生成數與採納數分開 |
| whitespace_only_rows／flavor_context_conflict_rows | 沿 1/3 的空白來源及同 context 衝突筆數、owner／context／原因；保留合法名稱／卡文，只讓不適用風味回原文 |

索引是新增獨立入口，`translation_authored_format=1`、模板／譯本／glossary 的必填 data、
已有的 264 概念／39 分片與 hash 一律不改。新能力未支持時完整入口拒收，不忽略政策或降成空集合。
現有 wording policy 的檔案與收據保持原位，不替代或重用它的授權。
公開七欄 translation／FieldTranslation、tokens=null、bootstrap／詳情分片與格式版本都不改。
風味政策不得在本格式合併且 loader 完整支持前套用；effect 亦須有可驗正式 pair／首輪才能用。

## 6. 合成成功與逐條拒絕案例

下面僅是後續實作驗收，ID／文字／收據均自撰，沒有真實核可、官方原文或已跑測試宣稱。
每個多條件格拆成獨立最小案例，不用外層 KeyError／格式錯掩蓋要驗的拒絕。

| 編號 | 合成基例／單條修改 | 預期 |
| --- | --- | --- |
| P01 | 兩個合成 human sampled 譯本、首輪 exact 輸入、不可變 effect policy／receipt／index、agreed 長尾 | 長尾可 confirmed，當批真人樣本 0；初輪仍計實際看過的兩項 |
| P02 | 合成 flavor 初輪成立、維護者明確確認定義例外、same pair 授權兩種 kind | exact 定義與 agreed 譯本各自機械全查／confirmed／分檔分計數 |
| P03 | 新包仍在 scope，沒有新真人樣本 | 可引用舊首輪／收據，不冒稱真人看過新資料 |
| P04 | 尚未採納的同內容 YAML 換排版；已採納 pair 換排版；新 policy_id 續版各一次 | 前者 canonical hash 相同而 bytes hash 不同；第二例拒絕；合法新版本保留舊核可 |
| P05 | 缺索引／漏 pair／未索引 tmp／symlink／錯 canonical hash 各一次 | 各自拒絕完整閉包，不從設定補政策 |
| P06 | 同 ID 換收據／另指 path／pin revision 不含索引／呼叫端換同名政策各一次 | 各自拒絕；authored 索引為權威 |
| P07 | 首輪不是 human sampled／sample_ids 多一項／漏一項／revision 或 text_hash 錯各一次 | 各自拒絕，不能借全 checked 冒人工 |
| P08 | 抽樣頻率用舊草稿／歷史抓取重複計／更改 minimum_count 沿用收據 | 各自拒絕；按正式來源閉包重算 |
| P09 | 同模型互審／final text 改／result disputed／machine 改 project 各一次 | 各自不得政策採納；分歧仍走 human |
| P10 | flavor 批次混 effect／定義多 slot／非 exact recipe／未確認定義例外 | 各自拒絕，逐筆清冊與 kind 複核 |
| P11 | 借 wording／glossary 收據、authorized_kinds 與 scope 不符、day 卻非 UTC 午夜各一次 | 各自拒絕錯授權／事件 |
| P12 | 初輪定義與譯本引用未來政策互相代簽 | 拒絕循環；先 human 初始化再出真實 pair |
| P13 | 佇列摘要缺 bytes／hash 改／漏 disputed／adopted 指非 sample 成員各一次 | 各自拒絕閉包；deferred 不入正式分片 |
| P14 | format=true／未知 roles／recipe／wildcard／單檔恰 1 MiB 各一次 | 各自拒絕，不靜默寬鬆或截短集合 |
| P15 | note 隱去政策核可／機械 checked 數算成人工／套用時間代核可時間各一次 | 各自檢出錯歸因；分開報告 |
| P16 | 沿用相同 index canonical hash 卻改 pair 或首輪 byte dependency | exact bytes／F1 複核拒絕，不只重算頂層 hash |
| P17 | 高頻項人看過而 declined／deferred，另有已採納 human sampled 與 agreed 長尾 | 實際抽查計數保留，未採納項不進分片，不阻擋合法長尾 |
| P18 | sampled_items 借 model agreed／漏最高頻項／同 ID 多 revision 湊數／缺真實事件各一次 | 各自拒絕，機械 checked 不冒作看過 |
| P19 | 翻譯 policy 釘新 pair，卻漏 authored 索引或把定義授權只放呼叫端設定 | 拒絕；九欄定義不加新 adoption_review，採納 authority 必須能由 authored 驗回 |

政策的實際支援、真實頻率／來源完整性與採納量各自驗收；未有完整 loader／validator 不宣稱本格式可正式套用。
