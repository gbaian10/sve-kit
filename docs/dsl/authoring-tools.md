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

所有觸發能力都可用 `targets` 宣告正式目標，並搭配 `cost` 頭付款或 `optional_cost` 可選付款；控制者在檢查時點打出待機能力時選擇目標，沒有足夠合法目標就不發動，不執行後續效果。謝幕在來源離場後選擇目標。目標宣告前不執行本體；同一印刷行的入場曲與謝幕仍以不同 `scope` 區分。模式目標、單一目標組的分配傷害，以及 `at: resolution` 公開計數讀值沿用共同打出規則。

謝幕可用 `cost` 宣告頭付款（例如 `cost: {pp: 2}`），沿用接受／拒絕與不足資源不發動的付款流程；目標在付款前宣告。

謝幕中的 `move: {ref: this, to: field}` 可將該次離場後仍在墓場的普通從者放回控制者自己的場上；沿用入場事件及守護姿態選擇，不重新打出卡片或支付印刷打出費用，但會照常觸發入場曲（[官方玩法說明](https://shadowverse-evolve.com/rules/ver_0324/)）。它只追蹤該次離場產生的世代，卡片後續再移區後不會追取新世代；場上已滿五張時不移動。墓場法術打出觸發也可將自身放回場上；其他持有者位置、任意選擇物件的回場、復活入場附帶效果與其他連結家族仍不由此入口准入。

墓場法術打出觸發可接正式目標、可選付款與模式，也可執行不操作持有者的普通本文；對 `this` 的操作只准放回控制者場上，不能把墓場持有者當成場上活物件強化、破壞或移 EX。

`entered_from`、`entered_by`、`played_from` 讀取自身入場事件；入場曲與 `auto.on.enter_field: {ref: this}` 都可使用，監看他者入場或其他事件的能力不提供這些欄位。自身回場及離場後移 EX 需要該次離場確定落在墓場，目前只有謝幕符合；一般 `left_field` 不開放這兩種動作。

法術 token 使用一般 `spell` 本體與印刷 PP 費用，從手牌或 EX 區正式打出；印刷 Quick 同樣經由權威產生的 Quick 問題選用。結算沿用法術流程，離開結算區進入墓場後依 token 規則消去，不留在墓場。

`line` 從 1 起；`face` 為 1 起面序或唯一 `front/back`；`section: main` 表示主文，數字 N 表示第 N 個 section。多面未消歧即拒絕。`scope` 區分同一行的能力，跨行能力用 `covers: [1, 2]` 明示覆蓋；每行都須有能力或 `ignored` 宣告。

進化頭可用 `usable_if: {cmp: {left: {count: {who: self, zone: cemetery, orig_cost_eq: 2}}, op: ge, right: 10}}` 表達自己的墓場中原始費用 2 的卡至少 10 張。費用與門檻目前只接受非負整數常量；每次宣告進化時先依當前墓場與印刷費用檢查，再沿用一般進化的公開、付款與連結程序。不足時不公開進化卡、不付款、不消耗進化次數，達標的墓場卡也不會被移動。這個入口不增加其他能力本體讀取原始費用或選取隱藏區域的權限。

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

目前 `create: {token: フェアリーウィスプ, n: 2, to: ex, result: made}` 支援固定數量、單一規則名稱、自己 EX；`to: field` 支援從者與護符 token 至自己場上。省略 `n` 為 1；EX 與場上各最多五張，只建立並放入仍有容量的數量。`result` 或 `bind` 擇一，綁定實際放入的 token，滿區時為空；authority receipt 同樣只列出成功放入者。token 的整卡 runtime 程式仍須准入。場上建立共用進場事件與守護的選擇橫置程序；沒有先前區域的建立以 `EntryOrigin::Other` 表示，成功建立的每個 token 各發生一次進場。

回合時點自動能力可用 `auto: {zone: ex, on: {phase: {whose: self, which: end}}, do: [...]}`，只在持有卡位於 EX 時訂閱；省略 `zone` 仍為場上。EX 訂閱目前只接受單一區域的 Main／End 時點，並不授予從 EX 打出的許可。條件中的 `count: {who: self, zone: field, kind: follower, trait: 魔法使い}` 讀取結算當下符合條件的場上張數；私有區域計數仍不接受卡型或類型篩選。普通法術可在自身結算期間以 `move: {ref: this, to: ex}` 移動自身；EX 已滿則仍走既有結算清理。

墓場的自身回場訂閱可寫 `auto: {zone: cemetery, on: {played: {who: self, sel: {kind: spell, class: ドラゴン}, during: {turn: self}}}, cost: {pp: 1}, do: [{move: {ref: this, to: field}}]}`。只在自己回合、自己適正打出指定職業法術時擷取當時已在墓場的來源；該法術本文才送墓的來源不補觸發。多張來源各自待機、各自宣告及支付 PP；拒付或不足費用不回場，滿場時付款後沒有成功移動。此准入只接必需頭付款與單一步驟自身回場，其他區域、任意目標回場及一般事件欄位讀取仍維持原邊界。

自己墓場的卡可放在 `targets`，例如 `from: {who: self, zone: cemetery, kind: follower, trait: 竜族}`；共用正式目標程序在付款前固定候選與宣告，沒有必需目標時不能付款。結算本文用 `move: {ref: returned, to: hand}` 回收。不能用本文的 `choose` 代替正式目標，否則會把本次付款才送墓的卡納入候選。這個目標入口不擴充回場的種類或容量選擇能力。

公開場上物件移至 EX 的 `move` 可用 `result: moved`，後續 `if: {cond: {did: moved}, then: [...]}` 只在至少一個成員實際移動成功時執行；選零或目的 EX 已滿不算成功，搬兩張也只執行一次後句。這個子集只接受已綁定的 Move 收據名稱，不接受其他結果種類、過濾條件或前向／分支外引用；一般 EX 搬移的部分容量選擇仍明確拒絕並還原，不因此擴充容量決策。

動態數量另接受本次 `destroy: {ref: ..., result: broken}` 的成功回條：`n: {count: {in: {objects_of: broken, view: before}, as: victim, where: {cmp: {left: {player: {master_of: victim}}, op: eq, right: {player: p}}}}}`。只計本次實際從場上被破壞者，按離場前控制者區分，包含隨後消去的 token；破壞失敗不計數，不按目前區域或擁有者重新計算。

`each_player: {as: p, stages: [[{create: {token: ..., n: ..., to: field, by: p}}]]}` 支援單一、無選擇的 token 建立階段，雙方在同一批次建立並進場；守護進場選擇按回合玩家優先處理，在所有選擇完成後一次提交。建立與計數須指向同一玩家；各目的區沿用容量上限與守護進場流程，守護由該 token 的控制者選擇。此入口不接受其他回條、after 計數、owner 比較、其它階段動作或結果匯出。整個能力完成後才進入衍生觸發能力的處理。

手牌作費用的 `discard.sel` 支援 `class` 與 `orig_cost_eq/ge/le`，可與種類、特徵、名稱交集。篩選讀取印刷職業與原始費用，不受目前費用修正影響。觸發能力的頭費用沿用宣告、正式目標、私有材料選擇、付款的順序；拒付不執行本文，材料不足不提交付款。

BP22-068 的下一次規則起身限制使用下列 `apply`；`who` 的目前支援子集為 `self/opp`，兩處須一致，期限只接受下一個 start 的 end。安裝綁定所選物件世代與指定玩家，經實際回合排程等到該玩家下次開始階段的規則起身，隨後失效。效果造成的 `stand` 不受阻止，來源離場也不提早終止；目標離場後的新世代不沿用限制。

```yaml
apply:
  on: enemy
  effect:
    cannot:
      what: stand
      on: affected
      during: { phase: start, who: opp, cause: rule }
  until:
    next_phase: { who: opp, phase: start, edge: end }
```

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

效果本文的 `evolve: {ref: this}` 可在結算時從控制者未公開的進化牌堆選一張同名普通進化卡，沿既有進化程序處理，並產生真正的進化時事件；不另扣 PP／EP／SEP，也不占用一般進化次數。只准入自身普通進化，其他對象、指定名稱及超進化仍須另行准入。沒有合法進化卡時照既有結算選擇契約回答空集合，已支付成本不回退。`entered_from: {ref: this, from: hand}` 讀取這次入場事件原有區域，可用 `not` 表示非手牌；不等同能力入場。場上 `count` 保留既有種類／trait 篩選；本批滿場判斷使用不帶篩選的場上卡數（包含護符）。

公開區域的數值讀取可用 `distinct_names` 計算不同卡名數，或用 `distinct_orig_costs` 計算不同原始費用種類數。兩者與 `count` 共用公開區域選取、種類／職業／類型篩選及費用條件讀值；原始費用取印刷資料，不受物件當前費用影響，同名或同費用的多張卡只計一次；沒有費用資訊的卡不另算作一種 0 費。可用於能力本體條件、動態數值與打出費用修正；`at: resolution` 延續墓場結算起始讀取，不開放隱藏區域或 `other`。

傷害的 `to: {any_of: [{leader: opp}, {who: opp, zone: field, kind: follower}]}` 可把領袖與從者放進同一次傷害的受傷者集合。它沿用既有 `RecipientSet::Union` 與去重規則，只有一個傷害動作及數量回條；傷害替代條件對同批所有受傷者生效。
