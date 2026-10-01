# 翻譯、模板與跨區採納契約

本文件細化 [build-db §9／§14](build-db.md#9-翻譯句型與術語) 與 [authored-layout §6](authored-layout.md#6-模板翻譯與語義例外)。只定義文件契約，不表示已有來源遷入、翻譯採納、DDL 或匯入器。範例全部為合成識別碼與自撰文字，不是真實卡文或審核紀錄。

## 1. 核可範圍與來源選擇

**使用者已核可（2026-10-01）**：繁中翻譯以**日文卡文為來源**；日英身分已確認為同一張卡時一律以日文為來源。只有兩區版本明顯不同、或英文版獨有的卡才以英文為來源。此決定適用卡文翻譯，不改商品、Q&A、CR 各自的原文來源。

來源選擇與跨區適用分開。日英同卡仍從 JP 建立繁中譯文；要把該譯文顯示在 EN，須再通過目前兩端的 `region_text_review=aligned`、面對應與相關 scope 無未解 divergence。未核對不能先從 EN 另翻來繞過 JP 來源規則，也不能把「同卡」寫成「已證明等義」。JP 來源缺失就報缺來源並回 EN 原文，不以資料缺失作 EN 例外。英文獨有需 confirmed_none 的範圍及 as_of 證據；unmapped/pending 不等於獨有。兩區確有差異須 confirmed 的 divergence 證據，按受影響 scope 使用 EN，未受影響欄仍沿 JP。

沿用既定政策：同語義模板與參數只選一種翻法；翻譯最低 sampled 或 confirmed；machine 審過仍是 machine。數位官方卡名只限已採納的同概念，優先 svwb 再 sv1；same_character、同字串、confidence=high 均不足以套用。本站效果翻譯 authority 永遠 unofficial；直接顯示已核對 SVE 官方 counterpart 是官方原文選用，並非本站翻出的效果。繁中效果不能因用了官方數位卡名而變官方。

**以下新增格式、遷移操作與選用細節均為「提案，待使用者決定」**（§2–§8）；其中重述的不可變 ID、引用閉包、來源失效及人工門檻沿既定契約，不因提案狀態放寬。格式核可不等於資料採納。首輪卡包範圍、人工抽查政策與工作量亦須另定，不能預填核可人或把模型審查當人工抽查。

## 2. 獨立入口、封套與決定續版（提案）

沿 [authored-layout §1／§2／§10](authored-layout.md#10-商品人工輸入-product-authored-v1) 的 YAML 1.2 解析限制、未知欄拒絕、可空欄明示 null、canonical JSON recipe、完整分片 <1 MiB 與 512 KiB 目標；不擴充身分 registry v1。以下路徑相對 authored 根目錄。

| 路徑 | 完整頂層欄位 |
| --- | --- |
| `translations/index.yaml` | `translation_authored_format: 1, kind: translation_index, includes, inventories` |
| `translations/<area>/<filing_key>/<sequence>.yaml` | `translation_authored_format: 1, kind: translation_shard, default_decision_id, records, decisions` |
| `translations/inventories/<sequence>.yaml` | `translation_inventory_format: 1, kind: translation_inventory, artifacts, templates`，見 §3 |

area 恰為 `templates/glossary/contexts/uses/translations/selections/semantics/region-reviews`。filing_key 是 `[A-Za-z0-9_-]+`，只分檔、不代表卡片身分或商品收錄；sequence 為只增三位以上十進位序號。includes／inventories 分別映射上述分片／清冊路徑到解析後 canonical Hash；只讀 index 指名項目，拒絕絕對路徑、`..`、symlink、重複及未索引 YAML。先驗全區所有分片、清冊及決定，再作地區／語言投影。啟用時缺 index 是錯誤，空集合明示兩個空映射；新檔驗妥後原子換 index，已登錄分片與清冊不原地修改。

每筆 record 恰有 `record_key, kind, filing_key, data, evidence`；同一分片只有一種 kind、一個 default_decision_id，decisions 恰含該決定。record_key 是 `[kind,...主鍵]` 的 §2 canonical JSON **字串**，不可用分隔符拼接。主鍵、record_key 全入口唯一；下表的不可變物件以自身主鍵命名，需替換的選擇以 adoption_no 追加，不以重複主鍵或檔案順序覆蓋。

| area | kind 與 record_key 的主鍵部分 |
| --- | --- |
| templates | `sentence_template: [id]`；`template_component: [parent_id,ordinal]`；`template_translation: [template_id,lang,revision]` |
| glossary | `glossary_term: [id]`；`glossary_choice: [term_id,lang,adoption_no]` |
| contexts | `translation_context: [id]`；`text_template_binding: [id]` |
| uses | `translation_use: [id]` |
| translations | `translation: [id]` |
| selections | `translation_selection: [context_id,target_lang,adoption_no]` |
| semantics | `face_semantics: [id]`；`revision_semantics: [revision_id,adoption_no]`；`semantic_reference: [semantic_id,target_face_id,relation]` |
| region-reviews | `region_text_review: [card_id,region,source_jp_hash,source_region_hash,adoption_no]` |

所有 data 不存本筆自身的 decision_id，由封套展開到 build-db 具體 FK；same_concept 等證據可引用另一筆既有 decision_id；D 表的決定保留於 authored source_record／decision_source，不虛構 DB 欄。decision 欄位與 record/batch 規則沿 authored-layout §2：semantic_content_hash 取完整 record（含 evidence），members 釘 `(record_key,semantic_content_hash)` 排序集合；record scope 三個 batch 欄皆 null，batch 的 membership_hash/policy_id/sample_ids 必填。sampled 必須真人、時間、非空精確樣本子集；confirmed batch 的 checked 集合是全部成員。候選、模型審查不得進已採納投影。採納不得跨越 kind 所需的更嚴門檻：身分、divergence、跨表記等義仍依各自 confirmed／核可規則。

有 adoption_no 的 data 皆加 `adoption_no, predecessor`；從 1 連續只增，首筆 predecessor=null，續筆為 `{record_key, semantic_content_hash, decision_id}`，須精確指向同選擇鍵的唯一有效前件。每筆完整替代、不作局部 patch，拒絕分叉、缺號、循環、錯前件；只有達門檻的新決定推進有效鏈，proposed 不能撤銷已採納選擇，rejected 不等於撤回前件。撤回以新的已採納 selection 指 null（§6）；舊決定、舊譯與舊快照均保留。數位與身分修復仍走其原入口，不藉此續版重配 card/face。

evidence 為去重陣列，每項是兩種具名引用之一：`{kind: raw, store_id, batch_id, source_version_id, locator, role}` 或 `{kind: artifact, artifact_id, locator, role}`。raw 依來源歸檔驗 batch／descriptor／receipt／bytes；artifact 依 §3 驗 bytes。locator 是不可執行的精確定位，role 非空；不得只放 URL 或摘要 hash 冒充可重建內容。決定另外釘完整 authored commit、index／分片 canonical hash；原文不複製到封套。decision_source 指回封套及所有證據。

## 3. 舊 ID、完整內容與持久清冊（提案）

既有分類 ID 是 `T`（sentence）或 `C`（clause）加 SHA-256(normalized UTF-8) 的前 10 小寫 hex。舊產生器先分離提示文再做 NFKC、引號內名稱占位及數字占位；這是候選分組 recipe，**不是可安全刪除所有括號的語義政策**。保留 normalized 的 exact bytes 與產生器版本，不能只保存重跑指令。分類標註或模板共用不等於翻譯／semantics 已採納。

正式 runtime 只讀持久清冊及正式 authored；禁止讀本機研究草稿路徑或 latest cache。清冊保留以下內容：

| 欄位 | 完整內容與檢查 |
| --- | --- |
| artifacts | 每項 `{id, kind, object_hash, byte_size, media_type, producer, inputs}`；kind 為 `legacy_templates/legacy_clauses/legacy_members/catalog/glossary/digital_map/normalizer/template_payload/translation_payload`；id 永久，object_hash 驗 exact bytes，producer 為 `{name,version,code_hash,config_hash}`，inputs 是 `{kind: artifact,artifact_id}` 或 §2 raw evidence 引用的去重陣列，閉包完整且無循環 |
| templates | 每項 `{id, level, legacy_fingerprint, content_hash, payload_ref, normalizer_ref, legacy_refs}`；legacy_fingerprint 為舊 normalized 全 SHA-256，新模板為 null；後三者分別為 artifact 定位引用、產生器 artifact ID、舊資料定位引用陣列 |
| 定位引用 | `{artifact_id, locator, value_hash}`；locator 以 JSON Pointer 指向 JSON，JSONL 則固定為 `{line: 正整數, pointer: 字串}`；value_hash 驗該值的 canonical-json-v1，缺行、錯型或 hash 不符拒絕 |

artifact bytes 存於 repo 外、內容定址的持久物件庫；清冊只保存邏輯 ID/hash/定位，不放本機絕對路徑。執行時明示 artifact 根目錄映射，按 hash 取物件，禁止聯網補取或向草稿回退。此物件庫與官方 raw 來源歸檔分開，不擅自擴充 source-archive 的來源種類；每次新增須獨立備份並驗還原，任一物件缺失即不能完成依賴它的採納。含官方卡文、normalized 或第三方官方名稱的 payload 不進 git；index/清冊及人工原創譯文可進 authored，完整 hash 不授權複製來源文字。全庫不需重新抓取，但正式採納所依賴的 raw／數位來源與 artifact 必須可驗。

sentence_template 的不可變 payload 恰為 `{level,source_lang,normalized_text,normalizer_version,semantic_variant,parameter_schema}`；content_hash 使用 build-db §14 canonical-json-v1。這與封套的 §2 hash、舊分類 normalized 指紋及 raw bytes hash 是四種不同用途，不能互換。參數宣告須指定型別及引用種類；出現順序由 normalized_text／組件 ordinal 釘住，不依賴 JSON object 的鍵序；沒有 schema 的舊分類不能先補一個猜測的空 schema 並標 reviewed。

初次遷入先保存所有舊 ID 與來源內容，再由核對建立精確 payload；未具備 payload 的 ID 保留於 legacy artifact，**不產可採納 templates 項**。一個 ID 一旦登錄完整 payload，不得改任何欄位。同舊 ID 對到不同 normalized 或不同完整 payload 一律停止該次匯入，不覆蓋、不合併；全 hash 相同也比完整 bytes。normalizer、schema、參數意義、semantic_variant 或文字變動均新配 ID，`supersedes_id` 明示原 ID，禁止 self/cycle；supersedes 不自動改 binding、不繼承舊翻譯決定。多個新語義分叉可各自 supersede 同一舊候選，使用者需精確選 binding，不能靠「最新」挑子孫。

**新 ID 配法提案**：`T`／`C` 加完整 payload hash 的最短可用前綴，從 16 hex 起、每次加 2、最長 64；永久 registry 檢查全部歷史 ID 與內容，交易內分配並記清冊。只加長新鍵，不重配任何舊鍵。已存在同 ID 同 payload 可冪等重用，異內容碰撞不得以覆寫處理；完整 hash 撞異 bytes 則失敗。舊 10 hex 鍵的碰撞在遷入時直接失敗，不能靠更改其中一個歷史 ID 脫困。

## 4. 模板、術語與上下文 data（提案）

下列「完整欄位」加上 §2 指定的續版欄位；`?` 表示明示 null，其他必填。型別與 enum 沿 build-db。`source_ref` 一律 `{unit_id,lang,content_hash,evidence}`，驗 unit exact bytes/hash、語言與來源 evidence 可重建同一值，不在 YAML 重抄原文。

| kind | data 完整欄位 |
| --- | --- |
| sentence_template | `id, payload_ref, content_hash, supersedes_id?`；payload 由 §3 解出 DB 欄位，必須與清冊同 ID 相等 |
| template_component | `parent_id, ordinal, child_id`；ordinal 自 0 連續，父為 sentence、子為 clause，語言相同；子參數為父參數的同名同型子集，不作隱式重新命名，圖無環；完整子清單隨 parent 首次採納封存，增刪／換子亦須新 parent ID，不能在舊父上追加組件 |
| template_translation | `template_id, lang, revision, text, status, origin`；revision 從 1 只增，status=draft/reviewed；引用正確參數且無遺漏／額外參數；最高已審且採納的精確 template/lang 修訂供新產物使用 |
| glossary_term | `id, category, source_ref, concept_key`；source_ref.lang=ja，投影 source_ja；concept_key 全域唯一，不能以日文同字串作概念唯一鍵 |
| glossary_choice | `term_id, lang, value, origin, same_concept?`；value 為 `{kind: authored,text}` 或 `{kind: source,source_ref}`；same_concept 為 `{digital_face_id,lang,name_unit_id,decision_id}`，僅數位官方來源必填，需同概念採納及該語數位原文閉包 |
| translation_context | `id, source_ref, semantic_variant, reason`；投影 source_unit_id；default 的 reason 可空，非 default 必須採納非空的同字異義理由；`(source_unit_id,semantic_variant)` 唯一 |
| text_template_binding | `id, context_id, ordinal, template_id, params, source_span`；ordinal 從 0 連續；params 按精確 parameter_schema 驗，card／term ID 驗存在與概念適用 |

source_span 固定 `{start,end}`，以**原文 Unicode code point** 的半開區間 `[start,end)` 計，不是 UTF-8 bytes／JS UTF-16 code units，也不是 normalized 的 offset。每個 context 的頂層 binding 按 ordinal 不重疊、無缺口覆蓋整份來源（含標點、提示與空白）；空字串用空 binding 列表，未知 null 不能代入。拆出的子句透過 template_component 組合，不另放重疊頂層 span。正規化時移出的提示仍須有對應模板，不能消失。單次出現句亦用模板；template 回填 params 後須能以釘住的 span 對照規則驗回來源，不可把正規化碰巧相同當成完整匹配。

template_component 清單另參與 parent 記錄的成員核對；組句器也是 normalizer_version 的一部分，改組合策略須升該版本及新 ID，以維持 build-db 已定的六欄 content_hash recipe；不能為純組件修正捏造同字異義 variant。術語原意改變須新 term/concept，不原地改舊項；只改選定譯詞以 glossary_choice 續版。不同 origin 同字不表示同品質，候選仍各留證據。

## 5. 翻譯用途與完整例子（提案）

translation_use.data 恰為 `id, context_id, owner, field, ordinal`。owner 是下列 tagged union，恰一組，匯入展開 build-db 的 nullable owner 欄，其他皆 null。這個 owner 與 filing_key 不同。use 是不可變的首次綁定；同 owner/field/ordinal 的內容修正不可新增第二筆活躍 use 來繞過唯一約束，見 §6 的修正邊界。owner 必須存在，field 合法，context.source_unit 必須**原樣等於 owner 指定欄位的來源單元**；section/action ordinal 必填且存在，其餘為 null。

| owner.kind／其餘欄位 | 可用 field 與原文 |
| --- | --- |
| face_revision／revision_id | name、effect、section；revision 的 name/effect/sections[ordinal] |
| printing_face／printing_id,face_id | name、effect、flavor、section；該版次的已知 printed 欄、flavor 或 printing_text_section；printed unknown 不冒用 current |
| qa_version／qa_version_id | question、answer |
| cr_clause／cr_clause_id | effect（條文正文） |
| vocabulary／vocabulary_kind,vocabulary_code | label |
| keyword／keyword_id | label、effect（definition）、action_label（已投影 actions 順序的標籤） |
| product_family／product_family_id | name |
| product／product_id | name |

`(owner,field,ordinal)` 條件唯一；不同 owner 相同 source_unit 且同語義共用 context，不每卡另建 variant。無主文的 exact 空字串仍可核一致；缺資料不得用空字串、其他版次或另一區字串填補。

以下為**完整邏輯例子**，U0/U1 等是合成 text_unit 的別名，hash 在實際封套必依內容算出，不能把這些別名當 Hash。各表給齊所示情境的來源 owner、context、selection 與公開結果；共同封套、來源閉包與其他 nullable 欄依 §2，不是可直接匯入的 production YAML。

| 項目 | 例 A：一般 owner | 例 B：同字異義 |
| --- | --- | --- |
| 來源單元 | U0=(ja,「試験値を2増やす。」)，由合成 JP revision RJ.effect 引用 | U1=(ja,「星」)，兩個合成 revision RA.name、RB.name 都引用 U1 |
| context | CX0=(U0,default) | CXA=(U1,character_a)、CXB=(U1,object_b)，各有已採納理由，不能只說「這卡想另翻」 |
| use | UX0=(CX0,face_revision RJ,effect,null) | UXA=(CXA,face_revision RA,name,null)、UXB=(CXB,face_revision RB,name,null) |
| binding | B0=(CX0,0,T-new,{count: 2},[0,len(U0.text)))；T-new 的 normalized_text=「試験値を{count}増やす。」、parameter_schema 指 count 為非負整數且禁止額外鍵，zh-Hant revision=1 已審譯本為「測試單位增加{count}」 | BA/BB 各綁不同 semantic_variant 的精確 name 模板／術語；同字 U1 不合併概念 |
| translation | TR0=(CX0,zh-Hant,1,「測試單位增加2」,project,unofficial,reviewed)，釘 B0 與該模板譯本 | TRA=(CXA,zh-Hant,1,「星角色」)、TRB=(CXB,zh-Hant,1,「星物件」)，皆 project/unofficial/reviewed 且依各自來源及依賴採納 |
| selection | (CX0,zh-Hant)→TR0 | (CXA,zh-Hant)→TRA、(CXB,zh-Hant)→TRB |
| FieldTranslation | RJ: `{field: effect, ordinal: null, target_lang: zh-Hant, translation_id: TR0, basis: own_source}` | RA/RB 分別 own_source→TRA/TRB；不能全域依 U1 選唯一譯文 |

### 5.1 官方 counterpart 與共用 JP

合成 card K 的 JP revision RJ 與 EN revision RE 具有已確認的共同 face F；RJ.effect=UJ、RE.effect=UE（兩份各自的官方來源）。CJ=(UJ,default)、CE=(UE,default)，UJ/UE 各有自身 use，不能給 RE 建一筆 context=CJ 的假 use。已採納且 fresh 的規則與 display 核對釘住 RJ/RE；無相關 divergence。

- JP→繁中：人工／模板產生 TZ，source=context CJ，selection(CJ,zh-Hant)=TZ；JP 顯示 own_source，EN 顯示 shared_jp，兩邊是同一 translation.id，EN 不存另一份繁中效果。
- JP→英文：建置推導 TO，context CJ、target_lang=en、text 直接引用 RE 的 UE，origin=official_sve、authority=sve_official、status=reviewed；source_hash 仍驗 UJ，另在建置輸入紀錄釘 RE/UE 與跨區核對決定，decision_id 可空（D），不是另造逐句人工 equivalence。selection(CJ,en)=TO；RJ 的 FieldTranslation 為 official_counterpart。
- EN→日文：對稱推導 context CE、target_lang=ja、text=UJ 的 TOJ；RE 的 FieldTranslation 為 official_counterpart。這個投影重用 JP 的來源 use；CE 不換成 UJ。UI 與原文同語言時不再顯示重複翻譯列。
- 較晚出現官方 EN，不直接覆寫先前 machine 譯文 TM；保留 TM 歷史，僅在 counterpart 全部門檻通過後改有效 selection。若 RE 改 effect 或核對失效，TO 不再選用；不因 origin=official_sve 跳過 freshness。
- EN 規則 divergent：停止該 scope 的 shared_jp/counterpart；依 §1 EN 來源例外建立 CE 的新繁中翻譯及 own_source 選用，未完成先顯示 EN 原文。name-only divergence 不自動撤銷獨立且仍有效的效果選用，但影響 rules_names／名稱依賴者須重驗。

## 6. translation、依賴與 selection（提案）

translation.data 恰含 `id, context_id, target_lang, revision, value, tokens, origin, authority, status, source_hash, translated_by, translated_at, bindings, terms`。value 與 glossary_choice 同一 tagged union；tokens 明示 null 或符合公開 token 契約的資料，不能借它執行程式。revision 從 1 只增，`(context_id,target_lang,revision)` 與 id 皆唯一；source_hash 是 context 的 exact 原文 UTF-8 hash，不是模板 hash 或 rule bundle。origin/authority/status 使用 build-db enum；人工寫入允許 draft/reviewed，stale 由驗證推導，不能以手動改回 reviewed 洗去失效。

bindings 是 `{binding_id,template_id,lang,translation_revision}` 去重列表，完整覆蓋 context binding，lang=target_lang，template_id=binding.template_id，binding.context_id=translation.context_id，精確引用 template_translation 的三欄主鍵，投影 translation_binding。terms 是 `{term_id,lang,choice_record_key,choice_content_hash}` 去重列表，列出 params、模板譯本、最終譯文引用的全部概念與選詞，投影 translation_term 並在封套保存不可變依賴版本；不能只記 term_id 而丟失所用譯詞版次。同語義、同參數的新譯須共用有效 context/selection。

人工 template 翻譯須 reviewed 且其決定達採納門檻，最終 translation 亦須通過完整渲染、來源一致、語言與 origin/authority 檢查。official_counterpart 是 §5.1 的推導例外：不拿機器模板重翻官方文字，其完整來源／核對閉包替代 bindings/terms（兩者為空），不允許 authored 自填 official_sve 來免除模板檢查。單字卡名與標籤可由已採納術語的 name/label 模板生成，仍記選詞依賴；official_svwb/sv1 限同概念名稱選用，不可套整段數位效果。

translation_selection.data 恰為 `context_id,target_lang,translation_id?,adoption_no,predecessor`；選中的 translation 必同 context/lang、完整且非 stale，reviewed 與人工採納均滿足。null 明示撤回而非空字串譯文。此入口只存人工選用意圖；實際 DB selection 是 D，先驗人工意圖，再依既定官方 counterpart 優先規則投影，每鍵最多一筆。不能把最新 revision、較新時間、檔案序或 origin 排序當作任意候選的採納決定；沒有可採納譯文時不產 selection。

**選用失效後的回退提案**：人工意圖選中的版本失效時，取消該選用並列 stale 診斷，不默選較舊人工譯文；新的選用須新決定。官方 counterpart 失效時，僅可回到仍有效、仍通過所有依賴檢查的人工選用意圖，否則回原文。這不變更「官方 counterpart 可用時優先」的既定政策。published 舊快照不回寫，建置 DB 可推導不同有效狀態而保留原 immutable 記錄。

| 變更或反例 | 失效範圍與修復 |
| --- | --- |
| exact 原文變更但規則等義 | 新 text_unit/context，不搬舊 selection；原 translation 對歷史 owner 可保持有效，對新 owner 不適用，報告列受影響用途待重建；不能因 rule_hash 相同繼承譯句 |
| normalizer/schema/semantic_variant 或 params 改 | 新 template／binding／translation，反查所有直接與 component 傳遞依賴，舊產物不得用於新組合 |
| 模板譯本出現更高已審修訂 | 依 binding 反查選用者，舊已生成譯文對當前模板選用 stale；新生成、重驗與採納，不原地換字 |
| 選定術語修訂，即使文字恰相同 | 按 choice 精確版本反查 translation_term 與參數依賴；舊依賴 stale，重核來源/概念/authority，不能只比輸出字串 |
| 日英兩端任一規則、名稱或 source 證據變 | 按 §7 scope 重驗，移除不再適用的 shared_jp/counterpart；不必讓 JP 自有且依賴未變的繁中譯文 stale |
| 同字異義 reason／owner 改、身分修復 | 舊 context/use 不改，新 owner 追加新物件與決定；同一 owner 的改綁依下述修正邊界阻擋，重驗跨區全部端點，不沿用 repair 前的對應 |
| 只改 YAML 排版或其他不相關卡的譯文 | canonical 內容與依賴都不變，不產假 stale；已釘完整 authored revision 仍另留重建輸入紀錄 |

**同一上下文／用途的修正邊界（提案）**：本版 binding/use 首次建立後不可變。模板 supersedes 不自動改既有 binding；舊 template 及其仍有效的譯本可繼續使用。若必須更換同一 `(context,ordinal)` 的模板／參數，或同一 `(owner,field,ordinal)` 的 context，現有 DB 唯一約束無法同時保留兩組歷史引用，不能硬塞第二筆或假造 semantic_variant。提案先列需修正清單並停止受影響選用；啟用此類改綁前須另定 binding/use 版本化契約及 DDL，再恢復選用。新原文產生新 context/new owner 不受此限；模板譯本與術語選詞本身的修訂仍依上表處理。

## 7. 語義組、跨區核對與 hash 遷移（提案）

[ADR-0013](../adr/0013-rule-bundle-migration.md) 記錄遷移步驟；hash recipe 沿 build-db §14，不新增另一種 rule-bundle-v2。未啟用 semantics 時維持原先能力邊界，不回寫首發已有等義證明。首個需要 v2 的跨區共用或等義功能才啟用完整三表與驗證器，不能只建空表稱支援。

semantics data：face_semantics 為 `id,face_id,region,rule_text_ref,rule_section_refs,normalizer_version,rule_hash`；refs 依 §4 source_ref，sections 有序，投影 text IDs。revision_semantics 為 `revision_id,semantic_id,adoption_no,predecessor`；semantic_reference 為 `semantic_id,target_face_id,relation`。同面同區；初始 exact 原文、不省略內容的機械映射可不要求人工 decision（工具推導，不經本人工封套偽造決定）。跨表記、刪提醒／重複 token 等轉換必釘核可政策或 confirmed 全體 checked 與原證據。semantic_reference 的完整清單跟該 semantic 首次封存，修改即新 semantic_id，不在舊 semantic 追加或刪邊。

region_text_review.data 完整欄位如下；保存於 authored，只有 build-db 原有欄投影到該表，其他作決定／F1 證據，不改 DDL。

| 欄位 | 意義 |
| --- | --- |
| card_id, region | 共同 card 與 target region=en；兩端面對應必經已確認身分登錄 |
| source_jp_hash, source_region_hash, hash_recipe | 目前兩端精確 bundle；hash_recipe=rule-bundle-v2；不比較 hash 相等來推跨語言等義 |
| state, checked_at | pending/aligned/divergent，真實核對 UTC 時間；aligned 需 sampled/confirmed 的語義決定 |
| faces | 按 face_id 排序，每項 `{face_id,jp_revision_id,region_revision_id,jp_semantic_id,region_semantic_id}`；涵蓋全部面，不省背面 |
| display_checks | 每項 `{face_id,field,ordinal,jp_source_ref,region_source_ref,state,reason}`；field=name/effect/section；ordinal 規則同 §5，state=pending/aligned/divergent，reason 非空；釘 exact 兩端顯示文字、完整段落及語義差異，不能只用 rules hash 通過卡名／提示翻譯 |
| divergence_refs | 去重的既有 divergence 決定引用；divergent 必須有 confirmed scope 證據才採納 EN 來源例外；pending 只留診斷 |
| adoption_no, predecessor | 同雙端 hash 鍵的追加決定鏈；兩端 hash 變更則新鍵，不改舊 review |

aligned 只表示該雙端與已核對 scope 的語義對照；identity confirmed 必須另外存在，不能由 aligned 建共同 card。DB state 管規則對照，display_checks 管逐欄顯示適用，某欄 pending 不可輸出該欄 counterpart。缺整面、未知段落、缺 token 規則目標、未確認面對應或釘 hash 不符均不能產可用 aligned。批次新增成員不得繼承原 sampled；display 檢查雖存在某一欄，不得擴稱其他欄已核。

規則 bundle 不含 display revision ID／reminder，故等義表記可重用規則核對；**display_checks 仍驗 exact revision/來源**，換字後重新核對並追加決定，不能自動把舊卡名／提示審查搬過來。這也適用官方 counterpart：卡名核 exact 名稱，效果與 sections 各自驗完整 display scope，不因 rules aligned 就省掉字句／版本核對。

## 8. 顯示與發布邊界

沿 [snapshot-format §5](snapshot-format.md#5-語言矩陣與取用)：版次決定卡圖／原文，介面語言決定翻譯列，切語言不暗換 printing。JP 原文＋繁中、EN 原文＋繁中皆保留原文；日／英 UI 不回退繁中。同語原文不重複顯示翻譯；缺繁中明示「尚無繁中，顯示原文」。繁中標「非官方翻譯」，數位官方卡名可另外標明其來源，不能抹去效果非官方標籤。機器翻譯一律標「機器翻譯・非官方」，審核不移除 machine 標示。

官方 EN 尚未到齊時的英譯另標來源狀態；未確認 EN 發行狀態時，不把 unmapped 寫成「尚未發行」。printed 模式只顯示該印刷文字版本適用的 FieldTranslation，未知 printed 不拿 current 譯文冒充。未完成某一句模板不可把半翻結果標完整；pending wording 不授權跨區共用。

公開只投影已選 translation/text 與 FieldTranslation，不出 context、inventory、逐列 hash 或模板依賴。無翻譯時回原文，不阻止查卡／手動；來源損壞、輸入 hash 錯或引用閉包不完整是建置失敗，不能降為「尚無翻譯」吞掉。名稱翻譯納入 bootstrap 及容量驗收，效果等按既有分片規則；不以 docs 範例宣稱雙區三語容量已通過。

## 9. 獨立反例與定向突變驗收

這是後續實作的驗收契約，**不是本文件已執行 production importer 測試的宣稱**。每列由最小合成成功基例只改一件事；驗期望診斷／拒絕或精確選用結果，不以 producer 自己的函式反算當 oracle。成功與失敗數量、真實遷入量及合成突變量分開回報。

| 編號 | 獨立反例／定向突變 | 預期 |
| --- | --- | --- |
| V01 | index 指名分片缺失、未索引分片、../ 或 symlink 各自注入 | 各自拒絕，不掃草稿補來源 |
| V02 | 修改 record 內容而保留舊 membership_hash；另增樣本集合外項 | 各自拒絕，不能繼承抽查 |
| V03 | confirmed batch 去掉一個 checked；sampled 清空真人／時間／樣本各一次 | 各自拒絕，不以模型信心代簽 |
| V04 | source artifact 少一 bytes／缺物件／定位錯行但同 hash 字串 | 各自拒絕，必須驗內容與定位 |
| V05 | 同 T+10hex 對兩個不同完整內容 | 停止匯入；舊 ID 不重配 |
| V06 | 保留 ID 只改 normalizer／schema／variant 各一次 | 各自拒絕；新 ID+supersedes 的基例可接受 |
| V07 | 對新 ID 人為製造短前綴碰撞／全 hash 異 bytes | 前者只加長新鍵，後者拒絕 |
| V08 | supersedes 自指／成環；component 成環／舊父新增 child 各一次 | 各自拒絕，舊譯不能自動移轉 |
| V09 | 刪一段 span／重疊一字／用 UTF-16 offset 處理含非 BMP 字元來源 | 各自拒絕；逐 Unicode code point 完整覆蓋 |
| V10 | 少參數／多參數／card 或 term 懸空各一次 | 各自拒絕 |
| V11 | use 同時兩 owner／owner 與 source 不同／缺 section ordinal 各一次 | 各自拒絕；EN owner 不可硬綁 JP 字串 |
| V12 | 把 CXA/CXB 按相同 source_unit 合併，或無理由另建 variant | 前者選譯結果與例 B 不符，後者拒絕 |
| V13 | binding.context/template/lang 任一與 translation 不符 | 各自拒絕 |
| V14 | selection 選錯 context／語言、stale 或 draft 各一次 | 各自拒絕，不任取最高 revision |
| V15 | 選用續版指錯前件／分叉／proposed null 撤回 | 前二拒絕，後者不得改有效選用 |
| V16 | 只改 exact 原文但維持相同 rule_hash | 新 owner 無舊 translation 選用，舊歷史 owner 可保留 |
| V17 | 新增更高已審模板譯本／修改 selected term 版本各一次 | 依賴者 stale，無關譯文不變 |
| V18 | 只有 identity confirmed 卻設 shared_jp | 拒絕；補 fresh aligned 等證據後才可用 |
| V19 | 只有 aligned 卻未確認 card/face 身分 | 拒絕，不自動配對 |
| V20 | JP 缺來源便自 EN 翻繁中；把 pending mapping 當 EN-only | 各自拒絕；保持原文回退 |
| V21 | 同卡 fresh aligned 另建立 EN 繁中重複效果 | 拒絕偏離 JP 來源規則；共用同一 translation.id |
| V22 | name-only divergence 擴大至全部效果；反向漏擋名稱 | 前者誤失效、後者誤共用均須檢出 |
| V23 | counterpart 的一端 exact 文字／身分或 bundle 改動 | 受影響投影移除，不能因 official origin 略驗 |
| V24 | same_character 或同名取代 same_concept；machine 審後改 origin | 各自拒絕；不得漂白來源 |
| V25 | 把數位官方卡名 authority 套整段繁中效果 | 拒絕；效果仍 unofficial |
| V26 | v1 receipt 只換 recipe=v2 就沿用 verified | 拒絕，需新語義閉包及重驗收據 |
| V27 | 刪 token target／漏背面／unknown section 改當 reminder | 各自不得產可驗 v2 bundle |
| V28 | token target 規則變、規則名稱或數值變各一次 | bundle 變且重驗；純已採納表記則規則 bundle 不變 |
| V29 | 語言切換換 printing、printed 偷用 current、缺翻譯顯示空白 | 各自不符顯示契約 |
| V30 | 只調 YAML 排版或無關翻譯也令全部 stale | 檢出過度失效；canonical 與依賴未變者維持 |

V31：在同一 `(context,ordinal)` 新增替代 binding，或在同一 owner/field/ordinal 改用另一 context；首版須拒絕並列需版本化診斷，不更名 variant、覆寫舊引用或默選最後一筆。
