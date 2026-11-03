#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the variable payment scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

const COMPANIONS: [&str; 4] = [
    "variable-idol",
    "variable-horse",
    "variable-other",
    "variable-enemy",
];
fn prepared() -> &'static SelectedEffects {
    static PREPARED: OnceLock<SelectedEffects> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|author| matches!(author["input"]["card_no"].as_str(), Some("BP15-PR12" | "CP01-003" | "CP02-T01" | "BP03-T02")));
        // Blank companions isolate the printed cost from unrelated subscriptions.
        for (number, traits) in [("variable-idol",vec!["アイドル"]),("variable-horse",vec!["ウマ娘"]),("variable-other",vec![]),("variable-enemy",vec![])] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","traits":traits,"cost":"1","power":"1","hp":"20","text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot,&keywords,&input)
    })
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "variable-material",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in COMPANIONS {
        game.load_internal_card(number, &blank).unwrap();
    }
    game
}
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a question")
    };
    question
}
#[test]
fn solo_token_pays_zero_or_two_idols_and_reads_that_payment_for_double_damage() {
    for number in ["BP15-PR12", "BP20-T13"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"a","card":"variable-idol"},{"id":"b","card":"variable-idol"},
            {"id":"c","card":"variable-idol"},{"id":"other","card":"variable-other"},
            {"id":"spent","card":"variable-idol","state":{"acted":true}}
        ]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"variable-enemy"}]);
        let baseline = game(&initial);
        for count in [0_u64, 2] {
            let mut game = baseline.clone();
            let mut play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            while let CardPlay::Awaiting(question) = play {
                let answer = match &question.domain {
                    InputDomain::Targets(_) => Answer::Targets(vec![0]),
                    InputDomain::CostMaterials(domain) => {
                        assert_eq!(domain.members.len(), 3, "only unspent idols are candidates");
                        assert_eq!(domain.cardinality.minimum, Nat(0), "zero payment is legal");
                        let before = game.projection(View::Referee).unwrap();
                        game.answer_internal_card(
                            question.actor,
                            question.token,
                            Answer::CostMaterials(vec![0, 0]),
                        )
                        .unwrap_err();
                        assert_eq!(
                            game.projection(View::Referee).unwrap(),
                            before,
                            "invalid payment preserves the play"
                        );
                        Answer::CostMaterials((0..count).collect())
                    }
                    _ => panic!("unexpected variable payment question"),
                };
                play = game
                    .answer_internal_card(question.actor, question.token, answer)
                    .unwrap();
            }
            resolved(play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                count * 2,
                "damage reads the paid amount"
            );
            assert_eq!(
                state["objects"]["a"]["acted"],
                count > 0,
                "only selected idols are acted"
            );
            assert_eq!(
                state["objects"]["b"]["acted"],
                count > 0,
                "only selected idols are acted"
            );
            assert_eq!(
                state["objects"]["c"]["acted"], false,
                "an unselected eligible idol stays standing"
            );
            assert_eq!(
                state["objects"]["other"]["acted"], false,
                "other traits cannot pay"
            );
            assert_eq!(state["P1"]["ex"], json!([]), "the played token leaves EX");
        }
    }
}
#[test]
fn smart_falcon_returns_other_horses_and_keeps_the_paid_count_after_they_leave() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP01-003"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"a","card":"variable-horse"},{"id":"b","card":"variable-horse"},{"id":"other","card":"variable-other"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"variable-enemy"}]);
    let mut game = game(&initial);
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "source enters before its fanfare"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("pending")
    };
    let declaration = awaiting(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(
        declaration.domain,
        InputDomain::OptionalCost,
        "head payment is declared before selecting materials"
    );
    let materials = awaiting(
        game.answer_internal_card(
            declaration.actor,
            declaration.token,
            Answer::OptionalCost(true),
        )
        .unwrap(),
    );
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("materials")
    };
    assert_eq!(
        domain.members.len(),
        2,
        "the source and other traits cannot be materials"
    );
    resolved(
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![0, 1]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"],
        json!(["a", "b"]),
        "both materials return to hand"
    );
    assert_eq!(
        state["P1"]["field"],
        json!(["other", "source"]),
        "source remains on the field"
    );
    assert_eq!(
        state["objects"]["enemy"]["damage"], 4_u8,
        "damage uses the paid count after movement"
    );
}
