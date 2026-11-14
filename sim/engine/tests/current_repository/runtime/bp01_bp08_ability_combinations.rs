use super::ability_combinations::*;
use super::*;

#[test]
fn bp03_121_counts_only_cemetery_names_and_hits_both_fields_and_the_opponent() {
    let mut initial = position("BP03-121", "hand");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ally","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"egg","card":"BP03-121"},{"id":"humpty","card":"BP03-115"},{"id":"other","card":"BP01-171"}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(state["objects"]["ally"]["damage"], 3_u8);
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
}

#[test]
fn bp07_049_legal_evolution_creates_an_earth_sigil() {
    let initial = evolve_position("BP07-048", "BP07-049");
    let mut game = loaded(&initial);
    evolve(&mut game, 1);
    let state = view(&game);
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
    assert!(
        state["objects"]
            .as_object()
            .unwrap()
            .values()
            .any(|o| o["name"] == "大地の魔片")
    );
}

#[test]
fn bp07_093_hand_payment_follows_the_discarded_generation_to_third_or_bottom() {
    for count in [0_usize, 1, 4] {
        let mut initial = position("BP07-093", "hand");
        initial["players"]["P1"]["leader"]["life"] = json!(17_u8);
        let ids = ["one", "two", "three", "four"];
        initial["players"]["P1"]["zones"]["deck"] = json!(
            ids.iter()
                .take(count)
                .map(|id| json!({"id":id,"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut game = loaded(&initial);
        activate_card(&mut game, "BP07-093", 2, &[]);
        let state = view(&game);
        assert_eq!(state["P1"]["leader"]["life"], 18_u8);
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert_eq!(state["P1"]["hand"], json!([]));
        assert_eq!(state["P1"]["cemetery"], json!([]));
        let mut expected: Vec<_> = ids.iter().take(count).copied().collect();
        expected.insert(count.min(2), "source");
        assert_eq!(state["P1"]["deck"], json!(expected));
    }
}

#[test]
fn bp01_031_lastword_gives_ward_and_posture_only_to_its_new_knights() {
    let mut initial = position("BP01-031", "field");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"existing","card":"BP01-T07","state":{"acted":false}}));
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"removal","card":"BP01-169"}]);
    let mut game = loaded(&initial);
    let CardPlay::Awaiting(q) = game
        .play_internal_card("removal", PlayerId { seat: 2 })
        .unwrap()
    else {
        panic!("removal target");
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("removal target");
    };
    let target = domain
        .members
        .iter()
        .find(|m| m.card.as_deref() == Some("BP01-031"))
        .unwrap()
        .handle;
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![target]))
        .unwrap();
    finish(&mut game, Some(step), &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 3);
    assert_eq!(state["objects"]["existing"]["acted"], false);
    assert!(
        !state["objects"]["existing"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("守護"))
    );
    for id in field.iter().filter(|id| **id != json!("existing")) {
        let object = &state["objects"][id.as_str().unwrap()];
        assert_eq!(object["name"], "スティールナイト");
        assert!(
            object["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
        assert_eq!(object["acted"], true);
    }
}

#[test]
fn bp02_088_necro_ten_can_revive_and_create_ghosts_together() {
    let mut initial = position("BP02-088", "hand");
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0_u8..10)
            .map(|i| json!({"id":format!("dead-{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    let mut game = loaded(&initial);
    play(&mut game, &[ModeId(1), ModeId(2)]);
    let state = view(&game);
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 3);
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("dead-0"))
    );
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
}

#[test]
fn bp06_059_ex_activation_pays_the_source_before_buffing_and_locking_attacks() {
    let mut initial = position("BP06-059", "ex");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    let before = view(&game);
    game.play_internal_card("source", PLAYER).unwrap_err();
    assert_eq!(view(&game), before);
    activate_card(&mut game, "BP06-059", 3, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    assert_eq!(state["objects"]["ally"]["power"], 6_u8);
    assert_eq!(state["objects"]["ally"]["hp"], 6_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"enemy"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
}

#[test]
fn bp07_004_discarded_by_the_opponent_creates_the_tree_in_its_controllers_ex() {
    let mut initial = position("BP02-106", "hand");
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"giant","card":"BP07-004"}]);
    let mut game = loaded(&initial);
    play(&mut game, &[ModeId(1), ModeId(4)]);
    let state = view(&game);
    assert_eq!(state["P2"]["cemetery"], json!(["giant"]));
    let ex = state["P2"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 1);
    assert_eq!(
        state["objects"][ex[0].as_str().unwrap()]["card"],
        "BP07-T03"
    );
}

#[test]
fn bp08_083_opponents_discard_on_our_turn_revives_the_cemetery_source_for_one_pp() {
    let mut initial = position("BP02-106", "hand");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"revived","card":"BP08-083"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"discard","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    play(&mut game, &[ModeId(1), ModeId(4)]);
    let state = view(&game);
    assert_eq!(state["P1"]["field"], json!(["source", "revived"]));
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
}

#[test]
fn bp04_022_fanfare_reveals_two_commanders_to_restore_two_pp() {
    let mut initial = position("BP04-022", "hand");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"reveal-one","card":"BP01-037"}),
            json!({"id":"reveal-two","card":"BP01-037"}),
        ]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    assert_eq!(state["P1"]["hand"], json!(["reveal-one", "reveal-two"]));
}

#[test]
fn bp02_111_evolution_counts_then_flips_all_face_up_evolve_followers() {
    let mut initial = evolve_position("BP02-110", "BP02-111");
    initial["players"]["P1"]["pp"]["current"] = json!(5_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"up-one","card":"BP01-003","face_up":true}),
            json!({"id":"up-two","card":"BP03-116","face_up":true}),
        ]);
    let mut game = loaded(&initial);
    evolve(&mut game, 1);
    let state = view(&game);
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
    for id in ["up-one", "up-two"] {
        assert_eq!(state["objects"][id]["face_up"], false);
    }
}

#[test]
fn bp07_049_paid_evolution_mode_distributes_damage_before_using_earth_rite() {
    let mut initial = evolve_position("BP07-048", "BP07-049");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}}));
    initial["players"]["P2"]["zones"]["field"].as_array_mut().unwrap().push(
        json!({"id":"second","card":"BP01-037","state":{"acted":true,"hp":30_u8,"max_hp":30_u8}})
    );
    let mut game = loaded(&initial);
    evolve_with_modes(&mut game, 1, &[ModeId(2)]);
    let state = view(&game);
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["objects"]["second"]["damage"], 2_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["soil"]));
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp03_121_replayed_from_the_cemetery_excludes_itself_from_x() {
    let mut initial = evolve_position("BP07-108", "BP07-109");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"egg","card":"BP03-121"},{"id":"humpty","card":"BP03-115"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = Value::Array(
        (0_u8..5_u8)
            .map(|index| json!({"id":format!("mill-{index}"),"card":"BP01-171"}))
            .collect(),
    );
    let mut game = loaded(&initial);
    evolve(&mut game, 2);
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
        .unwrap();
    finish(&mut game, None, &[]);
    let state = view(&game);
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["objects"]["source"]["damage"], 2_u8);
    assert_eq!(state["P2"]["leader"]["life"], 15_u8);
    assert_eq!(state["P1"]["pp"]["current"], 3_u8);
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("egg"))
    );
}
