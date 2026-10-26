# 效果作者工具

`sve-carddb author inspect/write/check` 共用
`sve_carddb.workflows.authoring` 的來源準備與整卡驗證。手寫輸入使用下列
format 2 儲存格式；主檔與本體直接提交到 `authored/rules/author-effects/` 與
`effect-bodies/`。`author write --storage anchors` 產生每包一份
`effect-anchors/<owner>.yaml`；這些機器檔提交、不手改。普通建置與 check 只重算比對，
不刷新精確引用，也不續用舊審核。

## 輸入 profile

每個指令都明示 `--inputs /absolute/work/public.json`。公開 profile 只讀取
已審查的 card-data；JP batch 與程式版次必填，不能自動選最新版本。

```json
{
  "profile": "public",
  "repo": "/absolute/project",
  "jp_batch_id": "sha256:<JP batch 的 64 位十六進位值>",
  "revision": "<固定的 40 位 Git SHA>"
}
```

`authored`、`card_data` 可明示其他絕對根目錄，預設為 repo 下的同名目錄。
公開 profile 的分類詞彙只包含這個 JP batch 或作者明寫來源的概念；其他來源的概念
不冒充已驗證資料。所有作者卡仍須通過整卡來源閉包，不能以缺少概念放寬准入。

正式 DB profile 使用相同 repo、JP batch 與額外四個必填欄位：

```json
{
  "profile": "database",
  "repo": "/absolute/project",
  "jp_batch_id": "sha256:<JP batch 的 64 位十六進位值>",
  "revision": "<凍結 DB 的程式版次>",
  "database": "/absolute/build/build.sqlite",
  "database_sha256": "<DB 的 64 位 SHA-256，不含 sha256: 前綴>",
  "recipe": "/absolute/build/recipe.json",
  "frozen_inputs": "/absolute/build/inputs.json"
}
```

DB 以 SQLite readonly/immutable 開啟，先檢查 SHA-256、recipe 與 inputs 的版次及
JP batch。recipe 提供凍結來源 store；工具不讀取 live manifest。所有路徑均由呼叫端
提供，不把本機位置寫進作者資料。

## 查詢與手寫

```bash
uv --directory /absolute/project/carddb run sve-carddb author inspect \
  --inputs /absolute/work/public.json --card BP10-122 --card BP09-005 \
  --output /absolute/work/inspect.json
```

報告包含全部面、各文字欄的逐行卡文、位置候選、句型名稱、typed slots、現有作者
用途及完整本體定義；pending 候選也會列出。報告含卡文，只寫到明示的本機檔案，
不要上傳測試 artifact 或 cache。查到候選不代表取得 current proof。

主檔格式範例中的 Author 是佔位；既有卡與本體的作者、日期和審核事實請照 inspect
原定義填寫，新卡才填實際貢獻者：

```yaml
format: 2
kind: author_effect_set
cards:
  BP10-122:
    meta:
      written_by:
        - actor: {who: Author, model: null}
          date: "2026-10-26"
    abilities:
      - line: 1
        use: draw_exact_attack_time.v1
        params: {n: 1}
```

本體檔：

```yaml
format: 2
kind: effect_body_shard
records:
  - id: draw_exact_attack_time.v1
    params: {n: Nat}
    body:
      attack_time:
        do: [{draw: {who: self, n: {param: n}}}]
    written_by:
      - actor: {who: Author, model: null}
        date: "2026-10-26"
```

`line` 是從 1 起的卡文行號。定位只使用卡號、行號與作者明寫的
`face`、`section`、`scope`，不使用 `use` 名稱、參數值或能力頭排除候選。
多候選依 face → section → scope 提示第一個可減少歧義的欄位。
`face` 可用從 1 起的序號或全卡唯一的 `front/back`；`section: main` 代表 effect，
數字 N 代表 section ordinal N−1；`scope` 是同一面、欄位與行的來源能力次序。
BP09-005 的 line 1 必須指定 face；定位成功後若來源仍 pending，write 仍拒絕。

能力可以直接寫原 DSL 頭（例如 `evolve: {cost: {pp: 1}}`），或寫
`keywords: [ward, aura]`。三種方式互斥；關鍵字保持小寫，大寫不會自動轉換。
inline 的空 ports 可省略。主檔的 scope/dsl 預設為 shared/1.0；每卡 meta 的
status 預設 draft，reviewed_by、verified_by_exam、rulings 預設空陣列。
note、deck_rule 保留原 DSL 邊界，來源尚未支援的結構仍拒絕。

本體的 params、ports、ability_scope、edition、registrations 可分別省略為空 map、
兩個空 ports map、ability_0、1、JP domain 且 reviews 為空的 registration。
參數型別不能由值猜測；省略 source 時，工具按 DSL 參數位置與來源葉的 type/role/component
唯一推導。無候選會拒絕，多候選要求明寫
`params: {n: {type: Nat, source: {slot: leaf_4, component: constant}}}`。
多個 edition 省略選版會拒絕，不選最新版；registration 預設 jp。

## 寫入與驗證

`--output-root` 是完整 authored 根目錄，可先複製一份以檢查 diff：

```bash
cp -a /absolute/project/authored /absolute/work/author-output
uv --directory /absolute/project/carddb run sve-carddb author write \
  --inputs /absolute/work/public.json --storage anchors \
  --main /absolute/work/main.yaml --bodies /absolute/work/bodies.yaml \
  --card BP10-122 --output-root /absolute/work/author-output
```

main/bodies 各可指定單檔或 `<owner>/<序號>.yaml` 的目錄；main 的 owner 必須與卡包
一致。同一 id/edition 不可重複定義。`--card` 只更新指定卡；`--all` 才選全部手寫卡。
重用已提交本體可透過完整本體目錄提供定義，工具算回原 frame/root/body_version 並逐欄
比對；作者、status、registration、review 或 note 衝突會拒絕，不合併或覆寫事實。
新增不同本體版本須明寫新 edition；原本體保留。既有作者的 card/faces 容器原樣保留，
其他卡與未引用本體不重排。同名／edition 的本體版本不能原地覆寫；共享本體的變動若影響
未選卡，write 拒絕並要求把所有用途納入同次更新。

write 在記憶體完成整份候選 inventory、schema、參數、來源、索引及整卡閉包驗證後，
才直接寫指定檔案。語意失敗不寫檔；I/O 中斷會列出已寫檔案，可能留下部分工作樹，
以 git diff/check 檢查並由作者修復。沒有 plan/apply、journal 或自動 rollback。

把 profile 的 `authored` 指向待查根目錄，再執行：

```bash
uv --directory /absolute/project/carddb run sve-carddb author check \
  --inputs /absolute/work/public.json --output /absolute/work/check.json
uv --directory /absolute/project/carddb run sve-carddb author check \
  --inputs /absolute/work/database.json --compare-public /absolute/work/public.json \
  --output /absolute/work/equivalence.json
```

check 永遠檢查全部索引作者卡，回傳完整 selection 與 review freshness 診斷。
缺／多錨點、摘要手改、精確引用不符或來源失效均拒絕。錯誤包含卡號、卡文 line 與
source／binding／frame／body／params／recipe／dependency／anchor 階段；只有 basis 不符時，
anchor 階段表示其餘摘要依賴有變動，不能猜成只有配方變更。
先 inspect、看主檔／本體與錨點的 git diff，再決定是否明示 write。新增 adapter 或 Schema
會使全域資源 pin 變動；在同一個 PR 對完整手寫目錄 `write --all` 並驗全量 check，
不能先留下整批 stale 的 main。review 的 anchor 是當時摘要的 literal；缺少或不同標 stale，
disputed 保留。report 的 recorded 只表示 literal 相符，不授予尚未實作的資格。

正式／公開比較各自 prepare，
比對 accepted/rejected、原因、展開樹、來源 inventory 與 provenance 的完整內容，
不複製同一份 selection 冒充兩個 profile。repository integration pytest 與 Rust
測試輸入 adapter 也呼叫這個公開 check，不需要私人 checkout 或新 CI job。
表示遷移的前後等價由本機一次性驗證，腳本與證據留在工作目錄；CI 檢查工具行為
與現行資料完整性。

需要移除原型時，write 加上 `--remove-prototype --base <固定 SHA>` 與
`--baseline-root /absolute/project/carddb/tests/fixtures/effect_baseline`；check 也接受
後兩個參數。工具檢查 base 的原型 AST 摘要、完整來源位置與 current replacement，
確認只少指定卡、其他原型不變、全部 baseline 檔 byte 不變，再允許寫入。
base 或 baseline 缺少時失敗，不自動建立或更新期望值。

退出碼：0 成功；2 參數／profile 配置錯；3 schema、唯一定位、來源、閉包、基線或
selection 等價失敗；4 I/O 或缺少 Git base／baseline。錯誤只包含分類、卡號、DSL/YAML
位置與必要的消歧選項，不列出輸入值或整行卡文。check 成功只表示現有來源准入與閉包
通過，不代表引擎 runtime 支援或審核資格通過。
