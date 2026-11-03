use super::*;
use sve_engine::game::internal::DecisionView;

const MATERIALS: [&str; 4] = ["hand-dragon7", "hand-dragon6", "hand-tokyo", "hand-inert"];

fn prepared() -> &'static SelectedEffects {
    static PREPARED: OnceLock<SelectedEffects> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|author| matches!(author["input"]["card_no"].as_str(), Some("BP11-068" | "BP18-119" | "CP02-T01" | "BP03-T02")));
        for (number, class, kind, cost, traits) in [
            ("hand-dragon7", "ドラゴン", "スペル", "7", json!([])),
            ("hand-dragon6", "ドラゴン", "スペル", "6", json!([])),
            ("hand-tokyo", "ニュートラル", "フォロワー", "1", json!(["透京"])),
            ("hand-inert", "ニュートラル", "フォロワー", "1", json!([])),
        ] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":class,"card_type":kind,"cost":cost,"power":"1","hp":"20","traits":traits,"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn board(number: &str, actor: PlayerId, quick: bool) -> Game {
    board_with_enemy(number, actor, quick, true)
}

fn board_with_enemy(number: &str, actor: PlayerId, quick: bool, enemy: bool) -> Game {
    let seat = if actor.seat == 1 { "P1" } else { "P2" };
    let opponent = if actor.seat == 1 { "P2" } else { "P1" };
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(if quick { opponent } else { seat });
    initial["players"][seat]["pp"] = json!({"current":2_u8,"max":10_u8});
    initial["players"][seat]["leader"]["life"] = json!(18_u8);
    initial["players"][seat]["zones"]["hand"] = json!([
        {"id":"source","card":number},
        {"id":"fuel","card":if number == "BP11-068" {"hand-dragon7"} else {"hand-tokyo"}},
        {"id":"wrong","card":"hand-dragon6"}
    ]);
    initial["players"][seat]["zones"]["deck"] = json!([{"id":"drawn","card":"hand-inert"}]);
    initial["players"][opponent]["zones"]["field"] = if enemy {
        json!([{"id":"enemy","card":"hand-inert"}])
    } else {
        json!([])
    };
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "quick-hand-real",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for material in MATERIALS {
        game.load_internal_card(material, &blank).unwrap();
    }
    game
}

#[expect(
    clippy::panic,
    reason = "Unexpected fixture questions fail the scenario."
)]
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected play question");
    };
    question
}

#[expect(
    clippy::panic,
    reason = "Unexpected fixture questions fail the scenario."
)]
fn start(game: &mut Game, actor: PlayerId, quick: bool) -> CardPlay {
    if !quick {
        return game.play_internal_card("source", actor).unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(
        question.actor, actor,
        "the nonactive controller owns this Quick question"
    );
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick window");
    };
    assert_eq!(
        options.len(),
        1,
        "only the fully payable source Spell is offered"
    );
    game.answer_internal_card(
        actor,
        question.token,
        Answer::Quick(Some(options[0].handle)),
    )
    .unwrap()
}

#[expect(
    clippy::panic,
    reason = "Unexpected fixture questions fail the scenario."
)]
fn pay_hand(game: &mut Game, question: &DecisionView) -> CardPlay {
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("hand material");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the matching hand card can pay"
    );
    let other = if question.actor.seat == 1 {
        View::P2
    } else {
        View::P1
    };
    assert!(
        game.internal_decision(other).is_none(),
        "hand cost choices remain private to their controller"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::CostMaterials(vec![0]),
    )
    .unwrap()
}

#[test]
fn thunder_roar_quick_uses_the_optional_printed_dragon_cost_and_paid_draw() {
    for (seat, accepted, quick) in [(1, false, true), (1, true, true), (2, true, false)] {
        let actor = PlayerId { seat };
        let name = if seat == 1 { "P1" } else { "P2" };
        let mut game = board("BP11-068", actor, quick);
        let declaration = awaiting(start(&mut game, actor, quick));
        assert_eq!(declaration.domain, InputDomain::OptionalCost);
        let target = awaiting(
            game.answer_internal_card(actor, declaration.token, Answer::OptionalCost(accepted))
                .unwrap(),
        );
        assert!(matches!(target.domain, InputDomain::Targets(_)));
        let answer = game
            .answer_internal_card(actor, target.token, Answer::Targets(vec![0]))
            .unwrap();
        let answer = if accepted {
            pay_hand(&mut game, &awaiting(answer))
        } else {
            answer
        };
        resolved(answer);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if accepted { 4_u8 } else { 3_u8 }
        );
        assert_eq!(state[name]["pp"]["current"], 0_u8);
        assert_eq!(
            state[name]["hand"],
            if accepted {
                json!(["wrong", "drawn"])
            } else {
                json!(["fuel", "wrong"])
            }
        );
        assert_eq!(
            state[name]["cemetery"],
            if accepted {
                json!(["fuel", "source"])
            } else {
                json!(["source"])
            }
        );
    }
}

#[test]
fn distorted_progress_both_printings_pay_before_damage_heal_and_draw() {
    let missing_actor = PlayerId { seat: 1 };
    let mut empty_plain = board_with_enemy("BP18-119", missing_actor, false, false);
    let authority_before = format!("{empty_plain:?}");
    empty_plain
        .play_internal_card("source", missing_actor)
        .unwrap_err();
    assert!(
        format!("{empty_plain:?}") == authority_before,
        "no legal target preserves all authority"
    );
    let mut empty_quick = board_with_enemy("BP18-119", missing_actor, true, false);
    empty_quick
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let empty_window = empty_quick.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(empty_window.domain, InputDomain::Quick { options, .. } if options.is_empty())
    );
    let unpaid_view = empty_quick.projection(View::Referee).unwrap();
    assert_eq!(unpaid_view["P1"]["pp"]["current"], 2_u8);
    assert_eq!(
        unpaid_view["P1"]["hand"],
        json!(["source", "fuel", "wrong"])
    );
    assert_eq!(unpaid_view["P1"]["leader"]["life"], 18_u8);
    for (number, seat, quick) in [
        ("BP18-119", 1, true),
        ("PR-481", 2, true),
        ("BP18-119", 2, false),
    ] {
        let actor = PlayerId { seat };
        let name = if seat == 1 { "P1" } else { "P2" };
        let mut game = board(number, actor, quick);
        let target = awaiting(start(&mut game, actor, quick));
        assert!(matches!(target.domain, InputDomain::Targets(_)));
        let material = awaiting(
            game.answer_internal_card(actor, target.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before[name]["pp"]["current"], 2_u8);
        assert_eq!(before["objects"]["enemy"]["damage"], 0_u8);
        let paid = pay_hand(&mut game, &material);
        resolved(paid);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(state[name]["leader"]["life"], 19_u8);
        assert_eq!(state[name]["pp"]["current"], 0_u8);
        assert_eq!(state[name]["hand"], json!(["wrong", "drawn"]));
        assert_eq!(state[name]["cemetery"], json!(["fuel", "source"]));
    }
}
