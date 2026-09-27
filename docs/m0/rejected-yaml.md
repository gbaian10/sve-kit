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
| 誘發事件 | 引擎不會發出的事件名；階段事件帶 `subject`、非階段事件帶 `side`；非誘發能力帶 `event`／`trigger_if` 等；`trigger_if` 讀事件限定（`event.*`、`is_active`）以外的值（R-0009：長寫條件寫在本文） |
| read 路徑 | 未知的回合計數、物件欄位、玩家屬性、參照；`event.*` 讀了該事件沒有的欄位或在誘發以外讀；`event.subject.*` 不是事件快照實際有的鍵；任何 `event.target.*`；`x` 沒有 `variables.x`；`paid.*` 在本卡沒有對應的追加費用；`choice.*` 在 `replace_choice` 以外；`damage.amount` 在 `replace_damage` 以外；`item.*` 在選擇器 `where` 以外 |
| 選擇器與參照 | 未知的 `type`、`keyword`、`ability_event`；參照既不是內建也不是本卡綁定的名稱；`target.<key>`／`cost.<key>` 的 key 不是本卡任何選擇的 key；`different_from` 指向不存在的選擇；`create.name`、`pilot.by` 內的讀值與參照 |
| 期間 | `until`、`during` 沒有到期處理的值 |
| 函式參數 | `fn` 的參數個數不符（add 等二元 2 個、not 等 1 個、min／max 1 或 2 個、and／or 1 個以上；引擎原本會靜默丟掉多的參數） |
| 其他參數 | `restrict` 沒有程序檢查的動作、未實作的抑制事件、非計數器的 `counter` 名稱、`draw`／`look` 的 `up_to`、本文中的 `modify.type／traits／cost／set_cost`、未知的 `distinct_by`、`damage.split` 沒有對應帶 `distribute` 的選擇、`replace_move` 的 `from`／`replacement` 區域、`move … to: banish`（要寫 `op: banish`）、從者／護符的 `name_alias` 不是 `while_zone: field`（CR 10.3.5、Q424） |
| 指稱 | 同一行、同類（誘發／宣告）且內容不同的能力沒有各自的 keyword（契約 3.1） |
| 行號 | 每個拒絕都要能定位到卡內的行（共用測試要求行號大於 0） |

## 尚未涵蓋（會靜默不做或回 null／0，載入期還抓不到）

| 構造 | 執行期結果 | 預定 |
| --- | --- | --- |
| 選擇器的 `trait`、`name`、`name_contains` 拼錯（不存在的種族或卡名） | 永遠沒有候選 | M2：對照卡表快照的種族、卡名、token 名 |
| `turn.phase` 或其他讀值與字面值比較時字面值拼錯（例 `"strat"`） | 條件恆真或恆假 | M2：型別化 IR 的列舉值 |
| `bind` 名稱全卡共用，不分能力、不分先後 | 在另一個能力或更後面才綁定的名稱讀到空集合 | M2：綁定的作用域與先後檢查 |
| 行號用子字串定位：同卡同字串多次出現時指向第一處；值加引號或流式寫法時退回卡號那一行 | 行號可能不精確（不會是 0） | M2：由 YAML 解析器帶位置 |
| 一般的「宣告了但沒有程式讀取」欄位 | 靜默忽略 | 目前靠手動盤點（見下）；M2 以型別化 IR 的 `deny_unknown_fields` 取代 |
| CR 10.7.6 的狀態誘發 | 若寫成 `trigger_if` 會被 R-0009 規則以錯誤理由拒絕（見 known-errors KE-28） | 遇到時另設構造 |

## 既有 422 張的結果

421 張通過，1 張拒絕（審核第 1 輪加嚴檢查後，另外抓到 BP03-078、SCS01-007，已修 YAML，見下）：

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

審核第 1 輪（`sve-kit/spec/design/m0-review/opus-r1.md`）後加嚴的檢查又抓到兩張，已改 YAML：
SCS01-007（`trigger_if` 讀回合計數，R-0009，M-003）、BP03-078（別名 `while_zone: any`，M-004）。
