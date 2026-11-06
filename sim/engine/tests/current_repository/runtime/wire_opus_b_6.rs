#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::damage_prevent::all_leaders_damage;
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..4)
            .map(|i| json!({"id":format!("P1-deck{i}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["deck"] = json!(
        (0_u8..4)
            .map(|i| json!({"id":format!("P2-deck{i}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn triggers(game: &mut Game) {
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        assert!(
            !matches!(step, CardPlay::Awaiting(_)),
            "no printed decision"
        );
        finish_checks(game);
    }
}

fn play(game: &mut Game, id: &str) {
    let mut step = game.play_internal_card(id, actor()).unwrap();
    while let CardPlay::Awaiting(q) = step {
        assert!(
            matches!(q.domain, InputDomain::WardEntry { .. }),
            "only the ward entry posture is asked"
        );
        step = game
            .answer_internal_card(q.actor, q.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(step, CardPlay::Entered(_) | CardPlay::Resolved(_)),
        "{id} is played"
    );
    finish_checks(game);
    triggers(game);
}

/// The producer deals 2 to both leaders through an ability.
fn hit_leaders(game: &mut Game) {
    game.activate_internal_card("producer", AbilityId(1), actor())
        .unwrap();
    finish_checks(game);
}

fn life(game: &Game, seat: &str) -> u64 {
    game.projection(View::Referee).unwrap()[seat]["leader"]["life"]
        .as_u64()
        .unwrap()
}

fn end_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    triggers(game);
    let q = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    finish_checks(game);
    triggers(game);
}

#[test]
fn bp14_t07_all_printings_reduce_every_hit_on_the_own_leader_this_turn_only() {
    for printing in ["BP14-T07", "BP14-P33"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"mercury","card":printing}]);
        let mut game = with_producer(&initial, all_leaders_damage());
        play(&mut game, "mercury");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), 1, "{printing} draws");
        // Not a "next damage" shield: both hits this turn are reduced by one.
        hit_leaders(&mut game);
        hit_leaders(&mut game);
        assert_eq!(life(&game, "P1"), 18, "{printing}");
        assert_eq!(life(&game, "P2"), 16, "{printing}");
        end_turn(&mut game);
        end_turn(&mut game);
        hit_leaders(&mut game);
        assert_eq!(life(&game, "P1"), 16, "{printing} expires at turn end");
    }
}

#[test]
fn bp14_090_all_printings_shield_themselves_and_the_leader_and_add_mercury() {
    for printing in ["BP14-090", "BP14-SL20", "BP14-U06"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        let mut game = with_producer(&initial, all_leaders_damage());
        play(&mut game, "source");
        let view = game.projection(View::Referee).unwrap();
        let placed: Vec<_> = view["P1"]["ex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| view["objects"][id.as_str().unwrap()]["name"].clone())
            .collect();
        assert_eq!(placed, [json!("水銀の断絶")], "{printing}");
        hit_leaders(&mut game);
        assert_eq!(life(&game, "P1"), 19, "{printing}");
        assert_eq!(life(&game, "P2"), 18, "{printing}");
    }
}

fn own_followers() -> Selector {
    let mut selector = subjects();
    if let Selector::Zone { players, .. } = &mut selector {
        players.relation = PlayerRelation::SelfPlayer;
        players.cardinality = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
    }
    Selector::Difference {
        input: Box::new(selector),
        except: Box::new(Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        }),
    }
}

#[test]
fn bp14_090_takes_one_less_damage_and_ignores_ability_destruction() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP14-090"}]);
    let mut hurt = with_producer(
        &initial,
        Action::Damage {
            recipients: RecipientSet::Objects {
                objects: own_followers(),
            },
            amount: DamageAmount::Uniform {
                amount: Expr::Literal(Int(2)),
            },
            flags: DamageFlags::Ability,
            source: ObjectView::SourceInformation {},
            batch: BatchPolicy::Instruction {},
        },
    );
    hit_leaders(&mut hurt);
    assert_eq!(
        hurt.projection(View::Referee).unwrap()["objects"]["source"]["damage"],
        1_u8
    );
    let mut destroyed = with_producer(
        &initial,
        Action::Destroy {
            objects: own_followers(),
        },
    );
    hit_leaders(&mut destroyed);
    assert!(
        destroyed.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "ability destruction does not remove it"
    );
}

#[test]
fn shion_and_mercury_reductions_stack_on_every_hit_this_turn() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP14-090"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"mercury","card":"BP14-T07"}]);
    let mut game = with_producer(&initial, all_leaders_damage());
    play(&mut game, "mercury");
    // Each hit of 2 loses one to Shion and one to Mercury.
    hit_leaders(&mut game);
    hit_leaders(&mut game);
    assert_eq!(life(&game, "P1"), 20);
    assert_eq!(life(&game, "P2"), 16);
}
