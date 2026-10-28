# DSL 1.0 入口容器

DSL 檔與伴隨本體共用 `author-nodes.schema.json` 的能力內容圖。`author.schema.json`／`effect-bodies.schema.json` 各由固定 format/kind 選取，逐卡 dsl 只驗支援集；不得依 dsl 猜「最新」資源。原型 effect_set 不在本 reader 的DSL 入口。

此 reader 只驗結構、inline 參數殘留及本體鍵／registration id 唯一性。第一版不執行 review 資格、用途依賴排序／追蹤、rekey 或實跑 DB 驗證；證據欄位保留封閉形狀與 DSL 原序。來源 registry／現行版次／FrameId／Port 閉包、索引候選雙向閉包、資格與引擎准入仍是其他驗證階段。generated_by 展開與 body hash 由下述明示展開 API 核對，不由結構 reader 隱式執行。`status:verified` 是DSL 聲稱，不構成可執行資格；非空 `verified_by_exam` 尚無封閉格式，現階段報 unsupported_exam_evidence。

## 容器矩陣

| 入口 | 必填 | 選填／互斥 |
| --- | --- | --- |
| DSL 檔 | format:1、kind:author_effect_set、scope、cards | 無其他頂層鍵；format／kind 檢查後 en_override 報 en_source_contract_unavailable |
| A.card | meta | note、abilities、faces、deck_rule；不附卡面數值 |
| A.face | abilities | faces 的 key 是永久 f:32hex；不再嵌 meta／faces |
| inline 能力 | line、sources、ports、恰一個能力頭 | section、非空 imports；禁止 generated_by |
| generated 能力 | line、sources、generated_by | section、非空 imports；禁止 ports 與能力頭 |
| 本體分片 | format:1、kind:effect_body_shard、scope:shared、records | 無其他鍵 |
| companion | frame_id、ability_scope、body_version、dsl、params、body、ports、written_by、registrations | note；body 不含用途位置或 generated_by |

sources 恰一筆，OccurrenceKey.segments 可以保留多段，不複製能力執行。普通能力／generated_by 使用 SourceUse，含真實 ability_scope；獨立 keywords 宣告只用 CardKeywordSourceUse `{binding_id,occurrence}`，不含 ability_scope。keywords 宣告的 ports 必空，imports／generated_by／舊 SourceUse 均拒絕；companion.body 的子圖排除 keywords。line 從 1 起算，section 從 0 起算。通用 OccurrenceKey 的四層 owner 型別包含 Q&A 等來源，結構可讀不表示這是合法 JP DSL 來源；shared 來源准入仍需已採納 JP face_revision 及精確 owner／field／face／line 與該種類的來源定位。

含獨立 keywords 宣告的卡目前只接受 draft、reviewed_by:[]、verified_by_exam:[]；不新增非本體用途 hash／review recipe，不使用本體 body_version 代填。正式 current reader 已對 ward／aura／storm／rush／bane／intimidate／assail 完成來源與作者整卡對帳，使用獨立 declaration sidecar 與分型非本體 inventory；D0 分別精確消耗本體與宣告義務。純結構或外供 context 展開不授 current proof；來源對帳不代表 executable lowering、runtime 或用途資格已通過，封閉執行邊界見 author-contract-1.0〈守護與光紋的具名宣告准入〉。

inline 的能力 payload 與 companion 使用同一型別圖。單跑 Schema／TypeAdapter 可能接受合法葉位的 ParamRef；完整結構 reader 另走 decoded 型別圖，對 inline 內任何 ParamRef 報 inline_param_reference。走訪透明 RootModel、保留 BaseModel 的序列化 alias 與 TypedDict wire 鍵，不掃全域同名鍵，因此不把依賴 params 誤認成能力參數。這項 inline 檢查不取代下述完整 expanded profile。

ability.imports 若出現必非空，沿 P1「沒有 import 時省略」；Ports.imports、Ports.exports、UseDependencies.imports 皆必填且可空。A.card 的 abilities／faces 及 A.face.abilities 沒有額外非空要求；合法省略與空值各自保留DSL bytes，不推定同 hash。

## 證據與局部 invariant

Actor 恰為 who/model，model 可以 null 但不得省略。written_by 非空，日期是真實 YYYY-MM-DD。BodyReview 只放 registration，UseReview／MacroMechanical 放 meta；機械項不虛構 reviewer，其 use_dependencies 只收引用用途（applicability_scope／domain_hash 非 null），inline 依賴不能取得機械資格。body_boundary 屬測試輸入與建置 DB，與逐用途 engine exam 不同，不在這兩種 authored 容器中。

UseDependencies.imports 值恰為 binding_id／ability_scope／port／dependency_key；後者是 P1 effect-use-v1 Digest，不內嵌遞迴上游快照。Rekey 保存完整 before/after 依賴，不表示機械延續已被驗證。

| invariant | 精確範圍 |
| --- | --- |
| 原序保留 | UseDependencies.sources、written_by、reviewed_by、registration.reviews、registrations、meta.rulings 及所有能力陣列 |
| registration id 唯一 | 每筆 companion |
| companion key 唯一 | 同檔 frame_id／ability_scope／body_version；跨分片另驗 |

## 診斷與驗證範圍

`AuthorSchemas.document` 依 kind 分派並讀取兩種完整容器；不開啟索引指定檔案。`author_tree`／`body_tree` 完成相同驗證後回傳嚴格 loader 的原 JSON 樹，保留合法選填欄位與順序，不由 model dump 重建 hash 輸入。`document` 可帶已解碼的 `AuthorIndexRecord` 作為 context；YAML／JSON 解析失敗仍保留其 card_id、record_key 與 scope。若錯誤位於另一張卡，不借所選卡的 card_id。

診斷帶 stage／reason／DSL 結構 path／可得 YAML line、column。source line 與 YAML line 不混用；來源定位僅取已驗型別。錯型別只帶 expected/actual 類別，不附 input_value 或卡文；parser 失敗只帶安全類別。未知 key、版本、非字串鍵、重複鍵、非 JSON 純量、unsafe int 與浮點皆拒絕。合法 alias／merge 展開，無直接 cursor 時取最近父容器。

能力頭診斷使用 authored profile 的 Schema 定義；同一 mapping 先報多頭的 `ability_head_conflict`，再報未知鍵並定位到鍵，最後報無頭的 `ability_head_missing`。只有 Schema 已拒絕的輸入才走此診斷，不改接受集合。

## 檔案集合與 current 來源

`load_author_set(authored_root, schemas)` 先讀 `rules/effects-index/<卡包>.yaml`，再嚴格遍歷作者、本體與原型區；拒絕符號連結、非一般檔案、非法路徑與達 1 MiB 的檔案。DSL 新分片路徑為 `<卡包>/<record_key>.yaml`，既有數字分片不改名；本體路徑仍為 `<owner>/<3位以上序號>.yaml`，原型為 `<owner>.yaml`。所有 DSL 檔及被索引的原型檔，其 cards 與 index 必須雙向閉合；未索引原型仍全部驗 astra/1，不要求既有原型先遷移。全部 companion 核對版本與跨分片唯一鍵，無須已被引用。

只有DSL 與本體目錄都不存在時，缺 index 才表示空DSL 集合。`author_inventory_tree` 對 shared 做完整結構驗證；對 en_override 只讀封閉 envelope 與 cards 鍵供索引閉包，內容不取得 typed DSL 樹，之後逐卡回 `en_source_contract_unavailable`。`prototype_tree` 是獨立入口；`document` 不會自動改用原型 reader。

`CurrentAuthorSources.validate(authored_root, expander)` 自行載入根目錄內的完整檔案集合，對每個 index `(card_id,scope)` 回一個 `CardAccepted` 或 `CardRejected`。外部自行組裝或刪減的 `AuthorInputSet` 不能用來取得 current 證明；`validate_author_set` 只重驗提供的 bytes。檔案／Schema／索引閉包、本體 registry 與原型重疊是全域錯誤；普通來源、未支援構造及 EN 錯誤逐卡拒絕，不影響同檔其他卡。原型的全部卡鍵必須能經 JP printing 唯一解成永久 card_id，不能略過未解身分；shared 原型與 shared DSL 檔不得重疊。DSL 記錄核對全部同號 JP printing，暫定號拒絕。

current 邊界重建真實 registry 的 Frame／Binding，核對全部 JP current 面、exact owner／field／section／line／root scope 及完整來源根集合，並驗所有 companion 的 resolved ability_body／root scope／ports，包括未引用本體。能力屬於哪一面，由 SourceUse.owner 的 revision 對應 face 決定；DSL 檔級／面級容器只表示位置，多面卡的卡級容器不表示全卡常駐。面容器另驗精確 FaceId，跨容器重複同來源 scope 拒絕。

`effect-source-v1` 依 P1 完整 payload 計算；純 hash 函式不授予來源准入。DB 的非 null 數值／代碼與建置保證完整的種族、稱號、特殊類型集合可判 known；type_code 為 spell 時，null attack／defense 依 P1 的規則確定不存在判為 known-null。type_code 為 follower 且同一 revision 已採納的 special_kinds 含 evolve 時，null cost 也依 P1 §4.2 記錄印刷費用確定不存在；advance 或普通 follower 不適用。leader 只有 DB cost／attack／defense 全為 null，且同一來源 descriptor 的凍結 document 明示 cost／power／hp 三項皆為 `-`，才把三項標為 known-null；其他 null 仍是 unknown，不推成 0。

精確空字串 effect 只有原 `translation-jp-v1` 投影、同 batch／version／face 的已驗 presence 為 absent 時，才記為 absent 與空字串 hash；缺證據、未驗證或不符的來源不得借用其他面的證據，空 effect 仍不能取得准入。非空 effect／section 不改來源、FrameId 或 BindingId。沒有任何 effect／section 編譯欄位且完整來源已知的空白 token 可以使用空 abilities；有文字的 token 仍須閉合其來源義務。token 依賴未閉合時不產完整來源 hash，也不把缺少 token 選用結果當空集合。

Rust 對照 current 面與快照時，只有 effect 的 absent 加上精確空字串 hash，才能匹配快照中缺少、null 或精確空字串的效果文；非空文字、錯誤 hash 或把 absent 用於 name 仍拒絕。面對應仍須唯一且雙向閉合，不藉此採納其他版卡文。

進化面的費用證據以已審 [ECP01-002 解析輸入](../../card-data/jp/ECP01/000-099.json)為例：document `documents-4a1197c7caf47383` 的 card_type 是 `フォロワー・エボルヴ`、cost 是 `-`；正式建置映成 follower／evolve 與 null cost。[日文 CR 1.28.0（2026-09-28）](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf) §2.3.3.1 區別 EVOLVE 特殊類型，§5.16.1.2 規定場上進化後的費用仍沿用原卡。這裡記錄的是進化面印刷值的缺席，不把 null 改成 0、不推算場上費用，也不授予普通打出資格。

| resolved 來源族 | scope → DSL 頭 | 接受的完整 payload |
| --- | --- | --- |
| evolve_entry.v1 | ability_0 → evolve | `evolve: {cost: {pp: N}}` |
| feed_entry.v1 | ability_0 → feed | `feed: {cost: {pp: N}}` |
| ride_entry.v1 | ability_0 → ride | `ride: {cost: {pp: N}}` |
| evolve_feed_entries.v1 | ability_0 → evolve；ability_1 → feed | 各 scope 的獨立來源 cost |
| evolve_ride_entries.v1 | ability_0 → evolve；ability_1 → ride | 各 scope 的獨立來源 cost |
| draw_exact_spell.v1 | ability_0 → spell | `spell: {do: [{draw: {who: self, n: N}}]}`；單一 required Nat/count/value 葉 |
| draw_exact_attack_time.v1 | ability_0 → attack_time | `attack_time: {do: [{draw: {who: self, n: N}}]}`；單一 required Nat/count/value 葉 |
| draw_exact_evolve_time.v1 | ability_0 → evolve_time | `evolve_time: {do: [{draw: {who: self, n: N}}]}`；單一 required Nat/count/value 葉 |
| destroy_fanfare_opponent_all_field_followers.v1 | ability_0 → fanfare | `fanfare: {do: [{destroy: {ref: {who: opp, zone: field, kind: follower}}}]}`；固定 Fanfare Concept，無參數 |
| damage_uniform_lastword_all_leaders.v1 | ability_0 → lastword | `lastword: {do: [{damage: {to: {leaders: all}, n: N}}]}`；固定 Lastword Concept 與 Nat/damage_amount/value 葉 |
| move_self_to_owner_cemetery_activated_pp.v1 | ability_0 → activate | `activate: {zone: field, cost: {pp: 1}, do: [{move: {ref: this, to: cemetery}}]}`；三個來源葉皆固定，無參數 |
| search_lastword_self_deck_named_hand.v1 | ability_0 → lastword | `lastword: {do: [{search: {sel: {who: self, zone: deck, name: 式神の使役}, n: N, to: hand}}]}`；唯一可變葉為 QuantitySpec/selection_count/constant 的 exact 數量 |
| numeric_fanfare_other_allied_followers_add_1_1.v1 | ability_0 → fanfare | `fanfare: {do: [{buff: {on: {who: self, zone: field, kind: follower, other: true}, power: 1, hp: 1}}]}`；五個 typed 葉固定，無參數 |

表列各族皆只配空 imports／exports ports，能力 imports 必須省略。去掉 line／section／sources 後完整 canonical 能力必須相等；多加 into、usable_if、times、其他費用或 expression 都不符來源。N 可為 0，須取該構造的 operand，不能借相同數值。非空 ports 與其他執行 scope 保持明示 unsupported；不把跨 owner 或DSL 陣列較晚定為永久非法。真卡整卡正例為 BP01-009／034／092 的純進化入口；feed／ride 與雙入口的有限機制測試不宣稱已有真卡整卡准入。

spell、attack_time 與 evolve_time 抽牌同樣只配空 ports、不得有能力 imports。spell 來源重建要求既有 normalizer 的完整 current normal spell effect、無 sections、單行 exact self 抽牌；attack_time 要求 current follower effect 的完整獨立 `【攻撃時】N枚引く。` body 行，自身攻擊事件固定在頭內；evolve_time 要求 current evolved follower effect 的完整獨立 `【進化時】N枚引く。` body 行，固定自身進化事件，與 evolve 費用入口分開。只對照來源 Nat/count/value 到 draw.n，不參數化 actor、能力頭或控制圖。`expand_contexts` 與 current 入口都比對完整展開能力的 canonical bytes；省略 who、改張數、增加條件／費用／步驟或換頭皆拒絕。真卡正例分別為 CP02-068、BP10-122 與 ECP01-002；up_to、對手／雙方、動態量、Quick、token 位置仍拒絕。同卡有其他 sibling 時必須逐一接通並驗證，不能只截出抽牌就准入整卡。來源准入不等於 reviewed、compiled、executed 或 qualified。

`destroy_fanfare_opponent_all_field_followers.v1` 只接已採納來源規則的完整 Advance 面：current JP follower、special_kinds 恰為 advance，effect 第一行守護／光紋、第二行完整入場曲，無 sections。固定 Concept/ability 葉、canonical occurrence、body 來源位置與空 ports 全部核對；inline 與 companion 都比較完整能力 bytes，不參數化 selector、能力頭、條件或費用。Fanfare 保留來源持有者由場外新進場的事件語義，不限普通打出，也不因卡型補造 AdvanceActivate。對手以能力 master 為準，destroy 的移動目的地依各卡 owner；不改成 banish／move，也不宣稱忽略破壞禁止、Advance／token 後續規則。

BP19-P32／BP19-114 同永久身分；完整 current 入口分別保留 `card_keywords.v2` 的守護／光紋非本體義務與入場曲本體。測試以公開來源重建完整選取，通過 D0、整卡准入及合法效果入場後的守護選擇與入場曲結算；不在同一選取重複註冊兩個印刷。正式 authored 尚須依遷移流程另行處理，不能用純來源展開的 derived_self_consistent 冒充用途資格。

具名謝幕曲搜尋只接受 current 普通 follower、無特殊種類與 sections 的完整單行 effect；
真卡為 BP06-048 與同永久身分的 BP06-P15。固定 Lastword Concept、自方 deck、目的 hand、
省略的自方目的 owner 及 CardName 引用 `term:name.shikigami_employment` 均逐葉核對，
並驗其日文原文恰為「式神の使役」。CardName 的 name_reference 角色不能當 name_filter
參數；只有 leaf_4 的 exact constant 映至 search.n。inline 與 companion 都比較完整能力，
換頭、換對象／區域／卡名、up_to、動態量、追加條件／費用／步驟、漏 sibling 或錯 owner
均拒絕。一個 Search 節點保留私有觀察、找到後先證明再移動及最後洗來源牌庫一次的義務；
非公開具名搜尋可找不到，但不改成 up_to，也不另加 reveal／move／shuffle 步驟。
沒找到或未成功移動仍須洗牌；self 取離場前能力 master。這些語義依既有搜尋與 Lastword
契約，本來源 adapter 的測試只證來源展開及 current 准入，不宣稱 D0／整卡執行或用途資格。

固定內容種子：BP08-038 的 Live count−5 與 draw；CP04-060 的 search→follow→本回合 cost delta −2；ECP01-058 的 evolve deck_rule 10；BP09-SL05 兩面定位及第二面 create 名稱列表。它們的內容來源是真卡，測試 SourceUse／FrameId／meta 為明示合成，不能冒充已採納來源或實跑資格。

自身送墓來源只接 normalizer 已採納的完整永久 token amulet effect 第二行
`{起動}{コスト1}：これを墓場に置く。`，第一行離場抽1棄1仍是獨立 pending sibling。
固定 activation Concept、PP1 的 Nat/cost_value 與 graveyard 的 ZoneSet/destination_zone，
不開放費用或目的區參數。普通起動先支付 PP1，本文以 Put 將 SourceHolder 目前世代送到
card owner 的墓場；不是 ability master 的墓場、犧牲費用或 destroy，token 隨即在目的區消去。
inline／companion 都須完整 canonical head/body 相等；換頭、對象、目的區、追加條件、
費用或指示皆拒絕。BP07-T03、BP12-T12、BP17-T19、PR-195 與 SP01-T03 是同一永久 token
的印刷，能力來源可展開，完整 current validate／selection_input 仍因 pending sibling 拒絕，
不能省略第一行或把 token 能力掛在母卡來取得整卡證明。

固定 Fanfare 數值來源只接受 current JP 普通 follower 面的完整單行 effect、無 sections／特殊類型。能力頭 Concept、攻擊力／體力 Concept、兩個 required Nat/stat_delta_magnitude 葉及各自 occurrence、domain、binding 值皆精確核對；兩個數值都固定為 1，不提供攻／體參數位置。效果為能力 master 的場上其他全部合格 follower 永久 Add 1/1，依既有 Fanfare 待機與播放時點處理；無目標、條件、能力費用、期限或 aura。inline 與 companion 均比較完整 canonical 能力；換頭、換對象／範圍、Set、動態量、追加步驟／條件／費用／期限皆拒絕。BP01-037／BP01-P09 由正式 JP printing 解為同一永久卡，完整 current inventory 與 selection 可接受測試暫存的 DSL 檔；漏能力、錯 owner、額外 sibling 或 token 仍拒絕。這不遷移正式 DSL 檔、不授予用途審核或執行資格。

## 展開與 D0 消費邊界

`AuthorExpander.expand(raw, shards)` 精確選取 frame_id／ability_scope／body_version 與明示 registration，拒絕跨分片重複本體鍵。版本由原 JSON 的 `recipe,dsl,ability_scope,params,ports,body` 六欄依 P1 canonical 配方計算；宣告不符即拒絕，同 hash 仍比較完整 payload。展開只替換具型別 ParamRef 葉，不補語義預設、不更名、不重排、不算術化簡。canonical 同義能力比較只去頂層 line／section／sources；卡片審核 metadata 不在所選能力樹內，ports／imports 及所有語義子樹保留。

| 介面／證據 | 已驗範圍 | 不代表 |
| --- | --- | --- |
| `expand` → `ExpansionCandidate` | 完整 expanded JSON `tree` 與分離的 `sidecar`；source_proof 為 none | 來源角色真實、現行來源採納或可執行 |
| `expand_contexts` | 核對呼叫端提供的 context 與支援的參數角色／operand；source_proof 保持 none | 來源重建、current registry／索引閉包或 qualification 已通過 |
| `CurrentAuthorSources.validate` | 有限支援族通過 current／registry／index／完整來源 hash 與能力映射；成功 sidecar 為 current_source_checked | review／exam 資格有效或可執行 |
| `AuthorSchemas.expanded` | 共用 `$defs/expanded` 全樹驗證，加 decoded 型別圖獨立排除 ParamRef／GeneratedBy | IR lowering 或引擎語義正確 |

`expanded` 以獨立內嵌資源的 dynamic anchors 覆寫參數與 generated 分支，與 authored 共用能力圖；grant→static→EffectSpec.grant、delayed、Steps、instead.replace_with／after 的任何深度都適用。測試同時涵蓋 17 能力頭、遞迴殘留，以及故意破壞 anchor 覆寫後仍由 typed residual 檢查拒絕的反例。

來源 adapter 只支援已登錄的具名構造。offset 沿 expression 的正式 operand 定位並核對 count sibling，不按同數值反查；negated_magnitude、Concept／CardKind／Phase 來源轉碼尚未支援。來源路徑遇到非空 Projection ports、DSL ports 或 imports 即拒絕；純 DSL binding／imports 圖分析不證明來源 source_role 或 scope 映射。直接 instead 參數、grouped target 數量及 grouped search 目的地的白名單範圍尚待定義，展開時拒絕，不放寬 P1。

D0 只消費通過 expanded profile 的 JSON 樹；C0 sidecar 留在樹外，不是新增公開 wire。候選 golden 可用於 lowering 與漂移測試，第一版以來源綁定、良構檢查及引擎實作的支援範圍作為接線邊界，不執行用途依賴與資格機制；current_source_checked 本身不足以執行。D0 必須沿 SourceUse.owner 決定能力持有面，不能將卡級容器解讀為全卡常駐。Rust 應以鎖定的 `jsonschema` 0.58.1 驗同一 profile 的 dynamic scope 正反例，目前尚未完成該 Rust 驗證。

BP08-038／CP04-060 的固定 canonical golden 只驗純展開，source_proof 為 none。在既有來源 fixture 下，BP08 仍為 unresolved_whole_line_semantics，CP04 仍有 signed_numeric_requires_review；兩者均不得取得現行來源資格。合成 context 案例與這些真卡內容分開，不宣稱來源已採納。
