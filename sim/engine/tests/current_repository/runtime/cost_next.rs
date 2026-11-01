#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the cost scenario."
)]
#![expect(
    clippy::default_numeric_fallback,
    reason = "Synthetic card costs and expected PP use signed integers."
)]
use super::*;

struct Fixture {
    selection: SelectedEffects,
    empty: WellFormedCardProgram,
}
fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (id, cost, traits) in [("cost-academy", 2, vec!["学院"]), ("cost-dragon", 4, vec!["武闘竜人"])] {
            snapshot.push_str(&json!({"number":id,"faces":[{"name":id,"card_type":"フォロワー","card_class":"ニュートラル","cost":cost.to_string(),"power":"1","hp":"1","traits":traits,"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{id}.yaml"), json!({"format":1,"kind":"effect_set","cards":{id:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        Fixture { selection: repository::selection_with_rules_from(&snapshot, &keywords, &input), empty: WellFormedCardProgram::check(CardProgram { abilities: vec![], card_play_costs: vec![] }).unwrap() }
    })
}
fn cost_game(initial: &Value) -> Game {
    let fixture = fixture();
    let mut game = Game::from_selected_effects(
        fixture.selection.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "cost-next",
    )
    .unwrap();
    for id in ["cost-academy", "cost-dragon"] {
        game.load_internal_card(id, &fixture.empty).unwrap();
    }
    game
}
#[expect(clippy::wildcard_enum_match_arm, reason = "The scenario rejects every unhandled question.")]
fn finish(game: &mut Game, mut result: CardPlay) {
    while let CardPlay::Awaiting(question) = result {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected cost input"),
        };
        result = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("cost pending");
        };
        let pending_result = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, pending_result);
    }
}
fn play(game: &mut Game, id: &str) {
    let result = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    finish(game, result);
}

#[test]
fn rulenai_taxes_only_opponents_spells() {
    for source in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP01-065"}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"draw1","card":"BP01-171"},{"id":"draw2","card":"BP01-171"}]);
        if source {
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"tax","card":"BP05-070"}]);
        }
        let mut game = cost_game(&initial);
        play(&mut game, "spell");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            if source { 6 } else { 7 }
        );
    }
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"tax","card":"BP05-070"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP01-065"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"draw1","card":"BP01-171"},{"id":"draw2","card":"BP01-171"}]);
    let mut game = cost_game(&initial);
    play(&mut game, "spell");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        7
    );
}

#[test]
fn manaria_stacks_for_matching_hand_and_ex_followers_but_not_opponent_or_wrong_trait() {
    for zone in ["hand", "ex"] {
        for side in ["P1", "P2"] {
            let mut initial = draw_position();
            initial["players"][side]["zones"]["field"] =
                json!([{"id":"a","card":"BP06-038"},{"id":"b","card":"BP06-038"}]);
            initial["players"]["P1"]["zones"][zone] =
                json!([{"id":"academy","card":"cost-academy"},{"id":"other","card":"BP01-171"}]);
            let mut game = cost_game(&initial);
            play(&mut game, "academy");
            play(&mut game, "other");
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                if side == "P1" { 9 } else { 7 }
            );
        }
    }
}

#[test]
fn drache_discount_survives_nonmatching_play_and_is_consumed_by_exactly_one_matching_play() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP13-054"},{"id":"other","card":"BP01-171"},{"id":"first","card":"cost-dragon"},{"id":"second","card":"cost-dragon"}]);
    let mut game = cost_game(&initial);
    play(&mut game, "source");
    play(&mut game, "other");
    play(&mut game, "first");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        4
    );
    play(&mut game, "second");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0
    );
}

#[test]
fn failed_payment_does_not_consume_the_next_discount_or_move_the_card() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(4);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":"BP13-054"},{"id":"first","card":"cost-dragon"}]);
    let mut game = cost_game(&initial);
    play(&mut game, "source");
    let before = format!("{game:?}");
    for _ in 0..2 {
        assert!(matches!(
            game.play_internal_card("first", PlayerId { seat: 1 }),
            Err(EngineFailure::IllegalPlay(_))
        ));
        assert_eq!(format!("{game:?}"), before);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["first"])
        );
    }
}

#[test]
fn leaving_field_removes_the_tax_before_the_next_spell_payment() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"tax","card":"BP05-070"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP01-065"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"draw1","card":"BP01-171"},{"id":"draw2","card":"BP01-171"}]);
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: subjects(),
        },
    );
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    play(&mut game, "spell");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["tax"]));
    assert_eq!(state["P1"]["pp"]["current"], 7);
}

fn end_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    if let Some(question) = game.internal_quick_choice().unwrap() {
        let result = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(game, result);
    }
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
}

#[test]
fn unused_discount_expires_at_the_end_of_its_installation_turn() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":"BP13-054"},{"id":"first","card":"cost-dragon"}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0..3)
                .map(|index| json!({"id":format!("{seat}-draw{index}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = cost_game(&initial);
    play(&mut game, "source");
    end_turn(&mut game);
    end_turn(&mut game);
    play(&mut game, "first");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        6
    );
}

#[test]
fn rulenai_tax_is_included_in_quick_availability_and_actual_payment() {
    for available in [3, 4] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"tax","card":"BP05-070"}]);
        initial["players"]["P2"]["pp"]["current"] = json!(available);
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"spell","card":"BP01-065"}]);
        initial["players"]["P2"]["zones"]["deck"] =
            json!([{"id":"draw1","card":"BP01-171"},{"id":"draw2","card":"BP01-171"}]);
        let mut game = cost_game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("Quick");
        };
        if available == 3 {
            assert_eq!(options.as_slice(), []);
        } else {
            assert_eq!(options.len(), 1);
            let result = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Quick(Some(options[0].handle)),
                )
                .unwrap();
            finish(&mut game, result);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["pp"]["current"],
                0
            );
        }
    }
}
