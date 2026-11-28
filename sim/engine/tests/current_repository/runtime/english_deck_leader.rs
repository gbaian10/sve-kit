use super::ability_combinations::{evolve_with_modes, finish, view};
use super::*;

#[test]
fn omnis_searches_for_a_low_cost_beast_only_under_a_forestcraft_leader() {
    for (leader_class, should_search) in [(Some("エルフ"), true), (Some("ドラゴン"), false), (None, false)] {
        let mut initial = draw_position();
        if let Some(class) = leader_class {
            initial["players"]["P1"]["leader"]["class"] = json!(class);
        } else {
            initial["players"]["P1"]["leader"] = json!({"life":20_u8});
            initial["players"]["P1"]["construction"] = json!("title");
            initial["players"]["P1"]["title_code"] = json!("idolmaster_cinderella_girls");
        }
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"omnis","card":"GFD01-004EN"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"too-expensive","card":"BP02-003"},
            {"id":"beast","card":"BP01-013"},
            {"id":"other-trait","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        let played = game.play_internal_card("omnis", actor()).unwrap();
        finish(&mut game, Some(played), &[]);
        let state = view(&game);
        assert_eq!(state["P1"]["field"], if should_search { json!(["omnis","beast"]) } else { json!(["omnis"]) }, "class or title construction {leader_class:?}");
        assert!(state["P1"]["deck"].as_array().unwrap().contains(&json!("too-expensive")), "the higher-cost Beast stays in the deck");
    }
}

#[test]
fn unicorn_requires_an_amulet_and_only_a_havencraft_leader_evolves_it() {
    for (leader_class, has_amulet, should_evolve) in [
        ("ビショップ", true, true), ("ドラゴン", true, false), ("ビショップ", false, false),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!(leader_class);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"unicorn","card":"GFD01-023EN"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"GFD01-024EN"}]);
        if has_amulet {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-091"}]);
        }
        let mut game = game(&initial);
        let played = game.play_internal_card("unicorn", actor()).unwrap();
        finish(&mut game, Some(played), &[]);
        let state = view(&game);
        assert_eq!(state["P1"]["hand"], if has_amulet { json!(["drawn"]) } else { json!([]) }, "Q1358 requires an amulet before the draw");
        assert_eq!(state["objects"]["unicorn"]["evolved_with"] == "form", should_evolve, "class {leader_class} with amulet {has_amulet}");
    }
}

#[test]
fn warbreaker_ramps_for_its_leader_and_doubles_only_after_a_legal_end_phase_target() {
    for (leader_class, starting_pp, has_target, max_pp, doubles) in [
        ("ドラゴン", 8_u8, true, 10_u8, true),
        ("ビショップ", 8_u8, true, 9_u8, false),
        ("ドラゴン", 10_u8, false, 10_u8, false),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!(leader_class);
        initial["players"]["P1"]["pp"] = json!({"max":starting_pp,"current":starting_pp});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFD02-001EN"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"GFD02-002EN"}]);
        initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        if has_target {
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"target","card":"BP01-037"}]);
        }
        let mut game = game(&initial);
        evolve_with_modes(&mut game, 1, &[]);
        let evolved = view(&game);
        assert_eq!(evolved["P1"]["pp"]["max"], max_pp, "the matching class increases maximum PP by three, capped at ten");
        game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
        finish(&mut game, None, &[]);
        let ended = view(&game);
        assert_eq!(ended["P2"]["field"], json!([]), "the legal target was destroyed");
        for stat in ["power", "hp"] {
            let original = evolved["objects"]["source"][stat].as_u64().unwrap();
            assert_eq!(ended["objects"]["source"][stat], original * if doubles { 2 } else { 1 }, "the end phase adjusts {stat} only when eligible");
        }
    }
}

#[test]
fn regal_wildcat_still_selects_lance_lizard_when_it_gives_storm_to_each_ally() {
    for (leader_class, group_bonus) in [("ロイヤル", true), ("ドラゴン", false)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!(leader_class);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"wildcat","card":"GFD02-019EN"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"lance","card":"BP03-071"},
            {"id":"other","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        let before = view(&game);
        let played = game.play_internal_card("wildcat", actor()).unwrap();
        finish(&mut game, Some(played), &[]);
        let after = view(&game);
        assert_eq!(after["P2"]["leader"]["life"], 19_u8, "selecting Lance Lizard triggers its own ability per Q1380");
        assert_eq!(after["objects"]["lance"]["power"], before["objects"]["lance"]["power"].as_u64().unwrap()+3, "Lance Lizard gains two attack from Wildcat and one from its own ability");
        assert_eq!(after["objects"]["other"]["power"], before["objects"]["other"]["power"].as_u64().unwrap()+if group_bonus {2} else {0}, "only the Swordcraft leader extends the buff to the second follower");
        let keywords = after["objects"]["other"]["keywords"].as_array().unwrap();
        assert_eq!(keywords.contains(&json!("疾走")), group_bonus, "Storm reaches the same additional recipients as the buff");
    }
}

#[test]
fn pain_immortal_discards_on_the_abysscraft_fanfare_and_strikes_either_target_kind() {
    for (leader_class, attack_target, size_after_fanfare) in [
        ("ナイトメア", "P2.leader", 2_usize), ("ドラゴン", "target", 3_usize),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!(leader_class);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"pain","card":"GFD02-023EN"}]);
        initial["players"]["P2"]["zones"]["hand"] = json!([
            {"id":"first","card":"BP01-037"}, {"id":"second","card":"BP01-037"}, {"id":"third","card":"BP01-037"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"target","card":"BP01-037","state":{"acted":true}}]);
        let mut game = game(&initial);
        let played = game.play_internal_card("pain", actor()).unwrap();
        finish(&mut game, Some(played), &[]);
        assert_eq!(view(&game)["P2"]["hand"].as_array().unwrap().len(), size_after_fanfare, "only Abysscraft triggers the Fanfare discard");
        game.internal_rule_action(&json!({"do":"attack","attacker":"pain","target":attack_target})).unwrap();
        finish(&mut game, None, &[]);
        assert_eq!(view(&game)["P2"]["hand"].as_array().unwrap().len(), size_after_fanfare-1, "Strike discards from the controller of {attack_target}");
    }
}

#[test]
fn wish_wielder_searches_only_for_runecraft_but_discounts_original_seven_in_both_classes() {
    for (leader_class, searches) in [("ウィッチ", true), ("ドラゴン", false)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!(leader_class);
        let wish = json!({"id":"wish","card":"GFD01-022EN"});
        let seven = json!({"id":"seven","card":"BP02-003"});
        let six = json!({"id":"six","card":"BP03-001"});
        let hand = if searches { json!([wish]) } else { json!([wish, seven.clone()]) };
        let deck = if searches { json!([six, seven]) } else { json!([six]) };
        initial["players"]["P1"]["zones"]["hand"] = hand;
        initial["players"]["P1"]["zones"]["deck"] = deck;
        let mut game = game(&initial);
        let played = game.play_internal_card("wish", actor()).unwrap();
        finish(&mut game, Some(played), &[]);
        let after_fanfare = view(&game);
        assert_eq!(after_fanfare["P1"]["hand"], json!(["seven"]), "the Runecraft search selects a cost-seven follower and leaves the cost-six follower in the deck");
        assert_eq!(after_fanfare["P1"]["deck"], json!(["six"]), "the selected follower is removed from the deck");
        let discounted = game.play_internal_card("seven", actor()).unwrap();
        finish(&mut game, Some(discounted), &[]);
        assert_eq!(view(&game)["P1"]["pp"]["current"], 2_u8, "the continuous reduction applies with leader {leader_class}");
    }
}
