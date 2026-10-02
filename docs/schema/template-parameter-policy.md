# 模板參數辨識政策與核可收據

本契約固定 [翻譯契約 §3／§4.2](translation-contract.md#3-模板來源清冊與-id) 的辨識政策載體，
重用 [authored-layout §1／§2](authored-layout.md#1-路徑與共同格式) 的嚴格 YAML、canonical-json-v1
與不可變 Git 輸入。序列化格式由協調者依維護者授權採用，並非維護者逐欄設計或點擊。
文件定義格式與驗證要求，不代表正式 loader、政策資料或模板採納已完成。

辨識政策只回答精確位置的數值／引用角色，不能授權表記等義、提示分類、模板定義或譯本採納。
效果模板定義仍須完整清冊、schema／語義／分叉檢查與實際 human sampled 決定；
譯本另依 [模板採納政策契約](translation-policy.md) 完成互審及真實首輪抽查。
規則確認頁的例子與機械全查不能代替這兩層樣本。

## 1. 獨立入口、配對與不可變

路徑相對 authored 根，只有一對 YAML，**不設 index／review-queue**：

| 路徑 | 完整頂層欄位 |
| --- | --- |
| `template-parameter-rules/<policy_id>.policy.yaml` | `parameter_rule_policy_format:1,kind:template_parameter_rule_policy,policy_id,scope,precedence,rules` |
| `template-parameter-rules/<policy_id>.approval.yaml` | `parameter_rule_approval_format:1,kind:template_parameter_rule_approval,policy_id,policy_hash,events,rules,note` |

policy_id 為永久版本 key，符合 `[a-z][a-z0-9_-]*`；檔名與兩檔內 ID 完全相同。
receipt 路徑只由同 basename 的 `.policy.yaml` 換成 `.approval.yaml` 推得，不接受另一個可任意改指的路徑。
不列入 translations/index.yaml.includes，亦不借 wording、translation、glossary 或 catalog 的收據。
format 僅收整數 1，不收 bool／字串；封套與以下所有物件欄位封閉，必填欄不得省略。
未另標 nullable 的字串須非空、所有布林／整數均採 strict 型別；Code、UInt、Hash 沿 build-db 的共同型別。
各檔完整寫出後 <1,048,576 bytes、目標 524,288 bytes；超限拒絕，不能截掉規則或證據。

loader 完整掃描該 revision 的此目錄，逐對驗證、全域查重 policy_id，拒絕孤立檔、額外檔、巢狀路徑、
symlink（含父路徑）、不安全路徑、未知欄位／kind／格式。缺目錄不能滿足非 null pin；
沒有政策必須由引用端明示 null，不把「找不到檔」解釋成空政策。
新增 pair 先在暫存區完整驗妥、一次發布；中斷留下半對檔就失敗，不猜恢復。

已保存的 policy_id、路徑、兩檔 canonical hashes 及 exact bytes 只增不改。
改說明、例子、收據、更正事件或續版規則集均另配 policy_id／檔名，保存舊 revision；
未變的精確規則可引用原真實事件，不因此要求重點按鍵；舊事件沒有條件物件時依 §3 明示橋接，
不回填當時不存在的 hash 或 commit。匹配條件／角色／範圍／行為變更不能借舊事件擴張，
須先取得相應核可或已明示授權的限制，再寫新版本與可驗證依據。
續版若沿用某原事件，必須完整沿用它的 authorized_rules 的 rule_id 集合；不能靜默漏掉一條。
本格式不表示部分撤回：退掉規則須以真實明示事件重新核可保留集合，另寫新 pair；舊 pair／事件仍保存，
新 pair 不引用已被替代的原事件，也不把新範圍改寫進舊事件。必要時可先停用整份政策（config 明示 null）。
matcher 行為、條件或固定案例變更也須新 matcher_version；只有非匹配說明改字可沿同一精確 matcher。

## 2. 政策本體與兩種 hash

`scope` 恰為 `{region,roles,source_batches,parser_ids,normalizer_ids}`：

| 欄位 | 約束 |
| --- | --- |
| region | 本格式的首輪範圍恰為 jp；不因收據存在就授權 en |
| roles | 非空排序唯一陣列，只收 body／reminder；每條仍須符合自身條件的較小範圍 |
| source_batches | 首輪恰一個 `{store_id,batch_id}`，釘 JP 單一凍結批次；不保存本機儲存路徑 |
| parser_ids | 非空排序唯一 Code 陣列，逐項為有完整 recipe pin 且 loader 支援的來源 parser |
| normalizer_ids | 非空排序唯一 Code 陣列，逐項有完整 recipe pin，含實際使用的分類／參數 recipe；不得 wildcard |

batch_id 與以下所有 Hash 均為完整 `sha256:<64 lowercase hex>`；Git revision／matcher_commit 為完整 40 碼小寫 hex。
每個非 null matcher_commit 必須是 repo 的 main 可達 immutable commit；loader 以被釘住的 main 歷史驗 ancestor，
缺歷史不能略過，也不連網補取。維護者所見若是擠壓前的分支 head，只留當時 matcher_version 與頁面 hash 作證據，
不能拿將消失的分支 commit 當 matcher pin，亦不能說維護者當時看到後來的 squash commit。
變更地域、批次或新增 role／parser／normalizer 不能由檔名、最新來源或 caller config 補出，須新政策及相應授權。

`precedence` 恰為有順序的
`[validate_source,existing_numeric_rules,unowned_pending_only,recognition_rules,reject_conflicts]`。
既有數值規則內部的 sign／identifier／lexical-veto／suffix／prefix 順序及 suffix 優先仍由其條件釘住；
回復／序數被 veto 後不能借 prefix 補回錯誤角色。新規則只能認領既有規則未認領且有相應待審原因的位置；
若兩條認領同一位置或證據衝突，就失敗／保留待審，不依 policy 陣列先後挑一條。

`policy.rules` 非空、按 rule_id 排序唯一，每項恰為：

```text
{rule_id,matcher_version,matcher_commit,recognized_role,match_conditions,condition_hash,finite_transform,examples}
```

- rule_id 為 `[a-z][a-z0-9_]*`，恰對 loader 支援的具名規則；診斷別名不視為正式 ID。
- matcher_version／recognized_role 必須與 immutable matcher 的登錄及該條件相符。
  首輪版本形狀為 `numeric-rule-proposals-v3:<rule_id>` 或 `parameter-rule-candidates-v1:<rule_id>`；
  認得版本名仍不代替內容驗證，登錄內沒有實際事件授權的規則不能加入有效政策。
- match_conditions 是該版本正式程式輸出的封閉條件物件，不是任意可執行 regex 或自由 JSON。
  各 rule 的完整鍵集合／型別／值與程式定義逐項比較，包含 regex、guard、排除、封閉概念集合、
  地域／role、所需原因、原值檢查及完整欄位證據；未知鍵／版本／規則拒絕。
- finite_transform 恰為 `preserve_source`：辨識不改 raw、normalized、符號、名稱、括號或 literal。
- examples 恰有非空 positive／negative 陣列，每例是 §2.1 的完整合成輸入；
  同一 matcher_version 的程式固定案例與政策使用同一組，不僅比案例數。

**condition_hash** 對單條完整 match_conditions 套 canonical-json-v1／SHA-256。
只涵蓋影響匹配的欄位，不含說明文字、status、matcher_version、matcher_commit 或案例。
role／scope 與共同 guard 若影響匹配，必須在該物件中；不能移到說明以避開 hash。
既有八條的條件亦從 `numeric_rules` 正式程式定義產生，備頁腳本不能另組另一份文法。
hash 在受 pin 的 matcher 程式、呈現目前三元組的確認頁、policy 與 approval.rules 間必須一致；
不要求沒有條件物件的舊事件憑空生出 hash，該事件依 §3 保留當時識別並明示橋接。

**policy_hash** 對完整解析後 policy（含 scope、優先序、所有規則、commit、有限變換與正反例）套同一 recipe。
approval.policy_hash 必須等於它；**approval_receipt_hash** 對完整解析後 approval 計算，事件與 note 也在範圍內。
canonical JSON 鍵排序、UTF-8、不 ASCII escape、不正規化 Unicode、緊湊 `,`／`:`、無尾換行。
YAML 排版不影響 canonical hash，但兩檔 exact bytes 另由 immutable Git revision／F1 dependencies 釘住。
改說明可以不改 condition_hash，仍會改完整 policy／receipt hash，不能原地覆寫既有 pair。

### 2.1 固定合成案例

每例恰為 `{case_id,input,expected_match}`；case_id 在該 rule 內唯一，positive 的布林值恰為 true，negative 恰為 false。
input 恰為 `{region,role,field_text,role_spans,slot_segments,original_issues,existing_numeric_rule,reference_candidates}`：

- field_text 是完整**合成**欄位，不保存官方卡文。role_spans 是按位置排序、完整分割欄位的 `{role,start,end}`，
  role 可 body／reminder／token_header／layout；slot_segments 是排序不重疊的 `{start,end}` 非空陣列。
  各 start／end 為 UInt、start<end；座標為原文 Unicode code point 半開區間，須在欄位內；
  input.region 為 jp／en、input.role 為上述四種 role，slot 必須屬案例宣告的 role。
  超出規則地域／role 的合法合成輸入可作 negative，不因此擴張政策範圍。
  adapter 必須用釘住的 partition 從完整 field_text 重算 role_spans，逐區間／role 與輸入比對；
  不一致即拒絕案例，不能用偽造的 body 區間讓提示文引導通過。
- original_issues 是排序唯一的完整 Code 原因集合（可為空陣列，不是 null），existing_numeric_rule 是既有正式規則 ID 或 null。
  adapter 從完整合成輸入重建 hint、重算全部 issues／原值及舊規則認領，再與兩欄逐項比對；
  漏掉 invalid_safe_unsigned_decimal 或另加所需原因均拒絕，不以單一 original_reason 代替 hint.issues。
  它們是驗算用宣告，不是跳過重算的正式來源證據。
- reference_candidates 為合成概念陣列，每項恰為 `{source_name,id,category,record_hash}`。
  需可表達未採納、同名歧義、錯 category／record hash；沒有引用時為空陣列。
- 不執行 YAML 中的程式、不取 live 資料；案例 adapter 須釘版，依完整輸入重建 slot 與 context，
  得到 expected_match 及該 rule 的 recognized_role。invalid evidence 不能算命中。

程式測試與 loader 都須對同一固定正反例核對，並驗與舊規則碰撞、ASCII 尾界、提示文全形正負號、
引導在提示文／錯欄／在後、同欄多引導與不完整標號群、未採納／歧義概念。
新增字形或零實例單位不能只因看似合理就加進正例或範圍。

## 3. 收據：一個事件可以覆蓋多條

approval.rules 非空、按 rule_id 排序唯一，恰涵蓋 policy.rules；每項恰為
`{rule_id,condition_hash,matcher_commit,event_id,presented_in,restriction_ids,note}`。
前三欄是**目前受 pin 的三元組**，逐項與 policy 相同；event_id 指向真正授權此 rule 的事件，
presented_in 指向呈現此三元組的事件，兩者都必須在本檔 events 中。
restriction_ids 是排序唯一 Code 陣列，可為空；每項指向 presented_in.presentation.disclosures 的 ID，
只允許 §3.1 明定的技術限制，不因任意告知或 note 就放行條件變更。
一般事件本身帶 hash 時，presented_in 恰等於 event_id、restriction_ids 恰為空陣列；舊事件橋接另見下文。
不是讓工具把「全核可」套到最新登錄，而是同時保存原授權與目前精確識別。

`events` 是非空的 event_id → 事件物件映射；event_id 為 Code（`[a-z][a-z0-9_-]*`），
由真實事件的 UTC 日期及固定序號命名後永久保存，不由本次執行時間產生；UUID 放 locator，不當重複 key。
approval.note／event note 必須非空 Text；rule note 可為空字串，**不是 null**，相關告知仍必須記入。
每項恰為 `{form,reviewed_by,reviewed_at,reviewed_precision,authorization_basis,presentation,authorized_rules,note}`：

| 欄位 | 完整約束 |
| --- | --- |
| form | conversation_bulk／page_bulk／page_rule，依真正回覆方式；對話事件不能記成按鍵 |
| reviewed_by | 實際核可辨識規則的維護者，不以工具或審核模型代簽 |
| reviewed_at／reviewed_precision | 真實 UTC 事件時間與 instant／day；有毫秒則原樣保留。day 取事件的 **UTC 曆日**，用該日零時編碼，不取操作者本機日期 |
| authorization_basis | 恰為 `{event_locator,source_locator,evidence_hash,statement}`；真實紀錄定位、可為 null 的原訊息定位、保存事件證據 exact bytes hash、完整原話（含保留語氣） |
| presentation | 恰為 `{page_hash,page_payload_hash,presented_rules,disclosures}`；兩個頁面 Hash 可 null（當時沒有相應載體才可），其餘定義如下 |
| authorized_rules | 非空、按 rule_id 排序唯一的當時識別陣列，項目格式同 presented_rules；不得 wildcard／family 名稱代替 ID |
| note | 真實限制與對話／頁面摘要依據，不貼官方原文或私人絕對路徑 |

event_locator／source_locator 用可追溯的訊息 UUID／正式決定定位或 URL，不是私人儲存路徑。
同一 UUID 不得以不同鍵重複成矛盾事件；若保存 queued_command 與原訊息兩個 UUID，須保留其對應，不能算兩次核可。
時間／人名、原話與精確範圍在採納當下對原證據驗回，不讓 coordinator 的轉述變成另一個維護者事件。

presented_rules 是非空、按 rule_id 排序唯一的當時識別陣列，包含呈現但不一定新核可的規則。
每項恰為 `{rule_id,matcher_version,condition_hash,matcher_commit}`，只接受兩種形式：

- 有條件物件：後兩欄均非 null，值為完整 Hash／main 可達 commit；matcher_version 與該 commit 的登錄一致。
- 當時程式尚未輸出條件物件：後兩欄均明示 null，matcher_version 是當時真正呈現的版本；page_hash 必須非 null。
  無 hash 不是漏填／unknown，也不能補算後冒稱當時有呈現。首輪只支援既有八條的
  `numeric-rule-proposals-v2:<rule_id>` 舊事件，其精確 ID 集合與文法由釘版的歷史 adapter 登錄；
  其他版本、混用一個 null 或只有 rule_id 的項目皆拒絕。當時識別可由保存的頁面產物及程式版本驗回，
  不能冒稱舊頁面已顯示這些 metadata 欄位。

此無 hash 舊事件的封閉 ID 集合恰為
`[prefix_field_attack,prefix_field_cost,prefix_field_health,suffix_unit_cards,suffix_unit_entities,suffix_unit_pp,suffix_unit_times,suffix_unit_turns]`；
其餘零使用舊規則不由此授權。§3.1 的限制只連這組 v2 識別與相應 v3 呈現，不能當成通用歷史事件後門。

authorized_rules 每項必須原樣存在於同事件 presented_rules；不能將新版本套到舊頁面。
**每個事件 authorized_rules 的 rule_id 集合恰等於以 event_id 引用它的 rows 的 rule_id 集合**。
若該事件項目帶 hash，row 三元組也必須與它相等；若項目無 hash，只比授權 rule_id，另驗下列橋接，
不得拿 row 的 hash／commit 回填原事件。所有 events 恰被 event_id 或 presented_in 引用，不能留下孤立事件；
本格式事件仍須有非空真正授權集合，不能造一個沒有核可的事件來掛新版呈現。
一份 pair 可保存不同時間的實際事件：先前八條的核可與後來 17 條整體核可是兩個事件，不能互相代簽。

**舊事件橋接**：八條 row 的 event_id 沿原 v2 事件；presented_in 指向後來真正呈現其 v3 三元組的 17 條事件，
後者的 authorized_rules 仍只列當次真正核可的 17 條，不因呈現八條就新增八條授權。
loader 須驗 row 三元組在 presented_in 的有 hash presented_rules 中、版本與 policy 相同，
原事件如實保留 v2／頁面 hash 且授權此 rule，presented_in 時間不早於原事件，
restriction_ids 恰列 §3.1 的 `reminder_fullwidth_sign_exclusion`，限制證據及重播均通過。
同事件、空限制、沒呈現 row 或未知限制均不能用此橋接；收據／事件／policy 三種對照缺一即拒絕。

**conversation_bulk** 允許維護者在對話中一句整體核可覆蓋明列的多條規則。
該事件須能由已保存頁面／摘要、時間序與原話確認精確集合；每條 approval.rules 都引用同一 event_id，
保留同一人名／時間／原話，不虛構八次家族或 17 次逐條按鍵。
例如頁面有 17 條、對話明示全部核可且沒有排除，authorized_rules 恰列那 17 個當時有 hash 的識別；
沒有看完所有例子仍可核可規則，收據須照實寫明依據，不能宣稱逐例審閱。
真正 page_bulk 則引用一次家族／多選操作；可個別取消，集合依實際留下的 ID 列出。未回答者不能入收據。

### 3.1 告知、原核可與樣本限制

disclosures 為按 id 排序唯一的 `{id,text,delivery,evidence_hash}` 陣列，**可為空陣列**（當時沒有告知）；id 是穩定 Code，text 是白話告知，
delivery 限 page／conversation／page_and_conversation，evidence_hash 指實際呈現載體的 exact bytes。
它保存告知事實，不能自行增加 authorized_rules 或捏造新核可時間。
本輪的三句告知須照實保留於 presentation，並在 approval.note／相關 rule note 說明：

1. 前版「個體數」說明誤列「つ」，原核可文法只含「体」；「つ」屬另一條新規則，不由原八條擴讀。
2. 八條既有文法也作用於 reminder，但先前提供的例子都是 body；這不等於提示文的分段／語義已採納。
3. 提示文全形＋／－新增排除，ID 恰為 `reminder_fullwidth_sign_exclusion`，為已定案的安全限制；
   須保存告知與原文法依據、核對其精確差異和舊位置重播，
   不把這項技術限制稱為維護者重新點擊八條。除此明示限制，不允許用「只改排除」繞過條件變更核可。

此例外僅指八條既有文法在未 NFKC 的 reminder 新增全形＋／－排除；
bridge adapter 從 main 可達的 v2 原始程式及其固定案例建立歷史文法，與目前 v3 的條件／行為驗差；
新增 conditions()／hash 輸出只是可追溯識別，匹配差異必須恰為該 reminder guard，不許其他放寬／換角色。
歷史 adapter、對照程式／案例及 restriction ID 的有限驗證都須由 recipe／F1 釘住，不執行頁面或分支 head。
採納當下另驗頁面所對的舊文法與該 main v2 程式相符，不把 main commit 冒充頁面當時呈現的欄位。
完整重播必須證明沒有放行新位置且首輪凍結批次舊認領影響恰為 0；只比總數不算，有任何改變就停下列影響。

若三句只在頁面呈現而沒有另行口頭複述，delivery 恰為 page、note 明示未口頭複述。
相關八條的原話、事件與當時呈現的文法仍保留；安全限制的採用歸因協調者，不冒稱使用者制定序列化或技術改動。
row／政策與 presented_in 所釘的正式條件輸出 hash 必須相符；舊事件的 null 識別仍保留，不能改標成後來的 hash／commit。
原核可、後續告知與明示限制的證據各自可驗，不以 note 單獨代替範圍核對。

此收據**沒有 sample_ids／sampled_items／initial_sample**。
頁面原文例子只留本機私人區，authored 可留頁面／事件 hash 與定位，不保存官方原文。
呈現 108 例、機械驗 483 個 slot 或逐條測試都不是真人看過的數量；不能憑規則核可產生 human sampled 決定，
亦不能把所有 checked 成員當真正首輪譯本抽查。後續兩層採納依各自契約另記實際樣本及事件。

## 4. 五欄 pin、引用與 F1

來源清冊的分類／參數 `recipes[].config` 必含 `recognition_policy`，值恰為 null 或：

```text
{policy_id,authored_revision,path,hash,approval_receipt_hash}
```

path 固定 repo 相對 `authored/template-parameter-rules/<policy_id>.policy.yaml`；authored_revision
是保存完整 pair 的 immutable commit，不能指自身尚未存在的未來 commit；hash 是完整 policy_hash。
兩檔從該 revision 的 Git blob 重取，驗 canonical hashes 及 exact bytes，不讀目前 worktree 同名檔補洞。
source recipe 自身仍有 code_revision／code_path／code_hash／config_hash；config_hash 包含完整五欄 pin。
matcher_commit 是目前 policy／row／有 hash 呈現項目所釘的 matcher revision，必須 main 可達；
authored_revision 是稍後保存政策的 revision，兩者不要求相同，不把前者無條件稱作原事件當時所見。
一般有 hash 核可事件：runtime 比其三元組、該 revision 的 matcher 內容／條件／固定案例，不能只比版本字串。
舊事件沒有條件物件：runtime **只以 presented_in 的有 hash 三元組釘目前 matcher**，驗其內容／條件／固定案例，
再驗原事件 v2 識別／頁面 pin、
歷史 adapter 與 restriction_ids 的有限差異，以及 §3.1 舊位置零影響重播；不與不存在的原 hash 比較。
沒有橋接、main 歷史、必要依賴或歷史重播 adapter 即拒絕；不得呼叫未經釘版的任意 Git 程式。

沒有政策恰為 null，仍可產生 pending_approval 候選；明示啟用候選開關不等於載入核可政策。
只有有效 pair、匹配與完整證據都通過時，才解除**該位置對應**的角色待審原因；其餘原因、來源 unknown、
未採納詞彙／卡名、提示分類與模板分叉仍保留，不以整句任一命中判為完整。
不把辨識 pin 塞進封閉九欄 sentence_template.data，也不將辨識 policy_id 當模板採納 decision.policy_id。

術語依賴在 config 的 references.glossary 釘完整 authored_revision、入口 index_hash 與全部分片的 exact bytes hashes，
沿既有 glossary loader 完整驗入口、成員、採納決定、概念 category／record_hash，從凍結證據重建原樣名稱。
不能僅有索引 hash，或以另一份本機 glossary 借同一 revision 標籤。
F1 dependencies 包含 pair 兩檔、實際 matcher／adapter／parser／normalizer、依賴鎖、完整術語入口及凍結來源閉包；
沒有官方網路、latest cache 或 live manifest 補洞。

私人核可頁面／事件在**採納當下**驗 exact bytes 及範圍，authored 保存 hash／定位與完整明示事件。
後續 CI 可驗結構、完整 pair 與事件集合，不能因沒有私人頁面就宣稱重新看過它；
正式來源重播仍須本次必要的封存閉包，兩層能力分開，結構通過不代替重播。

## 5. 對來源重播的約束

選項編號依賴同一**完整欄位**：有限引導須更早、引導與標號均屬 body，該群恰為連續 1..k、至少兩項，
下個引導切斷前群；保存完整欄位 hash、引導與整組標號的原樣 code point spans，不能借提醒或別欄。
同一舊模板在兩個來源可以一處命中、一處待審，不能把某個 legacy ID 的一次成功當全成員的核可。

術語引用 slot 蓋住 normalized 中**原樣保留的名稱**，不是用 X 替換；大括號等仍 literal。
該模板此 slot 的名稱固定，舊指紋不變；只在閉合概念集合內以唯一 exact 名稱、category、record_hash 辨識，
不靠 NFKC 相似字、ID 前綴、全文 substring 或新能力名稱自動擴張。
四條 keyword_threshold_*（combo／lesson／necrocharge／spell_chain）核對 exact 已採納能力名稱作 context，
**名稱部分留 literal，只有數字是門檻 slot**；不因這四條就核可名稱的術語引用或中文譯詞。

min／max 屬逐模板 parameter_schema 與六欄 payload hash，不屬辨識 policy 或 condition_hash 的語義界值。
沿已定案技術範圍：數量／增減幅度為 0..9007199254740991，序數為 1..9007199254740991；
safe unsigned 原值／序數非零等匹配檢查仍列在精確條件中。正負號保留原文 literal，slot 僅取非負幅度。
零使用的字形、可選分隔符、序數單位或未核可規則不因列在程式登錄內便有授權。

升辨識或**術語 pin** 均須在同一凍結批次逐一比對舊位置
`(inventory_id,slot,raw source spans,numeric_rule,value,raw_hash)`，不能只比總數。
首輪事實值只釘 JP `batch_id=sha256:2bf20b2ff9ae9e21be688dbf5a91d4cd7803fa0e253206ecbdcb55e334cbe9c4`，
policy.scope.source_batches 必須明列此批；14,782 個既有數值位置逐身份不變，新規則與舊位置交集為空；
3,669 舊指紋／13,913 body 使用亦不變。這三個數字不是其他批次的通用常數；新批次須另核範圍並釘住自己的基線。
術語新同名歧義使舊匹配停止是安全方向，但仍須停下列影響，
不能改選另一概念、沿用舊已解狀態或只更新計數當成功。新增位置與剩餘待審原因獨立回報。
指紋重現、來源覆蓋、角色分類完整及正式採納分開驗收；本契約不接 presence v2，unknown 不轉空字串。

## 6. Loader 的兩層驗證及最低反例

| 層級 | 必驗範圍 | 限制 |
| --- | --- | --- |
| authored 結構／Git 驗證 | 目錄完整 pair／歷史不可變、封閉欄位、全 hashes／五欄 pin、event 真實識別的一致性、逐條三元組／原授權 ID 集合／presented_in／限制橋接、main 可達 matcher／歷史 adapter／固定合成案例 | 不需官方 raw 或私人頁面；不宣稱真人閱讀、來源覆蓋或現場事件已重新驗回 |
| 採納當下及來源完整重播 | 原事件／頁面 bytes／範圍與限制、完整來源 recipe／parser／span／raw 值／UTF-8、全部術語證據／完整欄位依賴、逐位置匹配與優先序、升版單調比對 | 缺必要來源或 adapter 即失敗，不從候選說明、live 或另一份本機資料補出證據 |

loader 未完整支援前不得產生正式核可辨識結果或 authored 政策資料。
支援後先完整驗 pair 與來源，再提交當批結果；任一必要 pin、依賴或證據錯誤不得留下半套「已解」產物。
未知來源保持 unknown，未被完整條件涵蓋的位置保持原原因；政策核可不等於移除所有候選 issues。

以下為後續實作的最小獨立驗收，全部合成；不宣稱已有測試或真實收據。多條件格須拆成各自反例，
match 錨定單一完整拒絕訊息，不能靠外層 YAML 格式錯掩蓋目標檢查。

| 編號 | 修改／基例 | 預期 |
| --- | --- | --- |
| R01 | 一個 conversation_bulk 事件明列三條精確三元組，各 row 引用它，無按鍵／樣本 | 可驗規則核可，不產生 sampled 決定 |
| R02 | 漏／多 authorized rule_id；有 hash 事件換 hash／commit；row 不在 presented_in；原 v2 無 version／頁面 hash 卻 row 寫新版 commit；半 null 識別各一次 | 拒絕集合、三元組或歷史識別不一致；不對合法無 hash 舊事件要求不存在的三元組 |
| R02a | 合成八條舊 v2 無 hash 事件＋後續 17 條有 hash 事件，八條 row 沿原 event_id、呈現指後者、限制與零影響重播有效 | 接受原授權橋接；後者授權仍只有 17 條，不能稱重核八條 |
| R02b | 舊事件橋接缺 presented_in／缺限制／未知限制／限制另改角色或放寬／重播改一個舊位置／matcher commit 僅分支可達各一次 | 拒絕；證據少一環或不是明示有限限制就不可借原事件 |
| R02c | 續版沿原事件卻少一條；另一份新 pair 以真實新事件核可剩餘集合且保留舊 pair | 前者拒絕靜默部分撤回，後者可驗新的明示範圍 |
| R03 | 把同一對話寫成多次點擊／原 UUID 改成新版本三元組或另配矛盾內容／reviewer 或時間不同／冒稱舊頁面有 hash／把後續呈現當新增授權各一次 | 拒絕不實歸因；保留原事件版本與 page hash 的合法橋接不屬改寫事件 |
| R04 | 缺一檔／孤立額外檔／重複 ID／symlink／路徑穿越／檔案恰 1 MiB 各一次 | 拒絕完整入口 |
| R05 | 同 policy_id 換排版 bytes／換收據／caller pin 換 hash／未知 kind 或 format=true 各一次 | 拒絕不可變或封閉格式 |
| R06 | condition 加未知鍵／改排除／record category／固定正反例／只保留相同版本名各一次 | 拒絕 matcher 不一致；說明修改只不影響 condition_hash，仍需新 pair |
| R06a | 合成案例偽造 body role_spans；issues 漏 invalid_safe_unsigned_decimal／加不存在的所需原因；既有認領欄與重算不同各一次 | 拒絕案例宣告，不能靠輸入 hints 跳過 partition／完整 issues 重算 |
| R07 | 新規則碰到舊認領／兩條爭同一位置／回復序數被 prefix 補回各一次 | 不可改角色或先到先得 |
| R08 | 引導在提醒／另一欄／在後，第二群不完整，標號缺／重複／逆序各一次 | 未通過完整欄位證據，不確認角色 |
| R09 | glossary index 相同但分片 bytes 不同／revision 標錯／新增同名概念各一次 | 前兩者拒絕 pin，第三者重播停止並列影響 |
| R10 | 不明示 null／候選開關冒政策／來源 unknown 冒 absent／任一 slot 命中就清全部 issues 各一次 | 拒絕或保持原待審，完整性各自計數 |
| R11 | 只驗 14,782 總數卻交換兩個 slot 身分／換 raw value／span 各一次 | 逐位置重播拒絕 |
| R12 | 把呈現例子寫 sample_ids／disclosures 冒口頭複述／借本收據採納效果定義或譯本各一次 | 拒絕樣本與授權混用 |

本入口不改 SQLite DDL、translation record kind／九欄 payload、公開快照格式或建置／preview 接線。
