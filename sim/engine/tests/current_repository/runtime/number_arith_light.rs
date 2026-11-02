#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn cards(prefix: &str, card: &str, count: usize) -> Vec<Value> {
    (0..count)
        .map(|n| json!({"id":format!("{prefix}{n}"),"card":card}))
        .collect()
}

/// Answers target questions with the first member; returns the damage dealt by resolved bodies.
fn finish(game: &mut Game, mut play: CardPlay) -> Vec<u64> {
    loop {
        match play {
            CardPlay::Awaiting(question) => {
                let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
                    Answer::WardEntry(false)
                } else if let InputDomain::Targets(domain) = &question.domain {
                    Answer::Targets(vec![domain.members[0].handle])
                } else {
                    panic!("unexpected decision")
                };
                play = game
                    .answer_internal_card(question.actor, question.token, answer)
                    .unwrap();
            }
            CardPlay::Resolved(activation) => {
                let mut hits = damage_hits(&activation);
                hits.extend(triggers(game));
                return hits;
            }
            CardPlay::Entered(_) | CardPlay::Cancelled | CardPlay::Continued(_) => {
                return triggers(game);
            }
        }
    }
}

/// Plays every pending printed ability in turn.
fn triggers(game: &mut Game) -> Vec<u64> {
    let mut hits = vec![];
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            return hits;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        hits.extend(finish(game, step));
    }
}

fn damage_hits(activation: &Activation) -> Vec<u64> {
    activation
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(hit) = fact {
                Some(hit.amount.0.get())
            } else {
                None
            }
        })
        .collect()
}

/// Collaboration titles (ウマ娘, ヴァンガード) need the repository's rule bindings.
fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "number-arith",
    )
    .unwrap()
}

fn enemy_position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-080"}]);
    initial
}

#[test]
fn cp01_014_fanfare_deals_twice_the_followers_on_its_side_including_itself() {
    for (allies, damage) in [(0_usize, 2_u8), (2, 6)] {
        let mut initial = enemy_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP01-014"}]);
        let mut field = cards("ally", "BP01-171", allies);
        // An amulet is not a follower and does not raise the count.
        field.push(json!({"id":"amulet","card":"BP01-141"}));
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        let mut game = game(&initial);
        let step = game.play_internal_card("source", actor()).unwrap();
        finish(&mut game, step);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"], damage,
            "{allies} allies plus the source, doubled"
        );
    }
}

#[test]
fn cp03_087_adds_a_separate_hit_per_five_shadow_paladins_rounding_down() {
    for (paladins, hits) in [
        (4_usize, vec![3_u64]),
        (5, vec![3, 1]),
        (9, vec![3, 1]),
        (10, vec![3, 2]),
    ] {
        let mut initial = enemy_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP03-086"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"CP03-087"}]);
        let mut grave = cards("paladin", "CP03-100", paladins);
        // Other traits in the cemetery are not counted.
        grave.extend(cards("other", "BP01-171", 3));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        assert_eq!(
            triggers(&mut game),
            hits,
            "Q1428: {paladins} Shadow Paladins add their whole fives as a second hit"
        );
        let total: u64 = hits.iter().sum();
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            total
        );
    }
}

#[test]
fn bp19_040_costs_one_less_per_complete_wizard_follower_and_spell_pair() {
    for (followers, spells, pp, cost) in [(0_usize, 3_usize, 10_u8, 10_u8), (2, 3, 8, 8), (3, 3, 7, 7)]
    {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"]["current"] = json!(pp);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP19-040"}]);
        let mut grave = cards("follower", "BP22-044", followers);
        grave.extend(cards("spell", "BP04-044", spells));
        // Non-wizard followers complete no pair.
        grave.extend(cards("goblin", "BP01-171", 2));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        let mut game = game(&initial);
        let step = game.play_internal_card("source", actor()).unwrap();
        finish(&mut game, step);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            pp - cost,
            "min({followers}, {spells}) pairs reduce the printed 10"
        );
        assert_eq!(state["P1"]["field"], json!(["source"]));
    }
    // One PP short of the reduced cost cannot play it.
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(8_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP19-040"}]);
    let mut grave = cards("follower", "BP22-044", 1);
    grave.extend(cards("spell", "BP04-044", 3));
    initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
    let mut game = game(&initial);
    game.play_internal_card("source", actor()).unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["source"])
    );
}

#[test]
fn cp02_052_raises_max_pp_only_when_one_hit_deals_at_least_five() {
    for (attacker, max) in [("BP02-031", 7_u8), ("BP02-022", 8)] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"akari","card":"CP02-052","state":{"acted":true}}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":attacker}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"enemy","target":"akari"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        triggers(&mut game);
        let quick = game.internal_quick_choice().unwrap().unwrap();
        game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
            .unwrap();
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["max"], max,
            "{attacker}'s combat hit is compared with 5"
        );
    }
}
