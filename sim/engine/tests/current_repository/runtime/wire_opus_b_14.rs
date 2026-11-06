#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::event_cemetery::finish_checks;
use super::union_burst_batch1::finish;
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn settle(game: &mut Game) {
    for _ in 0_u8..6 {
        finish_checks(game);
        let Some(question) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed trigger");
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        resolved(finish(game, play));
    }
    panic!("trigger loop");
}

fn titled(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "wire-opus-b-14",
    )
    .unwrap()
}

/// A Princess Connect title deck, so UB abilities are enabled.
fn position(title: bool) -> Value {
    let mut initial = draw_position();
    if title {
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    }
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037"},{"id":"enemy-2","card":"BP01-037"}]);
    initial
}

/// Original-cost-1 followers (BP01-012) with the given current attack.
fn ones(powers: &[u8]) -> Vec<Value> {
    powers
        .iter()
        .enumerate()
        .map(|(i, power)| json!({"id":format!("one{i}"),"card":"BP01-012","state":{"power":power}}))
        .collect()
}

fn power(view: &Value, id: &str) -> u64 {
    view["objects"][id]["power"].as_u64().unwrap()
}

#[test]
fn cp04_022_all_printings_ub_act_to_gain_one_attack_only_in_a_title_deck() {
    for printing in ["CP04-022", "CP04-SL08"] {
        for title in [true, false] {
            let mut initial = position(title);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"source","card":printing}]);
            let mut game = titled(&initial);
            let sources = game.internal_activated_sources("source").unwrap();
            if !title {
                assert!(sources.is_empty(), "{printing}: UB needs the title deck");
                continue;
            }
            let before = game.projection(View::Referee).unwrap();
            resolved(
                game.activate_internal_card_instance("source", sources[0].instance, actor())
                    .unwrap(),
            );
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(power(&after, "source"), power(&before, "source") + 1);
            assert_eq!(after["objects"]["source"]["acted"], true);
        }
    }
}

#[test]
fn cp04_022_end_phase_counts_only_original_cost_one_attack() {
    // Source 2 + the cost-1 followers; the cost-2 follower's 2 attack must not count.
    for (powers, fires) in [([2_u8, 3, 3], true), ([2_u8, 2, 3], false)] {
        let mut initial = position(true);
        let mut field = vec![json!({"id":"source","card":"CP04-022"})];
        field.extend(ones(&powers));
        field.push(json!({"id":"fighter","card":"BP01-043"}));
        initial["players"]["P1"]["zones"]["field"] = Value::Array(field);
        let mut game = titled(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        let damage = if fires { 2_u8 } else { 0_u8 };
        for enemy in ["enemy", "enemy-2"] {
            assert_eq!(view["objects"][enemy]["damage"], damage, "{powers:?}");
        }
        assert_eq!(view["objects"]["fighter"]["damage"], 0_u8);
    }
}

#[test]
fn cp04_022_fanfare_equips_the_console_only_with_four_own_followers() {
    for (others, equips) in [(&[1_u8, 1, 1][..], true), (&[1_u8, 1][..], false)] {
        let mut initial = position(true);
        initial["players"]["P1"]["zones"]["field"] = Value::Array(ones(others));
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP04-022"}]);
        let mut game = titled(&initial);
        let play = game.play_internal_card("source", actor()).unwrap();
        assert!(matches!(finish(&mut game, play), CardPlay::Entered(_)));
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        let equipment = view["P1"]["equipment"].as_array().unwrap();
        assert_eq!(equipment.len(), usize::from(equips), "{others:?}");
        if equips {
            let id = equipment[0].as_str().unwrap();
            assert_eq!(view["objects"][id]["card"], "CP04-T04");
            assert_eq!(view["objects"][id]["equipped_to"], "source");
        }
    }
}

#[test]
fn cp04_t04_grants_a_ub_payment_that_buffs_only_original_cost_one_followers() {
    for title in [true, false] {
        let mut initial = position(title);
        // A cost-2 host without its own activated ability, so the only source is the grant.
        let mut field = vec![json!({"id":"host","card":"BP01-047"})];
        field.extend(ones(&[1, 1]));
        initial["players"]["P1"]["zones"]["field"] = Value::Array(field);
        initial["players"]["P1"]["zones"]["equipment"] =
            json!([{"id":"console","card":"CP04-T04","state":{"equipped_to":"host"}}]);
        let mut game = titled(&initial);
        let sources = game.internal_activated_sources("host").unwrap();
        if !title {
            assert!(sources.is_empty(), "the granted UB needs the title deck");
            continue;
        }
        assert_eq!(sources.len(), 1);
        let before = game.projection(View::Referee).unwrap();
        resolved(
            game.activate_internal_card_instance("host", sources[0].instance, actor())
                .unwrap(),
        );
        let after = game.projection(View::Referee).unwrap();
        for one in ["one0", "one1"] {
            assert_eq!(power(&after, one), power(&before, one) + 1);
        }
        // The cost-2 host pays and acts but is not an original-cost-1 follower.
        assert_eq!(power(&after, "host"), power(&before, "host"));
        assert_eq!(after["objects"]["host"]["acted"], true);
        assert_eq!(
            after["P1"]["pp"]["current"].as_u64().unwrap() + 1,
            before["P1"]["pp"]["current"].as_u64().unwrap()
        );
    }
}
