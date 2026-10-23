# 常見效果的框架本體與重鍵作業

本契約規定從 typed 來源建立作者 1.0 本體時的合併邊界，以及 normalizer 批次升版的核對程序。
語法以 `dsl/` 為唯一權威；本文件不新增作者節點、公開格式、capability 或資格 consumer。
來源依[四層契約](../schema/domains/four-layer-translation.md)，用途及本體依
[作者承載契約](author-contract-1.0.md)。卡號集合、數量、切片與完成狀態留在工作追蹤，
不由本文件凍結。來源 pending 不得因 v0 操作標籤或原型 `status: complete` 而升級。

## 1. 從候選來源到完整能力

按抽牌等效果分組只作分析入口，不是 frame 身分或自動生成的充分條件。以 v0 `I.draw`、`I.damage`、
`I.destroy`、`I.search`、`I.modify` 找出候選，再回到同位置的完整原文、typed 葉、
Projection、relations、費用與上下文確認。標籤缺席不代表該卡沒有這種效果。
`I.modify` 須再區分攻／體、生命、費用與其他數值；固定傷害須排除動態式、分配、
取代與時點未解的候選。單一指示可以命中多類效果，不將其拆成互不相干的程式。

每筆具名來源規則必須明列：完整來源構造、必要 owner／面／欄位及能力上下文、
語義判別、葉的型別／role／domain、Projection kind、能力 scopes、ports、
正反例與拒絕原因。名稱釘在全庫 normalizer 版本內，不替單類效果建立永久版本分岔。
同一 resolved `(frame_id, ability_scope)` 才能選用對應本體；不能把相似句子的不同
frame 硬併成一筆 companion。可重用轉換程式，但每個 frame 仍保有自己的精確來源責任。

一個能力的多段 segments 組裝一次；同一位置多能力及 sibling scopes 各自保留。
跨 frame 的 imports 只接已驗上游 exports，不把 ports 當任意拼接手段。
目前一個能力若必須由不同 frame 片段合成、而沒有契約允許的封閉接線，保留 pending，
交跨行／低頻來源工作處理，不丟掉第二段。token 能力歸 token owner，不複製到母卡。

## 2. 框架本體與葉參數的界線

框架本體（名詞表的 frame body）的能力頭、控制圖、動作種類與來源角色固定；可變之處只限
[作者承載 §3](author-contract-1.0.md#3-封閉葉參數)白名單的精確葉位置。
型別相同仍須核對 role、operand 與來源構造；相同整數或中文相同不能替代這項核對。

| 差異 | 同本體的條件 | 必須保留為不同本體／用途的情況 |
| --- | --- | --- |
| 張數、傷害量 | Nat/count → draw.n；Nat/damage_amount → damage.n，來源角色及固定控制圖相同 | exact、up_to、逐張決定、動態算式或分配互換 |
| 選取數 | QuantitySpec/selection_count 的 constant；父構造及 exact／up_to 不變 | 打出目標與結算 choose 互換、單組與分組互換、改 minimum 或聚合限制 |
| owner、區域 | 只在已列的 Player／Zone 葉位置及封閉對照中轉換；完整 holder 語義已驗 | 以 ZoneKind 猜 holder、把 self 改成 each、以中文判斷對手、多區聯集或 banish 當參數 |
| 卡名、特徵、卡種 | 有已採納 CardName／Concept／CardKind 引用，且目的葉位置在白名單 | 從引用卡號、顯示譯名或未登錄文字猜 token／規則身分 |
| 攻／體、生命、費用數值 | 只有生命增加、費用設值、已證負號的減費及算式偏移有白名單位置；其他合法值分開 inline 或分本體常量 | 攻／體增減與設值（stat_delta_magnitude、stat_value）、damage_mod、資源與計數器增減都沒有葉位置；加值、設值、費用減免、生命回復混用 |
| 負向減費 | 來源證明負號，Nat/cost_delta_magnitude 以 negated_magnitude 轉 Int，adapter 已支援才使用 | value 冒負數、減費改設為零、把負傷害解成回復 |
| 能力及控制圖 | 頭、費用位置、選取時點、條件範圍、模式及步序全部固定 | fanfare／spell／進化時互換，optional／pay／if／instead 互換，分支重排 |
| 結果與期限 | result、receipt 類型、follow、capture 時點及 until 結構固定 | chosen／found 冒 moved、舊世代追同卡號、live 改 capture、永久改本回合 |

合法作者常量也不代表現有來源 adapter 或 executor 接受。先核對
[來源展開邊界](author-entry-shapes-1.0.md)與[內部執行子集](internal-execution.md)，
遇到未支援葉轉碼／ports 逐用途拒絕，不擴大白名單。整個 selector、Steps、binding 名稱、
scope、port、運算子及 capture／期限結構不能參數化。

各類效果最低拆分與驗證責任如下；這些不是已准入清單。

| 效果 | 固定留在本體的語義 | 逐用途應核對的邊界 |
| --- | --- | --- |
| 抽牌 | actor、exact／逐張模式、能力頭、付款與條件位置 | 0、牌庫不足、對手／雙方、不能抽牌、抽空規則檢查時點 |
| 固定傷害 | 對象種類、目標／動態集合、Uniform／Allocation、來源及傷害 flags | 無合法目標、零量、大值、取代／禁止、已選目標失效與資訊 |
| 破壞 | destroy 與 move／banish 分別保留；對象、選擇及 receipt producer 固定 | 破壞禁止、目標換世代、失敗仍執行的獨立後句、LKI |
| 搜尋 | source zone／owner、filter、展示、選取、移動、洗牌、餘牌順序及 may 位置 | 找不到、EX 滿、選而未移動、私有觀察、search 後對 moved 的處理 |
| 數值增減 | 原值／目前值、加值／設值、攻／體／生命／費用、期限與固定讀值時點 | 負值、零差值、跨進化、換區失效、期限到期、疊加與取代 |

不以通過五種合成局面推定該類效果全部用途可執行；每個採納本體保留自己的受驗形狀與反例。

## 3. 共用本體的證據與逐用途審核

共用本體依 companion key、body_version、applicability_scope 精確選版，
不挑最新版本。分片 owner 只決定存放位置，不決定來源或身分。
inline 與 companion 展開後，去除位置及審核 metadata 的 canonical 能力 bytes 必須一致，
ports 保留；再交 D0 lowering，不能只比較稀疏 v0 IR 標籤。

[MQ 固定案例](macro-qualification-cases.md)定義的完整資格門檻為：同精確 frame、
resolved 變體與 ability_body，按不同 `(card_name_concept, evolved)` 去重至少三個；
重印／異圖不加計，進化前後依該鍵區分。還要兩個不同且均非任何 body 作者模型的
獨立有效審核，以及同版本／域的零卡、無合法目標、大值、時點、資訊五類邊界證據。
沒有永久卡名概念或 evolved 證據時不能用卡號張數代算。

第一版依 [#800 的維護者決定](https://github.com/gbaian10/sve-kit/issues/800#issuecomment-6307831555)
不實作這套資格機制，也不宣稱 qualified。規劃共用本體時保留上述三卡、兩個非作者模型
及邊界案例作設計證據；證據不足不擋本體存放或合法引用，但所有用途仍逐用途獨立審核。
單一審核者的設計核可不補足兩模型，不自動給新用途 reviewed。
不使用尚未實作的 macro_mechanical 或 verified_by_exam 來繞過第一版邊界。

逐用途報告分開記錄來源已確認、獨立 reviewed、compiled、executed 及拒絕原因；
compiled 須含完整 expanded 樹與 typed IR 的良構證據，executed 須指向真實場景結果。
未解規則保留 pending，不能用空 do 或 none 代替；disputed 不因重鍵或改寫格式消失。

## 4. 真卡的合併與拒絕示例

以下是已審解析輸入中的日文片段，供設計核對；不是正式作者採納或新增裁定。
官方內容仍依 [LICENSING.md](../../LICENSING.md)排除於專案授權之外。

- [BP01-041](../../card-data/jp/BP01/000-099.json)「騎士王の威光」面 0、effect 第三行：
  `「{起動}{コスト3}これを{アクト}：1枚引く。」`。
  只有 draw 的張數葉可以按 Nat/count 映成 n；PP 與自身 Act 保留在起動費用。
  第一行的 token 生成／疾走及第二行的 act 也必須在整卡閉包內，不能只因第三行可跑就准入整卡。
  出處：[官方卡頁](https://shadowverse-evolve.com/cardlist/?cardno=BP01-041)，
  document `documents-02284f6dfc7b21e6`。
- [CP04-060](../../card-data/jp/CP04/000-099.json)「カヤ」面 0、effect：
  `「{UB}【進化時】自分のデッキから「これと同名を除くドラゴンズネスト・カード」1枚を探し、EXエリアに置く。このターン、それをプレイする際、コストを-2する。」`。
  搜尋成功 moved 的新世代才是後續本回合減費對象；found 不等於 moved，-2 不等於 set 0。
  排除同名、進化時頭及 UB 不得參數化刪除；其 section 能力也須完整覆蓋。
  出處：[官方卡頁](https://shadowverse-evolve.com/cardlist/?cardno=CP04-060)，
  document `documents-7bf83dea87454cf4`。這張卡是複合反例，不是「單一搜尋」可直接執行證明。

## 5. normalizer 批次升版與全庫重鍵

以下為寫入工具及操作者必須完成的程序；第一版 loader 不代做 rekey，
目前缺少的寫入工具須先實作並驗證，不能手改 hash 後稱已完成。
每批規則只產一個新的全庫 normalizer 版本；不替單類效果長期保留旁支。
重鍵候選與對帳資料不覆寫已發布快照，也不新增長期相容 reader。

1. **凍結輸入。** 記錄 base、舊 normalizer、規則 recipe／資源指紋、JP 採納批次、
   current 及歷史來源、registry／token 選用、全部 authored 來源、裁定與作者資源。
   保存舊 DB／payload 與 exact bytes hash，先取得完整 frame、binding、用途與反向依賴盤點。
   缺舊 payload、來源域或原證據先列拒絕，不能從新版回推歷史。
2. **提出具名規則與候選。** 明列新增 resolved 的判別、未命中／衝突／pending 尾端、
   typed 值及 scope。以同一批輸入產生新全庫候選，除本批規則外不混入文字更正、
   裁定升級或 adapter 改版。來源變動另列，不能混稱純版本差異。
3. **逐 frame 比 payload。** 去掉 normalizer_version 後，比較完整 frame-v1 payload bytes，
   包括 canonical_source、語義變體、葉 schema／domain／occurrences 及 Projection。
   只有 payload 相同、occurrences／values／scope／接口語義不變，且新目標已存在而唯一，
   才列「版本唯一差異」。不可只比較 canonical_hash、中文、body hash 或舊 T ID。
4. **建立完整映射。** 列出版本唯一差異、語義變更、拆分、合併、新增、刪除、
   無唯一目標與缺證據。mapping 按精確來源用途處理多對多；即使多個舊 frame 合併，
   也逐用途驗證，禁止依最大覆蓋率或相同中文任選。集合差必須有原因，不能只抽樣對帳。
5. **遷移所有依賴。** 全庫重算 FrameId、SourceBindingId、source 及 interface_key，
   不只本批效果。對帳 targets／expression_targets、forms、binding values／trace、
   render、翻譯選用、裁定 reference／candidate／用途域、effects-index、
   companion frame／ability_scope、generated_by、sources、imports／ports、
   effect-source-v1 與直接及傳遞上游的用途依賴。未採納的裁定候選保持候選，
   原 pending 與證據 strength 不升級；不能把未知歷史域重算成已知。
6. **區分延續與失效。** 第一版不實作作者證據 rekey 或用途依賴 consumer。
   因 normalizer 重鍵而改變依賴的舊 `ok` review 只留 Git 歷史，不留在當前
   meta.reviewed_by 或 registration.reviews，也不計為新版有效審核；當前用途需重新取得
   獨立審核並記錄新依賴。現行 `revise`／`disputed` 不是過期核可：寫入時沿精確用途保留
   原審核者、日期、verdict、reason 及原依賴快照，繼續阻擋 ok 類資格，不以重鍵或過期
   不計數解除；disputed 只有明示解決原因並經獨立審核的寫入才可更新 verdict（作者承載 §5）。
   找不到唯一承接用途時也不刪除這些未解項，改列入批次報告。meta.status 依剩餘有效審核
   及保留的未解項重算：有未解 disputed 仍為 disputed，不自動洗成 draft 或 reviewed；
   含 revise 或缺有效審核的用途不計為 reviewed。原始審核者、日期與 verdict 不改成新版證據，
   未完成者保留 pending／拒絕；這不要求重翻純 ID 變動的中文。
   以下是延伸契約啟用後的有限延續條件，並非第一版工具義務：純版本差異只延續實際存在且先前有效的證據。
   若另行核准並實作 §4.3 的 rekey 形狀，釘住原 input_hash／
   record_key、before／after 及 mapping_recipe；寫入時取得原證據及舊 payload 驗證，
   壓成原證據到當前用途，不建永久逐次帳本。body、params、語義、案例、runner、裁定、
   展開配方有變，或非唯一／缺輸入時正常失效，重新逐用途審核及驗證；不得偽造延續。
   第一版未實作的資格失效資料不新建 consumer，只在批次報告列受影響證據與後續義務。
7. **產生用途差異。** 每個用途列舊／新來源原文、typed 值、render 輸出、
   frame／binding／接口與作者依賴差異及理由。區分純 ID 改變、同語義輸出不變、
   來源語義改變、中文輸出改變及未解；保留未選模式、token 及歷史用途。
   中文語義真正改變才派人工重查；純重鍵不重翻，同文字不構成語義相同證明。
   render 未變但來源語義改變的用途仍須人工判斷譯文是否足夠，不直接沿用核可。
8. **完成對帳後合併。** 檢查舊集合每筆都有去向／拒絕原因、新集合每筆有來源，
   所有 current 引用無懸空、每卡完整能力閉包、無殘留舊 normalizer；
   歷史原始證據保留舊 ID，不當成漏改。先前有效與失效證據數、保留的未解 revise／disputed、
   中文重審清單、reviewed／compiled／executed 及 pending 數分開回報。通過來源、展開、
   A2 基線差異且既有作者用途的真卡場景不退步後，單一 PR 原子切換該批 normalizer 與全部引用；
   新 resolved 來源的本體只能在切換後撰寫，其真卡場景另逐用途驗收。
   未完成的卡明示拒絕，不輸出可執行程式。下一批合併前以當前 main 重新核對。

## 6. 正式身分、完整能力與原型替換

逐卡來源從已採納 JP printing 唯一解出永久 CardId、FaceId、revision/current 與 token 選用，
不得以卡號前綴、去後綴或顯示名生成身分。所有同號 printing、全部面、sections 與 token
能力都在閉包內；SourceUse.owner 決定能力持有面，來源及 instructions 可逐節點回溯。
索引與作者檔雙向閉合；未知／歧義身分、漏 sibling、未選模式或未知 token 都拒絕整卡自動執行。

原型以完整卡為切換單位：同卡 shared scope 不得同時有 astra/1 與作者 1.0 記錄。
依上述雙向閉合，含多類效果的卡在全部能力都能寫成作者 1.0 前不建作者卡檔；
可先存放的只有 resolved ability_body 的 companion 本體。不能只搬一個能力就刪原型。
用 [A2 基線](v0-regression.md)的 AST、正反向位置與差異場景確認行為；它保存舊缺陷，
不能不經原文判讀便將舊輸出當正確期望。修正只更新已審的卡與案例，保留變更原因。

每張原型最後必須遷移或保留明確拒絕清單；空能力卡核對一般規則責任，不假算 executed。
移除 astra/1 前，索引、loader、測試與所有引用都須確認不再需要它；Git 中的來源與舊判讀不刪歷史。

### 逐批選用入口

每張完整卡的作者檔、`effects-index` 記錄與原型條目在同一個 PR 切換；
不能先刪原型、再等另一個 PR 補作者卡或索引。切換回退用正常 revert，保留來源歷史。

`CurrentAuthorSources.selection_input()` 從同一次 `load_author_set()` 的完整輸入清單，
沿用索引閉包、永久身分重疊與 current 完整能力驗證，交接原型文件及每張作者卡的
accepted／rejected 結果。它不略過拒絕卡，也不把拒絕卡換回原型。
Rust `Catalog::from_current_selection()` 消費這份內部交接：原型沿用既有 Catalog，
作者卡逐張經 `ExpandedLowerer::lower_current()` 的完整能力對帳與整樹 lowering。
任一卡拒絕、重疊或 lowering 失敗都拒絕整次選用，不回傳半份結果。

成功結果 `SelectedEffects` 分別保留原型 Catalog 與完整 current D0 候選；
這不是 D1 runtime 准入，也不讓永久 owner 自動進入舊原型執行器。
尚未提供 current 來源上下文的 `Catalog::load()`／`load_with_identity()`，
遇到作者索引、作者卡或 companion 目錄即明確拒絕，不再只掃原型目錄而漏載已切換卡。
`from_documents()` 仍是 caller 明示提供原型 bytes 的入口，不能用它代替完整作者目錄選用。
這些介面不新增公開 wire、快照欄位、capability 或用途資格。
