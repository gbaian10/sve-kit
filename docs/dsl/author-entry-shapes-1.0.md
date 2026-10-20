# 作者 1.0 入口容器

作者檔與伴隨本體共用 `author-nodes.schema.json` 的能力內容圖。`author.schema.json`／`effect-bodies.schema.json` 各由固定 format/kind 選取，逐卡 dsl 只驗支援集；不得依 dsl 猜「最新」資源。原型 effect_set 不在本 reader 的作者入口。

此 reader 只驗結構與檔內事實。來源 registry／現行版次／FrameId／Port 閉包、索引候選雙向閉包、資格與引擎准入仍是其他驗證階段。generated_by 展開與 body hash 由下述明示展開 API 核對，不由結構 reader 隱式執行。`status:verified` 是作者聲稱，不構成可執行資格；非空 `verified_by_exam` 尚無封閉格式，現階段報 unsupported_exam_evidence。

## 容器矩陣

| 入口 | 必填 | 選填／互斥 |
| --- | --- | --- |
| 作者卡檔 | format:1、kind:author_effect_set、scope、cards | 無其他頂層鍵；format／kind 檢查後 en_override 報 en_source_contract_unavailable |
| A.card | meta | note、abilities、faces、deck_rule；不附卡面數值 |
| A.face | abilities | faces 的 key 是永久 f:32hex；不再嵌 meta／faces |
| inline 能力 | line、sources、ports、恰一個能力頭 | section、非空 imports；禁止 generated_by |
| generated 能力 | line、sources、generated_by | section、非空 imports；禁止 ports 與能力頭 |
| 本體分片 | format:1、kind:effect_body_shard、scope:shared、records | 無其他鍵 |
| companion | frame_id、ability_scope、body_version、dsl、params、body、ports、written_by、registrations | note；body 不含用途位置或 generated_by |

sources 恰一筆，OccurrenceKey.segments 可以保留多段，不複製能力執行。line 從 1 起算，section 從 0 起算。通用 OccurrenceKey 的四層 owner 型別包含 Q&A 等來源，結構可讀不表示這是合法 JP 作者來源；shared 來源准入仍需已採納 JP face_revision 及精確 owner／field／face／line／scope。

inline 的能力 payload 與 companion 使用同一型別圖。單跑 Schema／TypeAdapter 可能接受合法葉位的 ParamRef；完整結構 reader 另走 decoded 型別圖，對 inline 內任何 ParamRef 報 inline_param_reference。走訪透明 RootModel、保留 BaseModel 的序列化 alias 與 TypedDict wire 鍵，不掃全域同名鍵，因此不把依賴 params 誤認成能力參數。這項 inline 檢查不取代下述完整 expanded profile。

ability.imports 若出現必非空，沿 P1「沒有 import 時省略」；Ports.imports、Ports.exports、UseDependencies.imports 皆必填且可空。A.card 的 abilities／faces 及 A.face.abilities 沒有額外非空要求；合法省略與空值各自保留作者 bytes，不推定同 hash。

## 證據與局部 invariant

Actor 恰為 who/model，model 可以 null 但不得省略。written_by 非空，日期是真實 YYYY-MM-DD。BodyReview 只放 registration，UseReview／MacroMechanical 放 meta；機械項不虛構 reviewer，其 use_dependencies 只收引用用途（applicability_scope／domain_hash 非 null），inline 依賴不能取得機械資格。body_boundary 屬測試輸入與建置 DB，與逐用途 engine exam 不同，不在這兩種 authored 容器中。

UseDependencies.imports 值恰為 binding_id／ability_scope／port／dependency_key；後者是 P1 effect-use-v1 Digest，不內嵌遞迴上游快照。Rekey 保存完整 before/after 依賴，不表示機械延續已被驗證。

| invariant | 精確範圍 |
| --- | --- |
| 字串排序唯一 | Domain.regions |
| 完整 canonical bytes 排序唯一 | UseDependencies／BodyDependencies 的 interfaces、rulings；BodyDependencies.sources |
| 原序保留 | UseDependencies.sources、written_by、reviewed_by、registration.reviews、registrations、meta.rulings 及所有能力陣列 |
| registration id 唯一 | 每筆 companion |
| companion key 唯一 | 同檔 frame_id／ability_scope／body_version；跨分片另驗 |
| reviewer＋依賴唯一 | 同陣列 review；模型按精確 model、真人按 Actor；原 dependencies 及 rekey.after 都查 |
| review 類型互斥 | 同明示用途不能同時有 UseReview／MacroMechanical；不以不同 body version 規避 |
| disputed reason | 必須非空 |
| MacroMechanical 本體一致性 | body 與 use_dependencies 的 frame_id／ability_scope／body_version／applicability_scope 逐欄相同 |

## 診斷與驗證範圍

`AuthorSchemas.author_effect_set`／`effect_body_shard` 讀取兩種完整容器，`document` 共用已解析文件依 kind 分派；不開啟索引指定檔案。`author_tree`／`body_tree` 完成相同驗證後回傳嚴格 loader 的原 JSON 樹，保留合法選填欄位與順序，不由 model dump 重建 hash 輸入。`author_effect_set`／`document` 可帶已解碼的 `AuthorIndexRecord` 作為 context；YAML／JSON 解析失敗仍保留其 card_id、record_key 與 scope。若錯誤位於另一張卡，不借所選卡的 card_id。

診斷帶 stage／reason／作者結構 path／可得 YAML line、column。source line 與 YAML line 不混用；來源定位僅取已驗型別。錯型別只帶 expected/actual 類別，不附 input_value 或卡文；parser 失敗只帶安全類別。未知 key、版本、非字串鍵、重複鍵、非 JSON 純量、unsafe int 與浮點皆拒絕。合法 alias／merge 展開，無直接 cursor 時取最近父容器。

固定內容種子：BP08-038 的 Live count−5 與 draw；CP04-060 的 search→follow→本回合 cost delta −2；ECP01-058 的 evolve deck_rule 10；BP09-SL05 兩面定位及第二面 create 名稱列表。它們的內容來源是真卡，測試 SourceUse／FrameId／meta 為明示合成，不能冒充已採納來源或實跑資格。

## 展開與 D0 消費邊界

`AuthorExpander.expand(raw, shards)` 精確選取 frame_id／ability_scope／body_version 與明示 registration，拒絕跨分片重複本體鍵。版本由原 JSON 的 `recipe,dsl,ability_scope,params,ports,body` 六欄依 P1 canonical 配方計算；宣告不符即拒絕，同 hash 仍比較完整 payload。展開只替換具型別 ParamRef 葉，不補語義預設、不更名、不重排、不算術化簡。canonical 同義能力比較只去頂層 line／section／sources；卡片審核 metadata 不在所選能力樹內，ports／imports 及所有語義子樹保留。

| 介面／證據 | 已驗範圍 | 不代表 |
| --- | --- | --- |
| `expand` → `ExpansionCandidate` | 完整 expanded JSON `tree` 與分離的 `sidecar`；source_proof 為 none | 來源角色真實、現行來源採納或可執行 |
| `expand_sources` | 自 raw／descriptor／classifier／context 重建來源，核對用途及支援的參數角色／operand；成功後 source_proof 為 derived_self_consistent | current registry／索引閉包或 qualification 已通過 |
| `AuthorSchemas.expanded` | 共用 `$defs/expanded` 全樹驗證，加 decoded 型別圖獨立排除 ParamRef／GeneratedBy | IR lowering 或引擎語義正確 |

`expanded` 以獨立內嵌資源的 dynamic anchors 覆寫參數與 generated 分支，與 authored 共用能力圖；grant→static→EffectSpec.grant、delayed、Steps、instead.replace_with／after 的任何深度都適用。測試同時涵蓋 17 能力頭、遞迴殘留，以及故意破壞 anchor 覆寫後仍由 typed residual 檢查拒絕的反例。

來源 adapter 只支援已登錄的具名構造。offset 沿 expression 的正式 operand 定位並核對 count sibling，不按同數值反查；negated_magnitude、Concept／CardKind／Phase 來源轉碼尚未支援。來源路徑遇到非空 Projection ports、作者 ports 或 imports 即拒絕；純作者 binding／imports 圖分析不證明來源 source_role 或 scope 映射。直接 instead 參數、grouped target 數量及 grouped search 目的地的白名單範圍尚待定義，展開時拒絕，不放寬 P1。

D0 只消費通過 expanded profile 的 JSON 樹；C0 sidecar 留在樹外，不是新增公開 wire。候選 golden 可用於 lowering 與漂移測試，正式生產接線仍須來源集合、current／index 閉包及資格驗證。Rust 應以鎖定的 `jsonschema` 0.58.1 驗同一 profile 的 dynamic scope 正反例，目前尚未完成該 Rust 驗證。

BP08-038／CP04-060 的固定 canonical golden 只驗純展開，source_proof 為 none。在既有來源 fixture 下，BP08 仍為 unresolved_whole_line_semantics，CP04 仍有 signed_numeric_requires_review；兩者均是來源重建路徑的負例。合成 resolved 相容性案例與這些真卡內容分開，不宣稱來源已採納。
