# 數位官方卡名與同名瀏覽規則

名字使用[翻譯當前資料格式](translation-contract.md)與下列可修改規則，不需核可頁、approval 或點擊紀錄。
同名瀏覽與真人數位卡關係仍是不同能力；名字規則不改卡片身分或人工同卡對應。

## 1. 名稱採用與關係分開

只處理 JP 面自己的 face_revision.name／已知 printing_face.name，取繁中名稱。
SVE 與數位完整日文名稱逐字相同，不 trim、normalize、casefold 或刪字；hash 相同仍比 exact 字串。
完整數位目錄中同名 ID／各實際 phase 的繁中名須非 null、非空、非純空白、通過最低文字檢查，
且恰一個 exact 譯名；不能只挑合格的一筆。JA／繁中同字但全漢字仍可用。

取詞順序仍為 sv1 → svwb；只有 sv1 完全沒有該 exact 名稱才自動試 svwb。
一代有名但缺譯／異譯時，不借二代跳過歧義；可用 owner 自己既有的有效真人同卡精確面關係供名。
該關係的資格仍按 digital-links 契約驗，不把名字資料的低信心或 PR 接受當成新同卡證明。
背面不借正面，unknown printed 不借 current，舊印刷名用自己的原文。

選用優先序為：明示卡名選詞覆寫 > 合格的直接官方名 > 自己有效同卡精確面供名 > 其他有效 glossary 選詞 > 原文。
明示覆寫以 `name_overrides` 指定精確 owner／source_hash／term_id，不從 sample_ids 推測。
轉換必須把舊優先級的實際勝出結果保留下來；官方與專案詞的單純字面差異列報告，不因此全部回原文。
真正語義歧義列清單；同名瀏覽不供名。origin=official 不表示官方實體繁中版，顯示標「數位版官方卡名」。

## 2. 同名規則瀏覽連結

獨立核可的連結政策以非空 exact 日文名先視為同卡，新增待能力實作的 `relation=same_name`。
不沿用 `same_card+confirmed`；既有真人 same_card／same_character／name_only 的定義與門檻不變。
所有同名數位 ID 都列，兩代全列，UI 先 sv1 再 svwb、各代按原始 ASCII official_id 排序；wire 仍依既有 link.id。
同一實體卡到同 game／official_id 恰一筆；subject 恰為 `{card_id,face_id:null,game,official_id,digital_phase:null}`。
不同面／印刷名稱可提供各自證據，但不展開面×phase，也不推測前後面機制對應；保留所有實際匹配來源。
完整目錄可能命中多張，不表示已確認異畫；繁中缺譯、職業／卡種／圖不同不阻擋這個瀏覽規則。

same_name 只授權瀏覽，永遠不供官方名稱、同概念、效果等義、DSL、圖或語音。
畫面用「同名規則視為同卡，未逐筆確認」。公開四種 review_level 不增值，此類固定 unreviewed；
typed 政策投影分支不能呼叫 `Source.review()` 把內部機械 confirmed 投成真人 confirmed。
application 可沿 decision FK 記 approved_rules／完整 checked_by_rules，但 sample_ids=[]、human_checked=[]，
核可者／核可時間與工具套用者／時間分開。沒有完整能力前不產生這種 application 或正式 link 分片。

fresh 真人同 card/game/ID 的任何合法關係優先，抑制同組重複規則連結。
真人負面、stale 或撤回不被一般規則復活。真人撤回後若要放行該對規則連結，僅維護者明示指定卡對，
沿既有 rule_resume 收據釘原 terminal 的 record_key／hash／decision_id、政策 hash、卡對、
真實訊息 UUID／instant、理由及前件／結果 hash；不恢復真人採納，不繞來源／排除，新 terminal 再變須重新放行。
政策卡層轉真人卡層且完整 subject 相同時沿原 ID；真人精確面是不同 subject，另 ID 並抑制原規則列。
ID 仍為 `dl:`＋H(`["digital-link-v1",subject]`)，relation、時間、政策／清單 hash 不參與。

首批 coverage 不採納，公開 `digital_link_coverage=[]` 是未知，不能表示查無。
未來完整 coverage 的 links 集合含有效規則與真人結果，但 same_name 不等於真人查完或 reviewed_none。
草稿 93／2,041 約 4.6% 是警訊而非誤連率：84 同角色、8 兩代判斷不一、1 只有同名，全部在 232 待確認內。
維護者選「照規則連」，初始排除空；不改草稿原關係、不把它們記成真人 same_card，錯連數仍未知。

## 3. 可修改的名字規則入口

| 檔案 | 完整頂層欄位 |
| --- | --- |
| `digital-name-policies/index.yaml` | `digital_name_policy_index_format:2,kind:digital_name_policy_index,policies` |
| `digital-name-policies/<policy_id>/current.yaml` | `digital_name_policy_format:2,kind:digital_name_policy,policy_id,purpose,content,origin,low_confidence,note` |

新 names 項的 policies 值是 `{path,hash}`，hash 是 canonical 檔案完整性檢查，不是核可文件 hash。
purpose=names；content 恰為 `{scope,game_priority,target_minimum_check,excluded_names,name_overrides}`。
scope 恰為 `{field,owners,region,source_lang,target_lang}`，固定 name、
`[face_revision.name,known_printing_face.name]`、jp、ja、zh-Hant；game_priority 恰為 `[sv1,svwb]`。

target_minimum_check 恰為 `{kana_ranges,whitespace_codepoints,trim_or_normalize}`；
兩個 Unicode code-point 表從舊有效規則無損轉入，trim_or_normalize=false。
範圍須合法且不重疊，程式依該表自動檢查，不讀私人測量報告或 known_limits 敘述來決定行為。
excluded_names 是按 source_name_hash 排序唯一的 `{source_lang,source_name_hash,reason}` 陣列；
source_lang=ja、hash 取完整 exact 名稱，reason 非空，不把整份官方名字表複製進 Git。
排除只停自動官名，不撤真人數位關係；不能借真人 link 繞回自動取名，明示選詞覆寫仍可用。
name_overrides 是 `{owner,source_hash,term_id}` 陣列：owner 用翻譯契約的具名 owner，
source_hash 必須等於自己的 exact 名稱，term_id 必須存在且為 card_name；同 owner／來源不能有兩個指派。
既有資料來源 hash 在此防錯配，不是新的核可收據。

資料可直接修改，note 可省略並預設為空字串，來源類別／低信心沿翻譯契約；不保存人名、日期、事件、私有檔名或頁面 hash。
規則只保留實際取名所需條件，去掉歷史答案、呈現文件、批准章節與 approval/exclusions 的配對封套。
輸入目錄的版本由建置配置與 CI 私有資料鎖定檔提供，不嵌每筆核可時的機器／程式閉包。

同一 index 在過渡期仍可含原 format 1 的 links 版本陣列，由舊格式 reader 處理；不得把它當 format 2 names 解析。
本次只轉名字資料，§2 同名瀏覽及真人 digital-links 的非翻譯採納不隨之放寬。
舊 names policy／approval／exclusions 僅供取出有效條件，轉換完成後不再是當前入口的必要檔案。

## 4. 建置與公開邊界

讀取只做結構及索引檢查；本次建置自動驗完整目錄、來源語言、owner、最低文字條件與排除，生成名字。
不重新播放舊核可／目錄歷史，不要求先把同一 PR 的程式 merge 才能寫資料。
來源壞掉失敗，無合格候選回其他有效詞或原文並列清單；低信心譯文依翻譯契約直接顯示待校對。
名字、glossary 概念、圖／語音與效果資格仍各自檢查，不能以新 origin 列舉取代它們。
直接官方名字的 render-v2 dependency_key 只含所選 game、exact JA／繁中 hash 與取名 recipe 語義；
不含後補概念、owner、整批目錄或規則檔案 hash。補概念不讓普通名字整批換 ID；
來源文字／選詞／取名語義改變則重算。只用名字不要求發布 digital_link／圖或整份數位目錄。

## 5. 自動檢查

至少驗 exact 名稱、兩代順序、同名異譯、缺 phase、錯卡面、未知 printed、排除與覆寫、缺譯回原文。
CI 固定輸入可重新產生相同名字及診斷清單；測試不需重建點擊或核可網頁。
公開 provider／authority 由實際來源投影，與低信心欄位的 producer／reader 版本一起更新，不能冒作真人逐卡已審。

## 6. 非翻譯 links 保留的 format 1 契約

下列保留既有 names/links 共用載體的定義，**只有 purpose=links 繼續適用**；
names 的 format 1 描述僅供轉換，不是 §3 當前名字規則的門檻。index format 2 可容納下列 links 版本陣列。
本節保留來源與排除條件，不讀核可收據，也不以核可事件作載入門檻；不能將非翻譯連結移用到翻譯。

### 6.1 政策與可攜入口

沿 authored-layout 的嚴格 YAML 1.2、安全路徑、全入口查重、canonical-json-v1 與單檔 <1 MiB。
未知欄／格式、Bool 當整數、symlink、缺檔、孤立／未索引檔、hash 不符、分叉或缺號均拒絕。
版本從 `001` 起只增，既有 policy／exclusions 的內容不改；核可收據及索引的 approval_receipt_hash 已移除。

| 路徑（相對 authored） | 完整頂層欄位 |
| --- | --- |
| `digital-name-policies/index.yaml` | `digital_name_policy_index_format:1,kind:digital_name_policy_index,policies` |
| `digital-name-policies/<policy_id>/<version>.policy.yaml` | `digital_name_policy_format:1,kind:digital_name_policy,policy_id,version,purpose,approved_document_hash,projection_recipe,content` |
| `digital-name-exclusions/<policy_id>/<version>.yaml` | `digital_name_exclusion_format:1,kind:digital_name_initial_exclusions,policy_id,version,purpose,approved_list_hash,entries` |

policy_id 沿實際核可文件，不因名稱帶 draft 就當未核可；version 為正整數，路徑序號至少三位。
purpose=names/links。policies 是 policy_id → 非空版本陣列；每項恰為
`{version,path,hash,exclusions_path,exclusions_hash,predecessor}`。
path／exclusions_path 固定符合上表與 ID／版本；predecessor 首筆 null，後續指上一 entry 的完整 canonical hash。
所有 Hash 是完整 `sha256:<64 lowercase hex>`。消費端 authored_revision 釘已合併後 main 的索引及全部歷史政策與清單，
索引不引用自己的未來 commit；完整 bytes 另進 F1。

**核可文件 hash 與 authored 操作政策 hash 分開**：維護者核可的是私人保存的完整呈現文件，含歷史題目／本機追溯欄，
不得刪路徑、回填核可事件或換封套後宣稱他核可新 hash。`approved_document_hash` 始終釘該完整 parsed canonical hash；
`policy_hash`／index.hash 是本表操作政策完整解析值的 hash。保留兩種內容 hash，不混用，不產生 hash 自我引用。

projection_recipe 恰為 `approved-digital-name-document-v1`，content **逐欄原值複製**下列集合，不能改字或改規則：

| purpose | content 恰含的完整核可文件欄位 |
| --- | --- |
| names | `policy_id,actual_answers,normative_rules,plain_language_rules,catalogue_pins,scope,game_priority,target_minimum_check,display_label,excluded_names,visible_notices,shared_answer_bindings,shared_future_scope_rules,final_exclusions_hash,proposed_clause_replacements` |
| links | `policy_id,actual_answers,rule,review,limits,precedence,exclusions,ids,catalogue_pins,registry_pins,warning_disclosure,coverage,publication,f1,versioning,plain_language_rules,visible_notices,fixed_receipt_disclosure,shared_answer_bindings,shared_future_scope_rules,proposed_clause_replacements` |

只省略既有文件的呈現題目／草案狀態／歷史流程／本機背景路徑等非操作欄；完整已核可文件留私人持久證據。
採納當下先驗其 canonical、真正呈現白話全文、頁面與按鍵，逐欄驗投影；不能藉新封套／recipe 添加新授權。
之後 build／CI 驗 authored 的操作政策、清單與來源閉包，不重讀私人核可頁，也不讀核可收據。
缺 supported projection recipe 即拒絕使用；內容雜湊只檢查資料完整性，不證明真人核可。

content 保留當時文件原文，包含歷史題目、收據及 proposed_…_not_merged 等措辭，不為清除流程欄位改寫規則內容。
這些歷史文字不是載入門檻；條文是否已合併以正式 docs 為準。
任何操作文字、matcher、枚舉、scope 或來源語意改變須另核可；來源／清單有限續版依 §6.3，不借純 metadata 投影改規則。

### 6.2 初始清單

初始排除 entries 恰等於政策內容的完整最終清單。
approved_list_hash 釘該清單 canonical，exclusions_hash 釘可攜清單完整解析值的 canonical。
政策與清單只保存規則所需內容；官方目錄、卡名表及私人核可頁不進 git。
names entry 為 `{source_lang,source_name_hash,reason}`；links entry 為
`{kind:name,source_lang,source_name_hash,reason}` 或 `{kind:card_target,card_id,game,official_id,reason}`。
完整鍵排序唯一，理由非空；清單可以空但必明示，不為空清單另造操作事件。

### 6.3 續版與排除

第三題 agree：新 JP 凍結批次自己的 owner 符合有效政策／指定目錄，就沿用，逐批報新增，不冒稱真人逐張看過。
第四題選 **乙，兩政策共用**：純新增、完整符合各自條件、舊名稱／資格／取名遊戲／既有連結與對應未變，
無新增兩代異譯、排除或真人決定衝突，才可有限機械續版。兩代異譯、譯名改變、失去資格、既有對應變動、
目標消失須經 PR 審核；較新建置批次只報差異，不使原政策 stale、不偷偷換目錄。
有限續版仍釘完整新批次、精確前件及完整檢查報告，不把原事件寫成他看過新批次；
**有限續版的封閉格式與 loader 尚未完成，不先啟用自動新增**。

names 排除不允許借真人 B／規則 link 繞回自動官方名，但本人實際採納選詞仍可用；
它不撤真人關係或獨立規則連結。改 names 清單須經 PR 審核完整新清單與差異，並證明其他規則／目錄未變。
links 排除只移除規則證據：同名排除不移除其他合法名字對同卡對的依據，整對排除才移除所有名依據。
真人有效關係照自己的決定顯示，重疊只報告不停出貨；要撤真人另走真人撤回。
兩份清單及 hash 各自獨立。後續 links 新增或移除直接修改政策或排除清單並經 PR 審核，
Git 歷史保留修改紀錄；不寫收據，也不記維護者訊息 UUID 或 instant。
修改規則／遊戲範圍同樣須經 PR 審核；未支持續版格式不得靜默套用。

### 6.4 來源與能力啟用

歷史背景由宣告的不可變 Git 版本驗，不與目前磁碟執行期比較；正式來源基準用 main 保留的 commit，
不能用會被 squash 掉的功能分支 commit。目前程式／parser／checker 的完整依賴另釘並重驗使用，
加註解或重構不讓歷史政策失效；raw／父層／閉包錯仍建置拒絕。
F1 包含政策索引／完整政策／清單及鏈、所有 raw descriptor／來源／parser、逐 owner 與實際使用；
完整 `record.verify(...,complete=True)` 不能由子階段 partial verify 代替。

純名字政策不要求發布數位卡／圖／link；同名瀏覽才啟用最小 digital_card／digital_face／digital_text／
digital_link／digital_endpoint 閉包，不因查整份目錄把所有數位內容公開。
same_name 啟用前由程式 PR 同步 DDL、公開 relation 白名單、typed projector、fixtures 與 reader，
新增枚舉以 minor＋`digital-same-name-links-v1` required capability／min_reader 拒舊讀者；改欄序或既有意義需 major。
review_level 原四值不變。正式檔案／表列不能在 docs-only PR 先改成已支援，舊快照不回寫。
