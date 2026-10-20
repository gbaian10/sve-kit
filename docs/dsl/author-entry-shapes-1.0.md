# 作者 1.0 入口容器

作者檔與伴隨本體共用 `author-nodes.schema.json` 的能力內容圖。`author.schema.json`／`effect-bodies.schema.json` 各由固定 format/kind 選取，逐卡 dsl 只驗支援集；不得依 dsl 猜「最新」資源。原型 effect_set 不在本 reader 的作者入口。

此 reader 只驗結構與檔內事實。來源 registry／現行版次／FrameId／Port 閉包、索引候選雙向閉包、generated_by 展開與 body hash、資格與引擎准入仍是其他驗證階段。`status:verified` 是作者聲稱，不構成可執行資格；非空 `verified_by_exam` 尚無封閉格式，現階段報 unsupported_exam_evidence。

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

inline 的能力 payload 與 companion 使用同一型別圖。單跑 Schema／TypeAdapter 可能接受合法葉位的 ParamRef；完整結構 reader 另走 decoded 型別圖，對 inline 內任何 ParamRef 報 inline_param_reference。走訪透明 RootModel、保留 BaseModel 的序列化 alias 與 TypedDict wire 鍵，不掃全域同名鍵，因此不把依賴 params 誤認成能力參數。這不是 expanded profile，不能由此直接交 D0。

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

## 診斷與驗證範圍

`AuthorSchemas.author_effect_set`／`effect_body_shard` 讀取兩種完整容器，`document` 依 kind 分派；不開啟索引指定檔案。`author_effect_set`／`document` 可帶已解碼的 `AuthorIndexRecord` 作為 context；YAML／JSON 解析失敗仍保留其 card_id、record_key 與 scope。若錯誤位於另一張卡，不借所選卡的 card_id。

診斷帶 stage／reason／作者結構 path／可得 YAML line、column。source line 與 YAML line 不混用；來源定位僅取已驗型別。錯型別只帶 expected/actual 類別，不附 input_value 或卡文；parser 失敗只帶安全類別。未知 key、版本、非字串鍵、重複鍵、非 JSON 純量、unsafe int 與浮點皆拒絕。合法 alias／merge 展開，無直接 cursor 時取最近父容器。

固定內容種子：BP08-038 的 Live count−5 與 draw；CP04-060 的 search→follow→本回合 cost delta −2；ECP01-058 的 evolve deck_rule 10；BP09-SL05 兩面定位及第二面 create 名稱列表。它們的內容來源是真卡，測試 SourceUse／FrameId／meta 為明示合成，不能冒充已採納來源或實跑資格。
