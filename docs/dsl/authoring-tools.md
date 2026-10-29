# 效果作者工具

新增卡片時，主檔放在 `authored/rules/author-effects/<卡包>/<record_key>.yaml`（例如 `BP02/BP02-084.yaml`）；寫入工具依卡號建立分片，不再取下一個數字，既有 `NNN.yaml` 保留原名。索引寫入 `authored/rules/effects-index/<卡包>.yaml`，每檔依 `record_key`、`scope` 排序，卡包取第一個 `-` 前的前綴。新增 runtime 測試只需新增 `sim/engine/tests/current_repository/runtime/*.rs`；build script 會自動探索並排序，不須修改 `runtime.rs`。

`sve-carddb author inspect/write/check` 使用同一個 current 選用入口。作者直接編輯 `authored/rules/author-effects/` 及 `effect-bodies/`，無需機器錨點、驗證檔或原型 baseline。

## 輸入 profile

公開 profile 讀取已審查的 card-data 與身分登錄，指定 JP 批次即可；`revision` 可省略，不要求作者固定 Git SHA。

```json
{
  "profile": "public",
  "repo": "/absolute/project",
  "jp_batch_id": "<已審 JP batch id>"
}
```

`authored`／`card_data` 可指定其他絕對根目錄。資料庫 profile 仍需 `database`、`recipe`、`frozen_inputs` 以重建相同來源上下文，依 CLI 型別接受配置；一般作者使用公開 profile 即可。來源歸檔及已審解析輸入的完整性契約不變。

## 查詢與手寫

```bash
uv --directory /absolute/project/carddb run sve-carddb author inspect \
  --inputs /absolute/work/public.json --card SYN-01
```

主檔可直接寫整卡 DSL，也可引用具名本體：

```yaml
format: 2
kind: author_effect_set
cards:
  SYN-01:
    admission: light
    meta:
      written_by:
        - actor: {who: "Author", model: null}
          date: "2026-10-27"
    abilities:
      - line: 1
        use: draw-self
        params: {n: 1}
```

```yaml
format: 2
kind: effect_body_shard
records:
  - id: draw-self
    admission: shared
    params: {n: Nat}
    written_by:
      - actor: {who: "Author", model: null}
        date: "2026-10-27"
    body:
      spell:
        do:
          - draw: {who: self, n: {param: n}}
```

`admission` 可放卡、本體或用途，值為 `light | shared`；用途優先於本體，本體優先於卡。只要任一層明示 admission，該卡走直接作者入口，其餘未標用途保留為共用語義責任。未標 admission 的既有卡維持來源重建路徑，亦不需要錨點。共用整理與審查見[常見效果作者流程](common-family-authoring.md)。

整卡 DSL 在索引中以永久 CardId 登錄一個代表卡號。current 選用依身分登錄展開同 CardId 的所有官方 JP 印刷卡號，共用代表卡的程式及來源定位；卡面仍讀各印刷的快照資料。不從卡號字串推測別名，也不為別名另登索引或保留原型程式。未解決的裁定同時阻擋這些別名。

實體 token 使用同一主檔、具名本體及行覆蓋入口。卡片層的 `token: {name: 操り人形, template: true}` 指定規則名稱與同名 token 的代表印刷；`template` 預設為 false，省略整個 `token` 時使用快照卡名。同名 token 有多個已選用程式時須恰有一個代表。卡面、數值與型別仍來自快照，`token` 只能標在單面的 token 卡上；印刷的突進、守護等以 `keywords` 宣告，與一般從者共用 lowering 與執行規則。

謝幕可用 `cost` 宣告頭付款（例如 `cost: {pp: 2}`），沿用接受／拒絕與不足資源不發動的付款流程；這不增加謝幕正式目標的准入範圍。

謝幕中的 `move: {ref: this, to: field}` 可將該次離場後仍在墓場的普通從者放回控制者自己的場上；沿用入場事件及守護姿態選擇，不重新打出卡片或支付印刷打出費用，但會照常觸發入場曲（[官方玩法說明](https://shadowverse-evolve.com/rules/ver_0324/)）。它只追蹤該次離場產生的世代，卡片後續再移區後不會追取新世代；場上已滿五張時不移動。其他能力頭或任意選擇物件的回場、復活入場附帶效果與其他連結家族仍不由此入口准入。

法術 token 使用一般 `spell` 本體與印刷 PP 費用，從手牌或 EX 區正式打出；印刷 Quick 同樣經由權威產生的 Quick 問題選用。結算沿用法術流程，離開結算區進入墓場後依 token 規則消去，不留在墓場。

`line` 從 1 起；`face` 為 1 起面序或唯一 `front/back`；`section: main` 表示主文，數字 N 表示第 N 個 section。多面未消歧即拒絕。`scope` 區分同一行的能力，跨行能力用 `covers: [1, 2]` 明示覆蓋；每行都須有能力或 `ignored` 宣告。

同一印刷行若有多個能力，各自指定不同的 `scope`（從 1 起）。例如 BP04-123 同行的 Fanfare 與 Lastword：

```yaml
abilities:
  - line: 1
    scope: 1
    fanfare:
      do:
        - draw: { n: 1 }
  - line: 1
    scope: 2
    lastword:
      do:
        - draw: { n: 1 }
```

兩個能力保留同一來源行，載入時以各自的來源 scope 區分；同一座標的重複能力仍拒絕。

```yaml
ignored:
  - line: 2
    reason: reminder
```

`reason` 只有 `reminder | layout`；不可與能力覆蓋重疊。token 定義不能標成母卡能力；使用卡片層的 `token_sources: [{section: 1, line: 1, card: BP01-T02}]` 明示連到共用 token 卡，跨行定義可用 `covers` 完整覆蓋。引用須指向選用中的完整 token 卡；區段與母卡能力不能重複覆蓋。參數實值為整數或字串，型別必須符合本體宣告；布林不算整數。多個 edition 時必須選版，不自動取最新版。

真正的規則本文若位於尚未分類的 section，可在卡片層明示 `rule_sections: [{section: 1}]`；多面卡須加 `face`（面序或 `front/back`）消歧。只有該 section 每個非空行都由能力的 `line`／`covers` 完整覆蓋時才放行，不能用提醒文字或 token 引用補足能力缺行。此宣告只影響該卡的 direct author 承載，不改來源的 `kind=unknown`，也不取代 DSL lowering 與 runtime 准入。

未明示 `rule_sections` 的未知 section 維持原契約：可用上述 token 引用，或在每個非空行都明示 `ignored: reminder` 時略過；已分類為 token 定義的區段必須使用 token 引用，不能宣告為規則本文。未覆蓋或僅標 layout 仍拒絕。這不授予提醒文字所述共通規則的執行能力。

目前 `create: {token: フェアリーウィスプ, n: 2, to: ex, result: made}` 支援固定數量、單一規則名稱、自己 EX；`to: field` 支援從者 token 至自己場上。省略 `n` 為 1；EX 與場上各最多五張，只建立並放入仍有容量的數量。`result` 或 `bind` 擇一，綁定實際放入的 token，滿區時為空；authority receipt 同樣只列出成功放入者。token 的整卡 runtime 程式仍須准入。場上建立共用進場事件與守護的選擇橫置程序；沒有先前區域的建立以 `EntryOrigin::Other` 表示，成功建立的每個 token 各發生一次進場。

回合時點自動能力可用 `auto: {zone: ex, on: {phase: {whose: self, which: end}}, do: [...]}`，只在持有卡位於 EX 時訂閱；省略 `zone` 仍為場上。EX 訂閱目前只接受單一區域的 Main／End 時點，並不授予從 EX 打出的許可。條件中的 `count: {who: self, zone: field, kind: follower, trait: 魔法使い}` 讀取結算當下符合條件的場上張數；私有區域計數仍不接受卡型或類型篩選。普通法術可在自身結算期間以 `move: {ref: this, to: ex}` 移動自身；EX 已滿則仍走既有結算清理。

## 寫入與驗證

```bash
uv --directory /absolute/project/carddb run sve-carddb author write \
  --inputs /absolute/work/public.json --main /absolute/work/main.yaml \
  --bodies /absolute/work/bodies.yaml --output-root /absolute/project/authored \
  --card SYN-01 --remove-prototype
uv --directory /absolute/project/carddb run sve-carddb author check \
  --inputs /absolute/work/public.json --output /absolute/work/check.json
```

`--card` 可重複，或用 `--all` 選全部輸入卡。只有替換既有原型時帶 `--remove-prototype`；不再要求 `--base`、`--baseline-root` 或 `--storage anchors`。工具先驗整份候選、索引閉包與原型互斥，再寫主檔／本體及索引；I/O 中斷回報已完成檔案，由作者修復 Git diff。

`check` 檢查全部索引作者卡；`body_usage` 依永久 CardId 去重，達三張提示整理共用本體，無額外資格 consumer。review 紀錄只報實際 verdict，不以 SHA freshness 授予核可或阻擋。source pending 不擋直接入口，真正 disputed／revise 只擋受影響卡。

check 成功代表來源定位、行覆蓋與 DSL 結構通過；Rust lowering／runtime 能力准入及對局行為測試仍須通過。錯誤回傳位置及分類，不印整行卡文。完整回歸交 CI，無需額外證據 JSON 或結果 DB。

打出限制可用 `play.cannot_if` 的 `in_zone: {ref: this, zone: ex}` 或 `my_turn: true`。目前只准入這兩個封閉形狀，分別禁止從 EX 打出及自己回合打出；一般打出與 Quick 候選會在扣費、移區或選擇前檢查，拒絕時不更動狀態。這些限制不阻止能力直接移動或將卡置入場。其他條件與額外打出區域仍須另行准入。
