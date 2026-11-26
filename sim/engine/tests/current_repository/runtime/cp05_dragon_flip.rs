
use super::*;

const ACTOR: PlayerId = PlayerId { seat: 1 };

const EARTH_FORMS: [&str; 4] = ["CP05-032", "CP05-042", "CP05-046", "CP05-092"];

fn finish(game: &mut Game, mut play: Option<CardPlay>, picks: &[&str]) {
    loop {
        while let Some(CardPlay::Awaiting(question)) = play {
            let InputDomain::Targets(targets) = &question.domain else {
                panic!("expected flip materials or a formal target");
            };
            let answer = Answer::Targets(
                targets
                    .members
                    .iter()
                    .filter(|member| {
                        member
                            .card
                            .as_deref()
                            .is_some_and(|card| picks.contains(&card))
                    })
                    .map(|member| member.handle)
                    .collect(),
            );
            play = Some(
                game.answer_internal_card(question.actor, question.token, answer)
                    .unwrap(),
            );
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("expected printed ability");
        };
        play = Some(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(
                    options
                        .iter()
                        .min_by_key(|option| option.line)
                        .unwrap()
                        .handle,
                ),
            )
            .unwrap(),
        );
    }
}

fn play(game: &mut Game, picks: &[&str]) {
    let step = game.play_internal_card("source", ACTOR).unwrap();
    finish(game, Some(step), picks);
}

fn activation_id(printing: &str) -> AbilityId {
    selection_with_rules().current[printing]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

#[test]
fn cp05_054_flips_two_earth_evolutions_before_its_final_release_damage() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP05-SL13"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"gold","card":"CP05-T15"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = earth_forms(2);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    let mut game = earth_game(&initial);
    let activated = activate_material(&mut game, "gold", "CP05-T15");
    finish(
        &mut game,
        Some(activated),
        &["CP05-SL13", "CP05-046", "CP05-092"],
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["earth2"]["face_up"], true);
    assert_eq!(view["objects"]["earth3"]["face_up"], true);
    assert_eq!(view["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(view["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn cp05_060_counts_its_fanfare_flip_for_both_release_damage_instructions() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-P44"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"earth-crystal","card":"CP05-T11"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = earth_forms(3);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut game = earth_game(&initial);
    play(&mut game, &["CP05-092"]);
    let activated = activate_material(&mut game, "earth-crystal", "CP05-T11");
    finish(&mut game, Some(activated), &["CP05-P44", "BP01-171"]);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["drawn"]));
    assert_eq!(view["objects"]["earth3"]["face_up"], true);
    assert_eq!(view["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(view["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn cp05_069_uses_its_own_flip_to_reach_four_earth_evolutions() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-P53"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = earth_forms(3);
    let mut game = earth_game(&initial);
    play(&mut game, &["CP05-092"]);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["earth3"]["face_up"], true);
    assert_eq!(view["objects"]["source"]["power"], 5_u8);
    assert!(
        view["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃"))
    );
}

fn earth_forms(face_up: usize) -> Value {
    json!(EARTH_FORMS
        .into_iter()
        .enumerate()
        .map(|(index, card)| json!({"id":format!("earth{index}"),"card":card,"face_up":index<face_up}))
        .collect::<Vec<_>>())
}

fn earth_game(initial: &Value) -> Game {
    static FIXTURE: OnceLock<SelectedEffects> = OnceLock::new();
    let selection = FIXTURE.get_or_init(|| {
        let (snapshot, keywords, mut input) = inputs().clone();
        for printing in EARTH_FORMS {
            if !selection_with_rules().current.contains_key(printing) {
                // The supporting forms stay in the evolve deck and only their printed traits are read.
                input["documents"].as_array_mut().unwrap().push(json!([
                    format!("{printing}.yaml"),
                    json!({"format":1_u8,"kind":"effect_set","cards":{printing:{"status":"complete","abilities":[]}}})
                        .to_string()
                ]));
            }
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    });
    let mut game = Game::from_selected_effects(
        selection.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "cp05-earth-flip",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: Vec::new(),
        card_play_costs: Vec::new(),
    })
    .unwrap();
    for printing in EARTH_FORMS {
        if !selection.current.contains_key(printing) {
            game.load_internal_card(printing, &blank).unwrap();
        }
    }
    game
}

fn activate_material(game: &mut Game, object: &str, printing: &str) -> CardPlay {
    game.activate_internal_card(object, activation_id(printing), ACTOR)
        .unwrap()
}
