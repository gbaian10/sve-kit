use super::*;
use sve_engine::game::internal::DecisionView;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::{InstalledChange, ObservedZone};
use sve_engine::game::player::{
    AdmissionGap, AdmissionGapReason, DeckConstruction, PlayerDeck, StrictStartError,
};
use sve_engine::catalog::{ProgramPrinting, SnapshotPrograms};

// Synthetic icon controls isolate rule ordering from source-image verification; the holders are real current cards.

pub(super) fn position(base: &str, form: &str, stats: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P2"]["leader"]["life"] = json!(20_u8);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":base,"state":{"evolved":true,"evolved_with":"form","power":stats,"hp":stats,"max_hp":stats}},
        {"id":"ally","card":"BP01-037","state":{"acted":true}}
    ]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-006"},
        {"id":"third","card":"BP01-009"},{"id":"tail","card":"BP01-010"}
    ]);
    initial
}

pub(super) fn drive_game(initial: &Value, facts: &Value) -> Game {
    Game::from_selected_effects(
        selection().clone(),
        initial,
        facts,
        &Value::Null,
        "drive",
    )
    .unwrap()
}

pub(super) fn attack(game: &mut Game, number: &str) -> CardPlay {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the real attack creates its printed Drive occurrence"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed attack trigger")
    };
    let option = options.iter().find(|option| option.card == number).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

fn question(play: CardPlay, icon: TriggerIcon) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("Drive trigger decision")
    };
    assert!(
        matches!(&question.domain,InputDomain::DriveTrigger {icon: found,..} if *found==icon),
        "the verified revealed icon determines this fresh decision"
    );
    question
}

fn answer(game: &mut Game, q: &DecisionView, accept: bool, target: Option<u64>) -> CardPlay {
    game.answer_internal_card(q.actor, q.token, Answer::DriveTrigger { accept, target })
        .unwrap()
}

fn finish_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(
            game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ),
        "passing the real end-phase Quick advances the turn"
    );
}

fn finish_stand_turns(game: &mut Game) {
    game.resume_internal_card().unwrap();
    while let Some(q) = game.internal_quick_choice().unwrap() {
        let InputDomain::Quick { .. } = q.domain else {
            panic!("post-Drive Quick")
        };
        game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
    }
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "Stand prevents this follower from attacking the opposing leader this turn"
    );
    finish_turn(game);
    finish_turn(game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
        6_u8,
        "Critical has no end-of-turn expiry"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "Stand's leader attack lock expires at the real turn end"
    );
}

fn title(initial: &mut Value) {
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("cardfight_vanguard");
}

fn checks(result: &Activation) -> Vec<DriveCheckedEvent> {
    result
        .progress
        .facts
        .iter()
        .filter_map(|fact| match fact {
            Fact::DriveChecked(event) => Some(**event),
            _ => None,
        })
        .collect()
}

#[test]
fn cp03_026_and_p16_twin_drive_finish_each_check_before_selecting_the_next_top() {
    for number in ["CP03-026", "CP03-P16"] {
        let mut initial = position("CP03-025", number, 5);
        title(&mut initial);
        let facts = json!({"BP01-037":{"verified":true,"trigger_icon":"draw"},"BP01-009":{"verified":true,"trigger_icon":"heal"}});
        let mut game = drive_game(&initial, &facts);
        let q = question(attack(&mut game, number), TriggerIcon::Draw);
        let revealed = game.projection(View::Referee).unwrap();
        assert_eq!(revealed["P1"]["trigger"], json!(["first"]));
        let q = question(answer(&mut game, &q, true, None), TriggerIcon::Heal);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["hand"],
            json!(["second"]),
            "first Draw consumes the next top before the second check"
        );
        assert_eq!(view["P1"]["cemetery"], json!(["first"]));
        assert_eq!(view["P1"]["trigger"], json!(["third"]));
        let result = resolved(answer(&mut game, &q, true, None));
        let events = checks(&result);
        assert_eq!(events.len(), 2_usize);
        assert_ne!(events[0].check, events[1].check);
        assert!(
            events.iter().all(|event| event.trigger_accepted.0
                && event.final_destination.kind == ZoneKind::Cemetery)
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            13_u8
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["deck"],
            json!(["tail"])
        );
    }
}

#[test]
fn cp03_047_and_p30_critical_only_increases_attack_and_stand_locks_leader_attacks() {
    for number in ["CP03-047", "CP03-P30"] {
        for acted in [false, true] {
            let mut initial = position("CP03-046", number, 4);
            initial["players"]["P1"]["zones"]["field"][1]["state"]["acted"] = json!(acted);
            initial["players"]["P2"]["zones"]["deck"] = json!([
                {"id":"opponent-top","card":"BP01-037"},
                {"id":"opponent-tail","card":"BP01-037"}
            ]);
            title(&mut initial);
            let facts = json!({"BP01-037":{"verified":true,"trigger_icon":"critical"},"BP01-006":{"verified":true,"trigger_icon":"stand"}});
            let mut game = drive_game(&initial, &facts);
            let q = question(attack(&mut game, number), TriggerIcon::Critical);
            let before = game.projection(View::Referee).unwrap();
            game.answer_internal_card(
                PlayerId { seat: 2 },
                q.token,
                Answer::DriveTrigger {
                    accept: true,
                    target: Some(0),
                },
            )
            .unwrap_err();
            game.answer_internal_card(
                q.actor,
                q.token,
                Answer::DriveTrigger {
                    accept: true,
                    target: Some(99),
                },
            )
            .unwrap_err();
            assert_eq!(
                game.projection(View::Referee).unwrap(),
                before,
                "foreign actors and handles cannot mutate the revealed check"
            );
            let q = question(answer(&mut game, &q, true, Some(0)), TriggerIcon::Stand);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["objects"]["source"]["power"], 6_u8);
            assert_eq!(
                view["objects"]["source"]["hp"], 4_u8,
                "Critical is attack +2, never life +2"
            );
            let result = resolved(answer(&mut game, &q, true, Some(1)));
            assert_eq!(checks(&result).len(), 2_usize);
            let observation = game.observe(Viewer::P1).unwrap();
            let ally = &observation
                .players
                .iter()
                .find(|player| player.player == Viewer::P1)
                .unwrap()
                .zones
                .iter()
                .find(|zone| zone.kind == ObservedZone::Field)
                .unwrap()
                .cards[1];
            assert!(ally.card.properties.state.as_ref().unwrap().effects.iter().any(|effect| effect.active && effect.change == InstalledChange::NoLeaderAttack));
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["ally"]["acted"],
                false
            );
            finish_stand_turns(&mut game);
        }
    }
}

#[test]
fn cp03_067_and_p45_normal_deck_does_not_read_icon_facts_and_rotates_each_reveal() {
    for number in ["CP03-067", "CP03-P45"] {
        let mut game = drive_game(&position("CP03-066", number, 6), &Value::Null);
        let result = resolved(attack(&mut game, number));
        let events = checks(&result);
        assert_eq!(events.len(), 2_usize);
        assert!(events.iter().all(
            |event| !event.trigger_accepted.0 && event.final_destination.kind == ZoneKind::Deck
        ));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["trigger"], json!([]));
        assert_eq!(
            view["P1"]["deck"],
            json!(["third", "tail", "first", "second"])
        );
        assert_eq!(view["P1"]["hand"], json!([]));
    }
}

#[test]
fn drive_decline_no_icon_empty_deck_and_unverified_icons_never_guess_a_trigger() {
    let mut initial = position("CP03-025", "CP03-026", 5);
    title(&mut initial);
    let mut game = drive_game(
        &initial,
        &json!({"BP01-037":{"verified":true,"trigger_icon":"draw"},"BP01-006":{"verified":true,"trigger_icon":"none"}}),
    );
    let q = question(attack(&mut game, "CP03-026"), TriggerIcon::Draw);
    let result = resolved(answer(&mut game, &q, false, None));
    assert_eq!(checks(&result).len(), 2_usize);
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::DriveTrigger(_)))
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(["third", "tail", "first", "second"])
    );
    initial["players"]["P1"]["zones"]["deck"] = json!([]);
    let mut empty = drive_game(&initial, &Value::Null);
    assert_eq!(checks(&resolved(attack(&mut empty, "CP03-026"))), vec![]);
    assert_ne!(
        empty.projection(View::Referee).unwrap()["game"]["ended"],
        true,
        "an empty Drive check is not a failed draw"
    );
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"first","card":"BP01-037"}]);
    let mut unknown = drive_game(&initial, &json!({"BP01-037":{"trigger_icon":"draw"}}));
    unknown
        .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
        .unwrap();
    let pending = unknown.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("Drive pending")
    };
    let before = unknown.projection(View::Referee).unwrap();
    unknown
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap_err();
    assert_eq!(
        unknown.projection(View::Referee).unwrap(),
        before,
        "unverified icon data rejects atomically"
    );
}

fn vanguard_programs(omit: &str) -> SnapshotPrograms {
    let (snapshot, keywords, _) = inputs();
    let records: Value = serde_saphyr::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../authored/catalog/trigger-icons/jp/001.yaml"
    ))).unwrap();
    let numbers = ["CP03-016", "CP03-P10", "CP03-025", "CP03-026"];
    let mut attachments = json!({
        "format_version":"4.2.0",
        "entries":[],
        "printings":{},
        "trigger_icons":{}
    });
    let mut printings = Vec::<ProgramPrinting>::new();
    let mut native = Vec::new();
    for number in numbers {
        let candidate = &selection().current[number];
        let observation = records["records"].as_array().unwrap().iter()
            .find(|row| row["card_no"] == number).unwrap();
        let id = observation["printing_id"].as_str().unwrap();
        let face = observation["face_id"].as_str().unwrap();
        let card_id = &candidate.card_id;
        if number != omit {
            attachments["trigger_icons"][id] = json!({(face): observation["icon"]});
        }
        let line = snapshot.lines().find(|line| {
            serde_json::from_str::<Card>(line).unwrap().number == number
        }).unwrap();
        let printed: Value = serde_json::from_str(line).unwrap();
        native.push(line);
        printings.push(serde_json::from_value(json!({"printing_id": id, "card_id":card_id,
            "card_no":number,"faces":[{"face_id":face,"revision_id":face,
            "cost":printed["faces"][0]["cost"].as_str().unwrap().parse::<u64>().ok(),
            "rules_name":printed["faces"][0]["name"]}]})).unwrap());
        if attachments["printings"].get(card_id).is_some() {
            continue;
        }
        let source = &candidate.expanded["cards"][&candidate.card_no];
        let mut ast = json!({"card_id":card_id,"scope":"shared"});
        for key in ["abilities", "faces", "deck_rule", "token"] {
            if let Some(value) = source.get(key) {
                ast[key] = value.clone();
            }
        }
        for ability in ast.get_mut("abilities").and_then(Value::as_array_mut).into_iter().flatten() {
            ability.as_object_mut().unwrap().remove("sources");
        }
        for face_program in ast.get_mut("faces").and_then(Value::as_object_mut).into_iter().flat_map(|faces| faces.values_mut()) {
            for ability in face_program["abilities"].as_array_mut().into_iter().flatten() {
                ability.as_object_mut().unwrap().remove("sources");
            }
        }
        attachments["entries"].as_array_mut().unwrap()
            .push(json!({"id":card_id,"dsl_version":"1.0","ast":ast}));
        attachments["printings"][card_id] = json!({"representative":id,"tokens":[]});
    }
    let native = native.join("\n");
    let titles = json!([rule_fixtures::title("cardfight_vanguard", numbers[0], json!([]))]);
    let source = rule_fixtures::rules(&native, titles, json!([]));
    let (rules, identity) = rule_fixtures::resolved(&native, &source.to_string()).unwrap();
    SnapshotPrograms::new(&native, keywords, &attachments.to_string(), &printings, &rules, &identity)
        .unwrap()
}

#[test]
fn published_vanguard_printing_icons_reach_title_opening_and_twin_drive() {
    let deck = |first| PlayerDeck {
        construction: DeckConstruction::Title("cardfight_vanguard".into()),
        main: [first, "CP03-025", "CP03-025", "CP03-016"].map(str::to_owned).to_vec(),
        evolve: vec!["CP03-026".into()],
    };
    let decks = [deck("CP03-P10"), deck("CP03-016")];
    let missing = vanguard_programs("CP03-P10");
    let Err(StrictStartError::Admission(report)) = Game::start_player_game(
        missing.select_all().unwrap(), decks.clone(), Viewer::P1, "printing-icons",
    ) else {
        panic!("an unreviewed printing cannot borrow its counterpart's evidence");
    };
    assert_eq!(report.gaps(Viewer::P1), [AdmissionGap {
        card_no: "CP03-P10".into(), reason: AdmissionGapReason::MissingTriggerIcons,
    }]);
    assert_eq!(report.gaps(Viewer::P2), []);
    let available = vanguard_programs("").select_all().unwrap();
    Game::start_player_game(available.clone(), decks, Viewer::P1, "printing-icons").unwrap();

    let mut initial = position("CP03-025", "CP03-026", 5);
    title(&mut initial);
    initial["players"]["P1"]["zones"]["field"].as_array_mut().unwrap().pop();
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"CP03-P10"},{"id":"drawn","card":"CP03-016"},
        {"id":"second","card":"CP03-025"}
    ]);
    let mut game = Game::from_selected_effects(
        available, &initial, &Value::Null, &Value::Null, "printing-icons",
    ).unwrap();
    let decision = question(attack(&mut game, "CP03-026"), TriggerIcon::Draw);
    let result = resolved(answer(&mut game, &decision, true, None));
    assert_eq!(checks(&result).len(), 2);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["drawn"]));
    assert_eq!(view["P1"]["cemetery"], json!(["first"]));
    assert_eq!(view["P1"]["deck"], json!(["second"]));
}
