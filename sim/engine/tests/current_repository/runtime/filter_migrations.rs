//! Light migrations that only needed the posture, amulet and exclusion filters.
use super::filter_blocks::{
    activate, build, checks, evolve, finish, fixture, offered, pending, position, princess,
};
use super::*;

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

fn has(zone: &Value, id: &str) -> bool {
    zone.as_array().unwrap().contains(&json!(id))
}

#[test]
fn cp04_098_acts_an_amulet_for_hp_and_its_section_stands_amulets() {
    for card in ["CP04-098", "CP04-P74", "PCS01-042"] {
        let mut initial = position(card, "hand");
        princess(&mut initial);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"ally","card":"flt-enemy"},
            {"id":"charm","card":"flt-amulet"}
        ]);
        let mut game = build(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, &[]);
        let trigger = pending(&mut game, card);
        finish(&mut game, trigger, &["flt-enemy", "flt-amulet"]);
        checks(&mut game);
        let after = state(&game);
        assert_eq!(after["objects"]["ally"]["hp"], 11_u8);
        assert_eq!(
            after["objects"]["charm"]["acted"], true,
            "the amulet pays the cost"
        );
        let ability = activate(&mut game, "CP04-098", 0);
        finish(&mut game, ability, &[]);
        checks(&mut game);
        let later = state(&game);
        assert_eq!(later["objects"]["source"]["acted"], true);
        assert_eq!(
            later["objects"]["charm"]["acted"], false,
            "the section stands every own amulet"
        );
        assert_eq!(later["objects"]["ally"]["acted"], false);
    }
}

#[test]
fn bp15_053_exiles_a_different_wizard_and_discounts_it_only_after_a_non_hand_entry() {
    for card in ["BP15-053", "PR-390"] {
        for zone in ["hand", "ex"] {
            let mut initial = position(card, zone);
            initial["players"]["P1"]["zones"]["cemetery"] = json!([
                {"id":"twin","card":"BP15-053"},
                {"id":"wizard","card":"flt-wizard"}
            ]);
            let mut game = build(&initial);
            let entry = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, entry, &[]);
            let trigger = pending(&mut game, card);
            assert_eq!(offered(&trigger), ["flt-wizard"]);
            finish(&mut game, trigger, &[]);
            checks(&mut game);
            let after = state(&game);
            assert_eq!(after["P1"]["ex"], json!(["wizard"]));
            assert_eq!(after["P1"]["cemetery"], json!(["twin"]));
            let before = after["P1"]["pp"]["current"].as_u64().unwrap();
            let replay = game
                .play_internal_card("wizard", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, replay, &[]);
            checks(&mut game);
            let spent = before - state(&game)["P1"]["pp"]["current"].as_u64().unwrap();
            assert_eq!(
                spent,
                if zone == "hand" { 3 } else { 0 },
                "only 手札以外から場に出ていた grants -3 to the exiled wizard"
            );
        }
    }
}

#[test]
fn pcs01_040_spares_the_chosen_target_from_the_sweep() {
    let mut initial = position("flt-base-PCS01-040", "field");
    princess(&mut initial);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"PCS01-040"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"chosen","card":"flt-enemy"},
        {"id":"bystander","card":"flt-other"}
    ]);
    let mut game = build(&initial);
    evolve(&mut game);
    let play = pending(&mut game, "PCS01-040");
    finish(&mut game, play, &["flt-enemy"]);
    checks(&mut game);
    let after = state(&game);
    assert_eq!(after["objects"]["chosen"]["damage"], 4_u8);
    assert_eq!(after["objects"]["bystander"]["damage"], 1_u8);
}

#[test]
fn cp04_065_stands_only_a_differently_named_geo_follower_that_cannot_attack_this_turn() {
    for card in ["CP04-065", "CP04-P49"] {
        let mut initial = position(card, "field");
        princess(&mut initial);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"twin","card":"CP04-065","state":{"acted":true}}),
                json!({"id":"geo","card":"flt-geo","state":{"acted":true}}),
            ]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"flt-enemy","state":{"acted":true}}]);
        let mut game = build(&initial);
        let play = activate(&mut game, "CP04-065", 0);
        assert_eq!(offered(&play), ["flt-geo"], "the same-name copy is excluded");
        finish(&mut game, play, &[]);
        checks(&mut game);
        let after = state(&game);
        assert_eq!(after["objects"]["geo"]["acted"], false);
        assert_eq!(after["objects"]["geo"]["power"], 2_u8);
        assert_eq!(after["objects"]["twin"]["acted"], true);
        assert_eq!(after["objects"]["source"]["acted"], true);
        for target in ["P2.leader", "enemy"] {
            assert_eq!(
                attack(&mut game, target),
                "cannot-attack",
                "このターン、それは相手を攻撃できない"
            );
        }
        end_turn(&mut game);
        end_turn(&mut game);
        assert_eq!(
            attack(&mut game, "P2.leader"),
            "resolved",
            "the prohibition ends with the turn"
        );
    }
}

fn attack(game: &mut Game, target: &str) -> String {
    game.internal_rule_action(&json!({"do":"attack","attacker":"geo","target":target}))
        .unwrap()
        .outcome
}

fn end_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    while let Some(window) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(window.actor, window.token, Answer::Quick(None))
            .unwrap();
    }
}

#[test]
fn cp04_083_buffs_another_diabolos_once_and_exiles_a_different_name() {
    for card in ["CP04-083", "CP04-P63", "PCS01-035"] {
        let mut initial = position(card, "hand");
        princess(&mut initial);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"demon","card":"flt-diabolos"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"twin","card":"CP04-083"},
            {"id":"exiled","card":"flt-diabolos"}
        ]);
        let mut game = build(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, &[]);
        let trigger = pending(&mut game, card);
        assert_eq!(offered(&trigger), ["flt-diabolos"]);
        finish(&mut game, trigger, &[]);
        checks(&mut game);
        assert_eq!(state(&game)["P1"]["ex"], json!(["exiled"]));
        let ability = activate(&mut game, "CP04-083", 0);
        assert_eq!(
            offered(&ability),
            ["flt-diabolos"],
            "the source is not 他の"
        );
        finish(&mut game, ability, &[]);
        checks(&mut game);
        assert_eq!(state(&game)["objects"]["demon"]["power"], 2_u8);
        let definition = fixture().selection.current["CP04-083"]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        assert!(
            game.activate_internal_card("source", definition, PlayerId { seat: 1 })
                .is_err(),
            "1ターンに1回"
        );
    }
}

#[test]
fn bp06_080_second_mode_returns_only_a_differently_named_yokai() {
    for card in ["BP06-080", "PR-134"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"twin","card":"BP06-080"},
            {"id":"yokai","card":"flt-yokai"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"flt-enemy"}]);
        let mut game = build(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, &[]);
        let trigger = pending(&mut game, card);
        finish(&mut game, trigger, &["mode-2", "flt-yokai"]);
        checks(&mut game);
        let after = state(&game);
        assert!(has(&after["P1"]["hand"], "yokai"));
        assert!(has(&after["P1"]["cemetery"], "twin"));
        assert_eq!(after["objects"]["enemy"]["damage"], 0_u8);
    }
}

#[test]
fn bp13_097_places_a_cheap_amulet_and_later_destroys_one() {
    for card in ["BP13-097", "BP13-P22"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"spell","card":"BP01-171"},
            {"id":"charm","card":"flt-amulet"},
            {"id":"filler","card":"BP01-171"}
        ]);
        let mut game = build(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, &[]);
        let trigger = pending(&mut game, card);
        finish(&mut game, trigger, &["flt-amulet"]);
        checks(&mut game);
        let after = state(&game);
        assert!(has(&after["P1"]["field"], "charm"));
        assert_eq!(after["P1"]["deck"].as_array().unwrap().len(), 2);
        let ability = activate(&mut game, "BP13-097", 0);
        finish(&mut game, ability, &[]);
        checks(&mut game);
        let later = state(&game);
        assert!(has(&later["P1"]["cemetery"], "charm"));
        assert_eq!(later["objects"]["source"]["acted"], true);
    }
}
