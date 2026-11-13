#![expect(
    clippy::panic,
    reason = "Unexpected synthetic decisions invalidate the event subject oracle."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;

/// `beast` carries trait A; `plain` carries none. Both cost 1 and enter from hand.
const CARDS: [(&str, &str); 3] = [("watcher", ""), ("beast", "A"), ("plain", "")];

fn catalog() -> Arc<Catalog> {
    let mut lines = Vec::new();
    let mut cards = serde_json::Map::new();
    for (number, traits) in CARDS {
        let traits: Vec<&str> = traits.split('・').filter(|t| !t.is_empty()).collect();
        lines.push(json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"2","traits":traits,"text":null,"sections":[]}]}).to_string());
        cards.insert(
            number.into(),
            json!({"status":"complete","review":"synthetic","abilities":[]}),
        );
    }
    let document = json!({"format":1,"kind":"effect_set","cards":cards}).to_string();
    Arc::new(
        Catalog::from_documents(
            &lines.join("\n"),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../authored/rules/keywords.yaml"
            )),
            &[("unit.yaml".into(), document)],
        )
        .unwrap(),
    )
}

fn tree(watcher: &Value) -> Value {
    let mut source = fanfare::expanded::tree().clone();
    let template = source["cards"]["BP08-038"]["abilities"][0].clone();
    let meta = source["cards"]["BP08-038"]["meta"].clone();
    let mut entry = template;
    entry.as_object_mut().unwrap().remove("fanfare");
    for (key, value) in watcher.as_object().unwrap() {
        entry[key] = value.clone();
    }
    source["cards"] = json!({
        "watcher":{"meta":meta,"abilities":[entry]},
        "beast":{"meta":meta,"abilities":[]},
        "plain":{"meta":meta,"abilities":[]}
    });
    source
}

fn lower(
    watcher: &Value,
) -> Result<BTreeMap<String, WellFormedCardProgram>, lowering::LoweringError> {
    let costs = CARDS
        .iter()
        .map(|(number, _)| ((*number).to_owned(), Nat(1)))
        .collect::<BTreeMap<_, _>>();
    lowerer().lower_with_printed_costs(&tree(watcher), &costs)
}

fn entry_auto(cond: &Value) -> Value {
    json!({"auto":{
        "on":{"enter_field":{"who":"self","sel":{"kind":"follower","other":true}}},
        "do":[{"if":{"cond":cond,
            "then":[{"buff":{"on":"this","power":2,"hp":2}}],
            "else":[{"buff":{"on":"this","power":1,"hp":1}}]}}]
    }})
}

/// The watcher's power after `entering` is played from hand and its trigger resolves.
fn grown(cond: &Value, entering: &str) -> Value {
    let programs = lower(&entry_auto(cond)).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"w","card":"watcher"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"e","card":entering}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "event-subject-condition",
    )
    .unwrap();
    for (number, program) in &programs {
        game.load_internal_card(number, program).unwrap();
    }
    let mut step = Some(game.play_internal_card("e", PlayerId { seat: 1 }).unwrap());
    for _ in 0_u8..16_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let InputDomain::WardEntry { .. } = &q.domain else {
                panic!("unexpected decision: {:?}", q.domain);
            };
            step = Some(
                game.answer_internal_card(q.actor, q.token, Answer::WardEntry(false))
                    .unwrap(),
            );
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
            continue;
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return game.projection(View::Referee).unwrap()["objects"]["w"]["power"].clone();
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("only the entry occurrence should remain");
        };
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
    panic!("unbounded entry resolution");
}

#[test]
fn a_branch_can_test_the_trait_of_the_entering_follower() {
    let beast = json!({"exists":{"in":"it","trait":"A"}});
    assert_eq!(grown(&beast, "beast"), json!(4));
    assert_eq!(grown(&beast, "plain"), json!(3));
}

#[test]
fn the_subject_test_composes_with_negation() {
    let not_beast = json!({"not":{"exists":{"in":"it","trait":"A"}}});
    assert_eq!(grown(&not_beast, "beast"), json!(3));
    assert_eq!(grown(&not_beast, "plain"), json!(4));
}

#[test]
fn a_head_without_an_event_subject_cannot_test_it() {
    let activated = json!({"activate":{"cost":{"pp":0},"do":[{"if":{"cond":{"exists":{"in":"it","trait":"A"}},
        "then":[{"buff":{"on":"this","power":1,"hp":1}}]}}]}});
    lower(&activated).unwrap_err();
}
