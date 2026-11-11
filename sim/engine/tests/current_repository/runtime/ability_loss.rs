use super::*;

fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (snapshot, keywords, mut input) = inputs().clone();
        input["documents"].as_array_mut().unwrap().push(json!([
            "striken-fragment.yaml",
            json!({"format":1_u8,"kind":"effect_set","cards":{
                "CP03-072":{"status":"complete","abilities":[]}
            }})
            .to_string()
        ]));
        repository::select(&snapshot, &keywords, &input).unwrap()
    })
}

fn striken() -> WellFormedCardProgram {
    let mut tree = expanded::tree().clone();
    let mut card = tree["cards"]["BP08-038"].clone();
    let mut template = card["abilities"][0].clone();
    template.as_object_mut().unwrap().remove("fanfare");
    let mut restriction = template.clone();
    restriction["line"] = json!(1_u8);
    restriction["static"] = json!({"effects":[{"cannot":{"what":"attack"}}]});
    template["line"] = json!(2_u8);
    template["auto"] = json!({"on":{"drive_trigger":{"who":"self"}},"do":[
        {"apply":{"on":"this","effect":{"lose_abilities":{}},"until":"turn"}}
    ]});
    card["abilities"] = json!([restriction, template]);
    tree["cards"] = json!({"CP03-072":card});
    lowerer().lower(&tree).unwrap().remove("CP03-072").unwrap()
}

fn pass(game: &mut Game) {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    while let Some(q) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
    }
}

fn end(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    pass(game);
}

fn position() -> Value {
    let mut initial = drive::position("CP03-025", "CP03-026", 5);
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("cardfight_vanguard");
    initial["players"]["P1"]["zones"]["field"][1] = json!({
        "id":"ally","card":"CP03-072","state":{"acted":false,"entered_this_turn":false}
    });
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"acted":true,"power":0_u8,"hp":20_u8,"max_hp":20_u8}}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"next-draw","card":"BP01-037"}]);
    initial
}

// Q1418/Q1419 are exercised with the real snapshot and source fragments;
// this fixture does not admit the card or verify its complete source closure.
#[test]
fn cp03_072_stand_trigger_preserves_its_independent_leader_lock_and_both_pending_losses() {
    let initial = position();
    // Synthetic icon controls isolate the trigger procedure from image verification.
    let facts = json!({"BP01-037":{"verified":true,"trigger_icon":"stand"},
        "BP01-006":{"verified":true,"trigger_icon":"stand"}});
    let mut game = Game::from_selected_effects(
        fixture().clone(),
        &initial,
        &facts,
        &Value::Null,
        "striken-fragment",
    )
    .unwrap();
    game.load_internal_card("CP03-072", &striken()).unwrap();
    for target in ["P2.leader", "enemy"] {
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":target}))
                .unwrap()
                .outcome,
            "cannot-attack",
            "the printed prohibition covers both kinds of opponent"
        );
    }
    let mut play = drive::attack(&mut game, "CP03-026");
    for _ in 0_u8..2 {
        let CardPlay::Awaiting(q) = play else {
            panic!("Twin Drive trigger")
        };
        let InputDomain::DriveTrigger {
            icon: TriggerIcon::Stand,
            targets,
        } = &q.domain
        else {
            panic!("Stand trigger")
        };
        let ally = targets
            .iter()
            .find(|option| option.card == "CP03-072")
            .unwrap();
        play = game
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::DriveTrigger {
                    accept: true,
                    target: Some(ally.handle),
                },
            )
            .unwrap();
    }
    resolved(play);
    game.resume_internal_card().unwrap();
    for remaining in [2_usize, 1] {
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("saved self loss")
        };
        assert_eq!(
            options.len(),
            remaining,
            "loss does not cancel an already pending occurrence"
        );
        assert!(
            options
                .iter()
                .all(|option| option.card == "CP03-072" && option.line == 2)
        );
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
    }
    assert!(game.internal_pending_choice().unwrap().is_none());
    pass(&mut game);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "Q1419: Stand's installed restriction survives self loss"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "Q1419: the printed restriction is suppressed, so a follower can be attacked"
    );
    pass(&mut game);
    end(&mut game);
    end(&mut game);
    for target in ["P2.leader", "enemy"] {
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":target}))
                .unwrap()
                .outcome,
            "cannot-attack",
            "both overlapping turn losses expire and restore the printed restriction"
        );
    }
}
