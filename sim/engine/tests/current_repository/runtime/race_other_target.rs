#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        // Named companions isolate conditions from the other cards' unadmitted abilities.
        for (number, name) in [("ready-daiwa", "ダイワスカーレット"), ("ready-teio", "トウカイテイオー")] {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":"フォロワー",
                "cost":"0","power":"2","hp":"2","traits":["ウマ娘"],"text":null,"sections":[]
            }]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}
fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"carrot","card":"CSD01-032"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"CP01-007"},{"id":"second","card":"BP01-171"},
        {"id":"third","card":"CP01-073"},{"id":"tail","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        fixture().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-cp01",
    )
    .unwrap();
    load_companions(&mut game);
    game
}
fn load_companions(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["ready-daiwa", "ready-teio"] {
        game.load_internal_card(number, &empty).unwrap();
    }
}
fn finish(game: &mut Game, mut step: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::DeclareName(_) => panic!("unexpected card name declaration"),
            InputDomain::DiceReroll { .. } => panic!("unexpected fixture dice reroll"),
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .first()
                    .map(|m| vec![m.handle])
                    .unwrap_or_default(),
            ),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .first()
                    .map(|m| vec![m.handle])
                    .unwrap_or_default(),
            )),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            }
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::CostMaterials(_)
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. } => {
                panic!("unexpected printed question: {:?}", question.domain)
            }
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}
fn settle(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability");
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step, 1);
    }
}
fn feed(game: &mut Game, card: &str) {
    meal_race::feed(game, card, 0, false);
    settle(game);
}

#[test]
fn cp01_019_racing_stands_another_follower_but_forbids_both_attack_targets() {
    for card in ["CP01-019", "CP01-P13"] {
        let mut initial = position(card, "field");
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"ally","card":"BP01-171","state":{"acted":true}}));
        let mut game = game(&initial);
        feed(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], 3_u8);
        assert_eq!(state["objects"]["ally"]["acted"], false);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"ally","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "cannot-attack"
        );
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"enemy"}))
                .unwrap()
                .outcome,
            "cannot-attack"
        );
    }
}
