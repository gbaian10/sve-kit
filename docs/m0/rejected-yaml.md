# 載入期拒絕的 YAML

`Catalog` 載入 `authored/` 時，除了 JSON Schema，還會用 `sim/engine/src/catalog/semantics.rs`
檢查每張卡。不通過的卡記在 `Catalog::rejections()`（`檔案:行: 訊息`），**永不執行**：
任何用到它的操作都回 `Unsupported("card program rejected at load: …")`。

```bash
cargo run --locked -p sve-engine --bin sve-prototype -- "$SVE_TEST_SNAPSHOT" . validate target/validate
```

機器可讀清單在 [rejected-yaml.yaml](rejected-yaml.yaml)；共用測試
`authored_yaml_loads_or_is_a_listed_rejection` 要求實際結果與清單完全相同（多一張、少一張都失敗）。

## 檢查項目

| 項目 | 拒絕的情況 |
| --- | --- |
| opcode | 該位置沒有實作（本文、靜態能力、費用、構築各有清單），含 `unsupported`、`declare_name`、`swap` |
| 誘發事件 | 引擎不會發出的事件名；階段事件帶 `subject`、非階段事件帶 `side`；非誘發能力帶 `event`／`trigger_if` 等 |
| read 路徑 | 未知的回合計數、物件欄位、玩家屬性、參照；`event.*` 讀了該事件沒有的欄位或在誘發以外讀；`x` 沒有 `variables.x`；`paid.*` 沒有對應的追加費用；`choice.*` 在 `replace_choice` 以外 |
| 選擇器 | 未知的 `type`、`keyword`、`ability_event`；參照既不是內建也不是本卡綁定的名稱 |
| 期間 | `until`、`during` 沒有到期處理的值 |
| 其他參數 | `restrict` 沒有程序檢查的動作、未實作的抑制事件、非計數器的 `counter` 名稱、`draw`／`look` 的 `up_to`、本文中的 `modify.type／traits／cost／set_cost`、未知的 `distinct_by` |
| 指稱 | 同一行、同類（誘發／宣告）且內容不同的能力沒有各自的 keyword（契約 3.1） |

## 既有 422 張的結果

421 張通過，1 張拒絕：

| 卡 | 位置 | 原因 | 處理 |
| --- | --- | --- | --- |
| BP10-T09 | `authored/effects/BP10.yaml:378` | 「武装・タイプを持つ」寫成 `modify.traits`，引擎不能執行（原本執行時才回 Unsupported） | 維持拒絕；需要實作種族授予（持續效果層），見 known-errors KE-20 |

第一版檢查另外拒絕了 11 張，處理如下（細節見 [known-errors.md](known-errors.md) 的 KE-07～KE-14）：

| 卡 | 第一版的拒絕原因 | 處理 |
| --- | --- | --- |
| BP14-074、BP14-081、BP20-P61 | 誘發事件 `banish`／`card_play`／`fusion` 不存在 | 引擎補上事件（KE-07） |
| BP20-P65 | `replace_choice` 未實作 | 引擎補上（KE-10） |
| BP16-036 | `self.turn.evolved` 不存在 | YAML 改讀 `evolutions`（KE-13） |
| BP20-P28 | `item.type` 不存在 | YAML 改 `union` 選擇器（KE-14） |
| CP03-009 | 同行兩個誘發能力沒有各自的 keyword | 補 `on_drive_gain`（KE-04） |
| BP21-PR04 | BOX 的 `until` 寫法與 BP18-SP01 不一致（兩者引擎都忽略，BOX 期間固定） | 檢查只接受引擎實際的 `next-controller-end`，BP18-SP01 改成同一拼法 |
| BP07-P06、BP15-SL13、EBD02-007 | `paid.*` 不在同一能力 | 檢查規則過嚴：靜態費用減免讀的是同卡打出能力的追加費用，改為同卡範圍 |

另外用「YAML 用到的每個欄位是否有程式讀取」的盤點找到檢查規則還沒涵蓋的
`restrict action: draw`（BP10-076）、`until: next-opponent-turn-end`（BP02-090、BP02-SL17）、
`search.groups`（CP04-088）、`name_alias`（BP03-078、BP08-T01、BP08-T02），引擎補實作後
把這些值加進檢查（KE-08、KE-09、KE-11、KE-12）。
