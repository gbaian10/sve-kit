#![expect(
    clippy::unwrap_used,
    reason = "Synthetic fixture failures must stop the test."
)]

use serde_json::{Value, json};

pub(crate) fn author(
    number: &str,
    id: &str,
    kind: &str,
    mut abilities: Vec<Value>,
) -> (Value, Value) {
    let card = format!("c:{id}");
    let face = format!("f:{id}");
    let revision = format!("revision-{number}");
    let lines = (1..=abilities.len()).collect::<Vec<_>>();
    for (index, ability) in abilities.iter_mut().enumerate() {
        let line = index.checked_add(1).unwrap();
        let fields = ability.as_object_mut().unwrap();
        fields.insert("line".into(), json!(line));
        fields.insert("ports".into(), json!({"imports":{},"exports":{}}));
        fields.insert(
            "sources".into(),
            json!([{
                "admission":"light","owner":{"kind":"face_revision","revision_id":revision},
                "face_id":face,"field":"effect","ordinal":null,"line":line,
                "scope":"ability_0","covers":[line]
            }]),
        );
    }
    let face_fact = json!({"name":number,"card_class":"ニュートラル","card_type":kind,
        "cost":"1","power":"1","hp":"2","traits":[],"text":"","sections":[]});
    let input = json!({"status":"accepted","printings":[number],"input":{
        "admission":"direct","card_id":card,"card_no":number,
        "source_payload":{"card_id":card,"faces":[{
            "face_id":face,"revision_id":revision,"ordinal":0_u8,"side":"front","cost":1_u8,
            "fields":[{"field":"effect","ordinal":null,"lines":lines}]
        }]},
        "tree":{"format":1_u8,"kind":"author_effect_set","scope":"shared","cards":{number:{
            "meta":{"dsl":"1.0","source":"direct","admission":"light",
                "written_by":[{"actor":{"who":"test","model":null},"date":"2026-10-28"}],
                "reviewed_by":[],"status":"draft","verified_by_exam":[],"rulings":[]},
            "faces":{face:{"abilities":abilities}}
        }}}
    }});
    (json!({"number":number,"faces":[face_fact]}), input)
}
