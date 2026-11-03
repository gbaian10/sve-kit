#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate these printed payment scenarios."
)]
use super::*;

fn prepared() -> &'static SelectedEffects {
    static PREPARED: OnceLock<SelectedEffects> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|author| matches!(author["input"]["card_no"].as_str(), Some("BP15-039" | "BP22-117" | "BP22-118" | "BP15-PR12" | "CP02-T01" | "BP03-T02")));
        // Inert companions isolate the printed quorum and payment predicates.
        for (number, traits) in [("x-light-idol", vec!["アイドル"]), ("x-light-uma", vec!["ウマ娘"]), ("x-light-other", vec![]), ("x-light-enemy", vec![])] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","traits":traits,"cost":"1","power":"2","hp":"20","text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "x-payment-light",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "x-light-idol",
        "x-light-uma",
        "x-light-other",
        "x-light-enemy",
    ] {
        game.load_internal_card(number, &blank).unwrap();
    }
    game
}

fn token_name(game: &Game, zone: &str, name: &str) {
    let state = game.projection(View::Referee).unwrap();
    let tokens: Vec<_> = state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| !matches!(id.as_str(), Some("source")))
        .collect();
    assert_eq!(tokens.len(), 1, "exactly the printed token is created");
    let number = state["objects"][tokens[0].as_str().unwrap()]["card"]
        .as_str()
        .unwrap();
    let (snapshot, _, _) = inputs();
    let row = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["number"] == number)
        .unwrap();
    assert_eq!(
        row["faces"][0]["name"], name,
        "the token uses the authoritative printing"
    );
}

fn declare(game: &mut Game, line: u64) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending")
    };
    let option = options.iter().find(|option| option.line == line).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay, materials: u64) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    2,
                    "only the two other own Uma cards can pay"
                );
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
                    "duplicate materials preserve the play"
                );
                Answer::CostMaterials((0..materials).collect())
            }
            _ => panic!("unexpected printed payment question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play);
    game.resume_internal_card().unwrap();
}

#[test]
fn bp15_039_creates_the_complete_solo_only_with_three_own_idols_including_itself() {
    for (number, count) in [("BP15-039", 2_u8), ("PR-418", 1)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..count)
                .map(|i| json!({"id":format!("idol-{i}"),"card":"x-light-idol"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"foreign","card":"x-light-idol"}]);
        let mut game = game(&initial);
        assert!(
            matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the printed follower enters"
        );
        let play = declare(&mut game, 2);
        finish(&mut game, play, 0);
        if count == 2 {
            token_name(&game, "ex", "奏絶の独唱");
        } else {
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["ex"],
                json!([]),
                "foreign idols do not satisfy the own-field quorum"
            );
        }
    }
}

#[test]
fn bp22_118_resolves_both_real_evolution_heads_and_buffs_by_the_other_uma_act_payment() {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP22-117"},{"id":"uma-1","card":"x-light-uma"},{"id":"uma-2","card":"x-light-uma"},{"id":"other","card":"x-light-other"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-118"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"x-light-enemy"},{"id":"foreign","card":"x-light-uma"}]);
    let mut game = game(&initial);
    assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":1_u8}})).unwrap().outcome, "resolved", "the real base super-evolves through the rule procedure");
    let before = game.projection(View::Referee).unwrap();
    let super_play = declare(&mut game, 3);
    finish(&mut game, super_play, 2);
    let paid = game.projection(View::Referee).unwrap();
    assert_eq!(
        paid["objects"]["source"]["power"].as_i64().unwrap(),
        before["objects"]["source"]["power"].as_i64().unwrap() + 2,
        "power reads the two paid cards"
    );
    assert_eq!(
        paid["objects"]["source"]["hp"].as_i64().unwrap(),
        before["objects"]["source"]["hp"].as_i64().unwrap() + 2,
        "health reads the same paid cards"
    );
    for id in ["uma-1", "uma-2"] {
        assert_eq!(
            paid["objects"][id]["acted"], true,
            "selected Uma cards are acted"
        );
    }
    for id in ["source", "other", "foreign"] {
        assert_eq!(
            paid["objects"][id]["acted"], false,
            "the source, wrong trait and opponent cannot pay"
        );
    }
    let ordinary_play = declare(&mut game, 2);
    finish(&mut game, ordinary_play, 0);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        4_u8,
        "the ordinary evolution head also resolves, as Q2859 requires"
    );
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "both printed heads finish"
    );
}
