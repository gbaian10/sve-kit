# 四層翻譯固定案例規格

本文件與 [four-layer-cases.json](four-layer-cases.json) 是 [#498](https://github.com/gbaian10/sve-kit/issues/498) 的測試輸入／預期結果，
**不是測試程式或已通過紀錄**。契約依[四層翻譯](four-layer-translation.md)，公開 wire 反例另由 #496 定義。
JSON 只含自撰合成文字、語義值、來源識別及規格預期，不含官方卡文；不得為了方便執行而把私有全文補進此檔。

## 1. 固定資料的讀取方式

頂層恰含 `format:1,kind:four_layer_contract_cases,fixtures,cases`。
fixtures 是具名常數物件；cases 是有序陣列，每筆恰有 `{group,id,operation,input,expected}`，
group 為下表 FL 編號、id 為組內唯一 Code、operation 為下表操作；input／expected 都是 JSON 物件。
完整案例 ID 是 `group/id`；新增案例不得改變既有 ID 的意思。

input 的 `fixture` 指 fixtures 的 key，先深複製 fixture，再套用 `changes` 的 JSON Pointer→值；
`remove` 列的 JSON Pointer 表示刪除欄位。若有 `changes`，比較原值與變更後值；不是把替換後結果當基準。
其他 input 欄位是該操作的直接參數，不隱含預設值。沒有 fixture 的操作只讀自己 input。
JSON 中的 fixture owner／concept／domain ID 是合成登錄，測試須在隔離資料根建立，不連官網。
`identity_base` 的 `semantic` 是完整 frame-v1 hash payload，projection 只含分類；
同層 `projection_interface` 存 scopes／imports／exports，與 frame_id 組成共用契約 §7 的 frame-interface-v1 payload。
frame_identity 同時驗 frame ID 與 interface_key 的固定 hash；render 部分不入這兩個 hash。
expected 的 before_interface_hash／after_interface_hash 就是前後 interface_key 的完整 SHA-256 值。
此操作只比較已分類合成 payload 的身分；來源分類與接口是否合法另由 FL-006／FL-022 驗證。
其中的 normalizer／domain／discriminator 是測試專用具名規則，不宣稱正式 parser 已接受這種自撰來源語法。

operation 是測試責任名稱，不是要求實作同名 production 函式。#498 可用實際 public reader／builder 入口建立相同情境，
但不能跳過 input、改 expected，或只檢查檔案能載入就稱通過。
`reject` 表示該驗證邊界拒絕；`fallback` 是合法缺譯的整欄退回；`pending` 是資料保留而未取得可執行資格。
expected 的 reason 是本規格原因碼，實作可映射既有公開診斷，但必須可辨識相同失敗責任，不拿任意錯誤算通過。

## 2. 操作與覆蓋矩陣

| 組／operation | 固定輸入的重點 | 必須驗到的結果 |
| --- | --- | --- |
| FL-001／frame_identity | 同來源與葉 schema，只開關 NP、改 form／note／中文或有效的接口描述 | frame ID 相同；葉值不變；只有接口描述改動會改 interface_key |
| FL-002／frame_identity | targets/choose、cost/effect 變體、normalizer、域、canonical 或投影分類改動 | frame ID 必不同，interface_key 隨之改變；固定完整 hash 可獨立核對 |
| FL-003／source_unit | 計數物、計數時區域集合、token、角色與實際單位 | EX 体、跨區枚、護符つ／枚、主戰者人通過；錯單位拒絕合併 |
| FL-004／leaf_domain | 數量、次數、傷害、序數、布林與域 | 錯型別／角色或超界拒絕，不因數字相同合併 |
| FL-005／target_references | 完整 target、缺葉、錯概念、required 被 literal 取代 | 完整引用通過；必要葉遺失／懸空拒絕 |
| FL-006／projection | ability body/flag、card field、已知提示／layout、未知 | 五類分開；unknown 保留 pending，不當 none |
| FL-007／source_identity | exact owner／face／field／hash／來源版本 | 錯 card、面、欄、hash、過期或缺來源拒絕 |
| FL-008／source_positions | 自撰非 BMP、全形數字與 CRLF 的 raw／canonical trace | raw 重建不漏 byte；UTF-16 偏移、重疊、漏 CR、假 layout 拒絕 |
| FL-009／record_shape | 缺必填、未知欄、重複鍵 | A／C 各自拒絕，不靠最後讀入覆蓋 |
| FL-010／authored_entry | 新封套、舊 kind、品質欄 | format 3 通過；source_exception、舊格式、非法型別拒絕 |
| FL-011／missing_translation | 結構正確而選詞缺譯／撤回 | 整欄 fallback；不輸出半中半日，也不擅取 variant |
| FL-012／form_signature | locative／allative／ablative、未知 form、錯語言 | 合法形式通過，缺形式或錯參數拒絕 |
| FL-013／annotation_identity | 卡名內同字、相同 text 不同概念 | 卡名與術語不互套 span；不同概念產生不同 set／context |
| FL-014／occurrences | 重複詞、多個輸出與原文單獨顯示 | 保留每次位置；不同 exact text、越界或缺關聯拒絕 |
| FL-015／render_dependencies | 改 label、form、bold、target、品質或 note | 真正依賴重算 render／annotation，note 不失效，frame 不變 |
| FL-016／semantic_dependencies | 改來源、frame 語義、接口描述、本體版本 | 精確失效用途／DSL；接口描述只失效相依 DSL，不重鍵翻譯；中文風格不重審規則 |
| FL-017／zone_forms | 在／到／從、加入／回到手牌、計數來源與目的地 | 格與融合形式分清；目的地不改來源量詞，頂底不當普通 zone |
| FL-018／partial_np | 部分修飾、分支限定、異種聯集、特性／token／職業 | 未覆蓋可翻譯但不造已支援 DSL；不拉平 scope／型別 |
| FL-019／quantity | exact/up_to/at_least/all/any/X/may/random | 構造分工不互換；缺 import／不合法 expression 拒絕 |
| FL-020／owner_omission | 具名成本規則可解析、省略而未知 | 不全域補 self；未知保留 pending、相依 DSL 不可執行 |
| FL-021／semantic_roles | 關鍵字頭／賦予／篩選／門檻／選項、階段／期限 | 角色不同分 frame；別名同概念，不從《》或數字猜 keyword |
| FL-022／ports | selected set／receipt／capture、branch scope | 型別／scope 正確才連接；未支援仍 pending，不用泛用 it |
| FL-023／jp_selection | 無 aligned、divergence、段落不同、錯面／來源、unmapped | 有效 JP 繁中可用，DSL／counterpart 獨立；EN offsets 不套 JP |
| FL-024／ruling_mapping | 舊模板拆分、新框架合併、版本命名空間 | 按 occurrence 唯一 resolved，不按文字或 ID 前綴 |
| FL-025／ruling_mapping | 唯一候選但適用域擴大、舊域未知 | 不得 active；保持 pending 或拒絕假 resolved |
| FL-026／ruling_resolution | 無候選、多候選、漏引用、懸空或狀態矛盾 | 原因／候選保留，active 懸空為零 |
| FL-027／db_boundary | SQL Json 錯 shape、錯 FK、必填 null | C 拒絕原始 Any；不可因 authored 先驗過而略過 |
| FL-028／translation_use | 同字串不同欄位／owner 的合法性 | 逐 use 驗來源；共享 context 不借官方資格 |
| FL-029／candidate | 未完整綁定的譯文草稿、嘗試 pin／render | 可保留候選，不得進 active 或算完整覆蓋 |

未列實作名稱、SQL 排序或私有 helper 呼叫次數，避免把契約測試寫成實作鏡像。

## 3. 真實來源的回歸邊界索引

下表只記卡號與必驗語義，不保存卡文。私有 testdata 的鎖定 commit、檔案 hash 與合法讀取沿既有流程；
由 #498 選擇相應面／欄位後必釘 exact source identity，不能只用卡號猜第一面。
「保留」指未支援文法／DSL 可保留語義殘餘，必要引用錯誤仍失敗；不表示可以當 no-op。

| 邊界 | 卡號 | 對應固定組與預期 |
| --- | --- | --- |
| B01 | PCS02-001 | FL-002／FL-022：打出時目標與後句共用 binding，不能把目標當動作 receipt |
| B02 | CP03-SL24 | FL-003／FL-017：墓場計數與手牌目的地分開 |
| B03 | BP21-003 | FL-003／FL-018：EX 代幣從者的体、token 與特性分型 |
| B04 | BP08-058、BP16-014 | FL-003／FL-018：跨區聯集枚、分支修飾與整體數量不拉平 |
| B05 | BP21-P60、CP04-P80、BP21-P34 | FL-003／FL-019：護符つ／枚、X；不從名字猜 token |
| B06 | BP21-PR06、CP04-P29 | FL-018：主戰者／從者異種集合用具名型別或保留，不能全部變 Card |
| B07 | BP21-004、BP21-P52、BP21-006、BP20-SL19、BP21-SL20 | FL-017：在／到／從、加／回手牌、牌堆頂底 |
| B08 | BP21-005 | FL-019：任意控制不能用 upto=0 代替 |
| B09 | CP04-SL17、BP20-P32、BP21-005 | FL-020：省略持有者依具名構造或保留未解 |
| B10 | BP21-SL06、CP03-001、BP19-SL07 | FL-018／FL-021：特性鏈、職業圖示；名字內符號不是特性 |
| B11 | SD07-013、BP15-109、BP21-004 | FL-001／FL-018：長修飾可部分 NP，不能漏排除條件 |
| B12 | ECP02-003、BP20-P49、BP20-R15、BP21-SL15、BP21-P38 | FL-004／FL-019：存在下限、指示物、次數各有角色 |
| B13 | BP21-089、BP17-SL21、BP21-P24、BP21-P64 | FL-018／FL-019：語序變體、各組上限／合計限制，不用單一 union 代替 |
| B14 | BP20-P56、BP21-001、BP21-P69、BP20-R12 | FL-019：any/all/X/random 不混用 |
| B15 | PCS02-002、BP08-038 | FL-019：CountExpr、算式及 Live 求值不提前固定 |
| B16 | PCS02-003、BP21-SL28、BP21-P58、BP21-012、ECP02-001、BP20-R11 | FL-021：能力頭、賦予、篩選、條件及費用分角色 |
| B17 | EBD03-008、ECP02-SP09、ECP02-U09 | FL-021：同概念別名保留 trace，不改執行語義 |
| B18 | BP21-007 | FL-021：數字選項不是 keyword；付款時點另依 R-0006，不假稱本卡驗過付款 |
| B19 | BP20-063、EBD03-007、BP21-026、BP20-073 | FL-022：前後者、成功結果、取代、選定時 capture 分型與 scope |
| B20 | SD08-006、BP10-P18、BP10-P24 | FL-013：完整卡名引用；中文同字位置另用合成值驗 |

這些索引是來源邊界要求，不是重新發布官方文字，也不是任何卡已取得 DSL reviewed／verified 的證明。

## 裁定版本與引用層級固定案例

`ruling_document` 是完整自編裁定封套；`ruling_reference` 是帶檔案實際 hash、原引用 ordinal 與明示 null 的完整 RulingResolution。
`ruling_occurrences` 保存兩個完整 OccurrenceKey，`ruling_typed_target` 保存完整 target；舊 `occ:a` 等簡寫僅說明既有 split／merge 意義，不送正式邊界。
`ruling_document_shape` 走裁定 public reader，`ruling_reference_shape` 走封閉 Resolution 型別；accept 案例逐值比對。

本次 #498 的 builder／DB 測試保存相同引用字串的兩個原 ordinal；IR 引用原樣保留，pending 不得進入 active 依賴。
已知域的 split／merge、完整用途集合與候選來源檢查屬 #499 後續實作的規格案例，不能當成本次已通過的行為。
只改裁定 format／revision 的切換必須另驗去除這兩欄後 parsed payload 完全相同；此逐欄比對是切換驗收報告，不作為正式建置的外部輸入。
已知舊域、多用途、候選與完整用途集合的行為由 #499 補上，#498 只驗引用層級與非模板引用的保留。
