# 公開 annotation 固定案例規格

[public-annotation-cases.json](public-annotation-cases.json) 是 #498 的共用測試輸入與固定預期，
不是測試程式或已通過紀錄。所有文字、概念及卡片識別皆為自撰合成值，不含官方卡文。
欄位與錯誤分類依[公開契約](public-annotation.md)，形狀依[機器 Schema](public-annotation.schema.json)。

## 1. 機器入口與 mutation

頂層恰有 `format:1,kind:public_annotation_contract_cases,fixtures,cases`。
cases 每筆恰有 `{group,id,operation,input,expected}`；完整案例 ID 是 `group/id`，不得重用 ID 改變既有意思。
expected 的 result 為 accept／reject，reject.reason 是固定責任分類，不要求 Python／TS 例外文字相同。
accept 中列出的欄位都要逐值比對；不能只驗「沒有拋錯」。

`input.fixture` 指 fixtures 的鍵，深複製後依 `changes` 的 JSON Pointer→值替換；空 pointer 代表整個輸入。
changes 依檔案中順序處理，不建立中間缺失的節點，只允許新增 object 的最末鍵。
`remove` 其後逐項刪除指定欄／陣列元素，目標必須存在；無 changes／remove 就保留原值。
input 其他欄位是操作參數，不混入公開列。含 raw_json 的案例保留字面 bytes，不先 parse/stringify 修復。

Schema 操作以 `#/$defs/<definition>` 驗個別元件；`Admission` 只驗 manifest 的三個准入欄，
**不是完整 manifest**。Schema 本身採 Draft 2020-12，全部 `$ref` 在檔內，無線上載入。
`x-columns`／`x-types` 固定新版 descriptor；reader 要比對兩者，不能只驗 tuple 的 JSON 型別。
整數的 `1.0` 表示、surrogate、重複鍵與 canonical bytes 須在 JSON 值失去原表示前驗。
Schema 不可代替跨列引用、座標上限／重疊、ordinal／集合排序及 hash 檢查。

## 2. projection fixture 與真實 reader 接線

`jp_en`、`printed`、`mixed_mode`、`counterpart`、`different_sections` 是同一種測試情境物件，
不是新增公開封套或第二份卡表。其明列資料如下。

| fixture 欄位 | 內容／harness 責任 |
| --- | --- |
| languages | 本次合成 manifest 的語言登錄，source／target／text 的語言都必須在其中 |
| admission | 完整三欄准入資料；read_projection 預設 reader 契約為 3.0.0，支援的格式／能力為契約 §1 的固定值 |
| text_units | `[id,lang,text]`；exact 身分，全部為合成文字 |
| annotation_sets／field_annotations／translations／concepts／vocabulary | 對應公開表的完整 tuple；concepts 即 annotation_concept；陣列索引供 mutation 定位，不是公開 PK 排序 |
| field_translations | `{receiver:PublicTextPointer,value:FieldTranslation}`；harness 把 value 放回 receiver 的公開父 owner.translations |
| owners | `{owner,card_id,face_id,region,mapping_state,fields}`；fields 是 `[field,ordinal,text_unit_id?]`；明示合法來源及同卡同面的合成公開背景 |
| cards | 確認可公開的 card ID 目標；harness 建立相應合成 card／face／printing，不能由字串猜未列關係 |
| explanation_targets | `{kind,id,text_unit_id?}`；還原 keyword.definition／CR.text／ruling.decision 的公開閉包；null 表示缺正文 |
| support | region、shared_status、override_status、region_blocks；按既有 Support 順序驗有效狀態，不以 JP basis 改資格 |
| producer | source_exists／source_fresh／identity_confirmed／source_adopted／aligned／divergent／counterpart_fresh；全部是 P-only 的建置背景，不序列化到公開快照 |

只有 fixture 明列的 owner／field／text 才存在；未列的不補默認文字或用途。
harness 將使用到的情境嵌入隔離的完整 3.0 合成卡表，補齊既有格式必要的非本單欄位，按 PK 排序與裝檔。
未被情境使用的 scaffolding 不得改變 oracle；可用預先固定的最小 baseline，不能由 production producer 生成預期結果。
測試選擇撤掉 translation 等列時，harness 同步裁掉純粹失去用途的 scaffolding 閉包，
但不得刪掉本例故意引入的懸空引用、補回缺失目標或修復錯誤資料。

read_projection 的 accept 結果是情境最後一個 FieldTranslation（一般為 EN receiver）的來源 owner kind、
source／target exact text ID；display 則按 input.receiver／ui_lang／mode 取用，逐值比對 expected。
printed 的 mode 由 printing_face owner 表示；revision 由 face_revision 表示，不由當前 UI 名稱猜來源。

[#498](https://github.com/gbaian10/sve-kit/issues/498) 的 Python `read_snapshot`／`read_text_all`、TS `readSnapshot`／`readTextAll` 與
`sve_carddb.export.read_api.load_export` 必須讀取同一 cases 檔；publish 以本地已驗匯出根接線，無須外部上傳。
read_projection 的成功例比對固定 expected 及分片／text_all 一致；所有失敗例在 Python／TS 分別命中相同原因類別。
不得只寫一個 fixture 自訂 validator、只跑 Schema 或只驗 JSON 能讀，就稱 #498 的 reader 測試通過。

mutation 後 harness 重算合法外層 File bytes／hash／path／counts、dependencies、manifest 及 text_all，
讓錯誤能進到指定欄位；不得重算被本例故意弄壞的 text ID 或 annotation ID。
reason 按契約 §6 的目標責任判定；Schema 遇到已標明的 owner／range／basis 條件時映射到該分類。
fixture 未按公開排序保存是為了固定 mutation 位置；裝檔時按 PK 排序，但 Annotation.ranges／occurrences 的壞次序不得修復。

## 3. 操作及案例對照

| group／operation | 覆蓋與比對責任 |
| --- | --- |
| PA-01／admission、envelope_versions、read_projection | 未知 major／minor、舊格式、最低 reader、未知／缺少／重複／亂序能力、容器混版；空內容不減少能力 |
| PA-02／schema、descriptor、canonical_bytes | 各新／改 tuple 的 valid／少格／多格、未知及退役 basis、品質型別、私有鍵、descriptor 換欄、JSON 原始表示 |
| PA-03／annotation_identity、shared_annotation、read_projection | exact hash、同文字不同概念或 bold、同 set 多 owner、錯 source／target text；獨立固定完整 ann hash |
| PA-04／unicode_ranges、read_projection | 負值／空／逆序／越界／不安全整數／Bool、UTF-16 錯座標、range／occurrence 重疊、跳 ordinal、多次／多段引用 |
| PA-05／owner_field、read_projection | 所有 owner 及 field、keyword action、缺原文 annotation、錯面、借同字串、printed/current 混用、null definition |
| PA-06／concept_references、read_projection | glossary／vocabulary／card_name 到公開目標，category、雙鍵、卡片／說明正文閉包、缺 translation／text／set |
| PA-07／read_projection | own_source／jp_source／官方 counterpart、exact 原文、語言／authority、錯 card／face、未確認 mapping、EN 套 JP offset、禁止 JP section 拼接 |
| PA-08／display、produce、schema | 無 aligned／已知 divergence 仍顯示 JP 繁中，完整 JP effect、規則限制保留；來源 freshness／採納只由 produce 驗；不造 aligned 欄 |
| PA-09／unicode_ranges、canonical_bytes | 非 BMP、組合字元、CRLF、NFKC 改長度與 wrong text identity、未配對 surrogate；CP→UTF-16 固定邊界表及切片 |
| PA-10／display、unicode_ranges、annotation_emphasis | 日／英 UI 不回繁中、無譯文原文位置仍在、加粗開關、低信心、空字串與非法空字串 span |
| PA-11／size_accounting | 以實際整檔按 key 去重；混區檔全額計各區、annotation／說明屬文字、media 計首屏、text_all 是替代、增量不重複加總 |

unicode_ranges 先驗 text identity／set 的 text 引用與 scalar 字串，再驗各 range／ordinal，最後驗 annotation hash。
expected.slices 與 utf16_ranges 按 occurrence、再按 ranges 順序展平；不把多段引用中間的字也加粗。
annotation_identity 驗每個固定 hash 與 distinct；shared_annotation 驗用途不能因 set 去重而消失。
annotation_emphasis 驗 glossary category 及 vocabulary class/type 的固定加粗規則，不由文字猜類別。
concept_references 驗所有明列概念／引用／說明／卡片的閉包與 category，不要求有翻譯才能打開說明。
owner_field 只驗給定的來源 owner／field 定位；不自行生成另一個原文來源。
produce 驗 P-only 的先決條件；不能拿公開 reader 接受結構正確的 bytes 算 P 的反例失敗。
display 的 grants_aligned／grants_official_counterpart 固定 false，表示本次 JP 顯示不授資格，
不是覆寫既有獨立核對紀錄。size_accounting 的 bytes 是合成算例，`capacity_acceptance=not_measured`。

## 4. 驗收紀錄的邊界

本單可檢查 Draft 2020-12 Schema 合法、每個 fixture pointer 可定位、ID 唯一、正反例的形狀、
合成 text／annotation hash、Unicode 固定向量與計帳算術一致。
這些只證明案例規格可讀且自洽；#498 還須交 producer／Python／TS／Web／read_api／publish 的實際測試結果，
[#53](https://github.com/gbaian10/sve-kit/issues/53) 交全批與手機容量。規格合併與程式驗收必須分開記錄，不能以本文件取代後者。
