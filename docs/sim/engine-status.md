# 規則引擎現況與已知限制

`sim/engine`（crate `sve-engine`）目前是 D 階段留下、經 M0 修正的**原型**，不是正式引擎。
這份文件只記錄長期成立的邊界與契約；**進度、待辦與尚未重新確認的限制**由
[#478「模擬器引擎：範圍與進度」](https://github.com/gbaian10/sve-kit/issues/478) 追蹤。

- 逐項的錯誤、預期結果與處理狀態：[M0／M1 已知錯清單](known-errors.md)
- 載入期會拒絕哪些 YAML：[載入期拒絕的 YAML](rejected-yaml.md)
- 效果 DSL 的正式規格：[docs/dsl/](../dsl/README.md)

文件裡沒有寫的功能，不代表已經支援；#478 標「待確認」的項目沒有逐項重新驗證，
補上測試或核對程式之前，不要當成已修好，也不要當成仍然成立。

## 對 DSL 的支援範圍

- 引擎讀的是 D 階段的原型文法 `astra/1`：`authored/effects/*.yaml` 與 `dsl/effects.schema.json` 都是這個版本。
  `astra/1` 的語法以該 Schema 為準，語義以引擎程式與測試為準
- [DSL 1.0](../dsl/README.md)（`sve-author/1.0`）目前只有規格：還沒有對應的 JSON Schema、載入器或 IR，
  也還沒有卡片用它撰寫。引擎改讀 DSL 1.0 之前，這裡的限制都是針對 `astra/1`；兩者的落差見 #478
- Schema 通過不代表卡文正確，也不代表引擎能正確執行；載入期檢查只擋下已知不支援的構造（見下一節）

## 載入期檢查

`Catalog` 載入 `authored/` 時，除了 JSON Schema，還用 `sim/engine/src/catalog/semantics.rs` 檢查每張卡。
檢查的對象是**已列舉且實際比對**的種類與值：各位置可執行的 opcode 清單、引擎會發出的誘發事件、
已知的讀值路徑與選擇器欄位、有到期處理的期間，以及 [rejected-yaml.md〈檢查項目〉](rejected-yaml.md) 列出的參數。
不在清單內的會帶卡號與行號拒絕，被拒絕的卡永不執行。

通過載入只是必要條件，不代表能正確執行：仍須靠執行測試與逐卡語義審查。
已知會通過載入卻靜默不做或讀錯的構造，列在 [rejected-yaml.md〈尚未涵蓋〉](rejected-yaml.md)，進度在 #478。

## 驗證方式

測試數量與通過結果會隨卡表快照與程式改變，不在文件裡保存。需要確認時直接執行：

- 共用測試與共同題本 `tests/rules-scenarios/`、已知失敗清單 `tests/engine/known-failures.yaml`；
  `authored/` 的載入結果必須與 `tests/engine/rejected-yaml.yaml` 完全相同
- 執行方式與 `SVE_TEST_SNAPSHOT` 的設定見 [sim/engine/README.md](../../sim/engine/README.md)

題本通過只表示這些局面正確，不能推論任意卡片、初始狀態或組合都正確。
過去某個日期的基準與結果是歷史紀錄，見 [known-errors.md〈基準〉](known-errors.md)。

## 執行邊界

- 解算燃料 20,000 步、部分列舉上限 10,000 是原型的防護界線，不是 CR 15.2 的循環判定（見 `sim/engine/src/game/effects.rs`、`payments.rs` 等的常數）
- 場上（`field`）上限固定為 5，從者與護符放不下時依規則處理（見 `sim/engine/src/game/rules.rs`、`legal.rs`）
- 引擎不驗牌組構築合法性（KE-21）；牌組合法性屬建牌器範圍
- 尚未支援的動作原子、選擇、付款、持續效果、取代、區域與誘發構造，以 #478 的清單為準；
  載入期拒絕的部分以 [rejected-yaml.md](rejected-yaml.md) 為準

## AI

- 根節點確定化取樣（profile 的 `samples`）加固定啟發式延伸，展望到當回合、最多 128 個決定深度（`sim/engine/src/ai.rs`）。
  不是完整的 IS-MCTS，沒有對手策略學習或跨回合價值
- Rust 只載入 `profiles.yaml` 列出的 profile（`sim/engine/profiles/`）；
  [docs/ai/](../ai/profile.schema.json) 的 v2 profile 是未實作的設計，現行載入器不接受
- `Game` 以深複製分支，只有 `Catalog` 以 `Arc` 共用；先驗、數值宣告與複製成本的限制見 #478

## 回放、輔助與資訊邊界

本節是設計契約；各項實作是否已重新驗證見 #478。

- 玩家投影不送出未公開手牌或背面起始護符的固定 ID；裁判視角與存檔含秘密，不能當一般封包傳送
- 回放原型保存完整 `Game` 快照；輔助模式目前只有手動攻擊、生命設定與兩層對齊。持久化、續體、`realign` 的語義與
  網路／權限／不可信存檔驗證，現況與缺口見 #478
- 現行 digest 尚未綁定 `Catalog` 內容指紋，不能當成版本相容的保證

## 建置目標

函式庫設計上可用 `--no-default-features` 編譯到 `wasm32-unknown-unknown`；
JS 綁定、瀏覽器宿主與計時驗證尚未建立，見 #478。
