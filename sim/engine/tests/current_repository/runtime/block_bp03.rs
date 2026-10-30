use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::InstructionOutcome;

fn initial(card: &str, material: &str, count: usize) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    value["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..count)
            .map(|i| json!({"id":format!("grave-{i}"),"card":material}))
            .collect::<Vec<_>>()
    );
    value["players"]["P2"]["zones"]["field"] = json!([
        {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"},
        {"id":"amulet","card":"BP09-102"}
    ]);
    value["players"]["P2"]["zones"]["ex"] = json!([{"id":"outside","card":"BP01-T03"}]);
    value
}

fn game(value: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        value,
        &Value::Null,
        &Value::Null,
        "block-bp03",
    )
    .unwrap()
}

#[expect(
    clippy::panic,
    reason = "Only the real evolution damage declares a formal target."
)]
fn trigger(game: &mut Game) -> Activation {
    let occurrence = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &occurrence.domain else {
        panic!("printed pending ability")
    };
    assert_eq!(options.len(), 1, "exactly one printed ability is pending");
    let play = game
        .answer_internal_card(
            occurrence.actor,
            occurrence.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    if let CardPlay::Awaiting(target) = play {
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("formal opposing follower target")
        };
        assert_eq!(
            domain.members.len(),
            2,
            "only the two opposing field followers are formal candidates"
        );
        resolved(
            game.answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        )
    } else {
        resolved(play)
    }
}

fn assert_joint_damage(game: &Game, result: &Activation) {
    assert_eq!(
        result
            .progress
            .receipts
            .values()
            .filter(|receipt| matches!(receipt, InstructionOutcome::Damage(_)))
            .count(),
        1,
        "the leader and followers share one damage application"
    );
    let damage = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(event) = fact {
                Some(event)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        damage.len(),
        3,
        "one enemy leader and two enemy followers take damage"
    );
    assert!(
        damage.iter().all(|event| event.amount.0.get() == 2),
        "every member of the same batch takes two damage"
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P2"]["leader"]["life"], 18_u8,
        "only the opposing leader takes damage"
    );
    for id in ["first", "second"] {
        assert_eq!(
            view["objects"][id]["damage"], 2_u8,
            "both enemy followers take the joint damage"
        );
    }
    for id in ["ally", "amulet", "outside"] {
        assert_eq!(
            view["objects"][id]["damage"], 0_u8,
            "allies, amulets and EX objects are excluded"
        );
    }
    assert_eq!(
        view["P1"]["leader"]["life"], 20_u8,
        "the friendly leader is excluded"
    );
}

#[test]
fn bp03_029_printings_evolve_from_the_real_base_and_count_only_allied_cemetery_heroes() {
    for printing in ["BP03-029", "BP03-P09"] {
        for count in [1_usize, 2] {
            let mut value = initial(printing, "BP03-028", count);
            value["players"]["P1"]["zones"]["hand"] = json!([]);
            value["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP03-028"}]);
            value["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":printing}]);
            value["players"]["P1"]["zones"]["cemetery"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"wrong","card":"BP01-037"}));
            value["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex-hero","card":"BP03-028"}]);
            value["players"]["P2"]["zones"]["cemetery"] =
                json!([{"id":"foreign-hero","card":"BP03-028"}]);
            let mut game = game(&value);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            finish_checks(&mut game);
            trigger(&mut game);
            let after = game.projection(View::Referee).unwrap();
            let total: u64 = ["first", "second"]
                .iter()
                .map(|id| after["objects"][*id]["damage"].as_u64().unwrap())
                .sum();
            assert_eq!(
                total,
                if count == 2 { 4 } else { 2 },
                "wrong-trait, EX and opposing heroes do not cross the cemetery threshold"
            );
            assert_eq!(
                after["P1"]["pp"]["current"], 9_u8,
                "native evolution pays the base's actual one PP"
            );
        }
    }
}

#[test]
fn bp03_076_printings_fanfare_jointly_hit_enemies_and_grant_storm_only_at_nc_twenty() {
    for printing in ["BP03-076", "BP03-P21", "EBD03-010", "PR-620"] {
        for count in [19_usize, 20] {
            let mut game = game(&initial(printing, "BP01-037", count));
            assert!(matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            finish_checks(&mut game);
            let result = trigger(&mut game);
            assert_joint_damage(&game, &result);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(
                after["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走")),
                count == 20,
                "the printed Fanfare grants Storm only at the frozen NC twenty threshold"
            );
        }
    }
}

#[test]
fn bp03_076_printings_native_attack_uses_the_same_joint_enemy_damage_batch() {
    for printing in ["BP03-076", "BP03-P21", "EBD03-010", "PR-620"] {
        let mut value = initial(printing, "BP01-037", 0);
        value["players"]["P1"]["zones"]["hand"] = json!([]);
        value["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"source","card":printing}));
        let mut game = game(&value);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let result = trigger(&mut game);
        assert_joint_damage(&game, &result);
    }
}
