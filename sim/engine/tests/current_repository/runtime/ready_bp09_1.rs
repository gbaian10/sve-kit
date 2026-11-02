#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };
type Support = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);
const CONTROLS: [Support; 14] = [
    ("plain", "ordinary", "ニュートラル", "フォロワー", "1"),
    ("imera", "白の王・イメラ", "ロイヤル", "フォロワー", "1"),
    ("amulet", "amulet", "ニュートラル", "アミュレット", "1"),
    ("elfamulet", "Elf amulet", "エルフ", "アミュレット", "1"),
    ("elf-follower", "Elf follower", "エルフ", "フォロワー", "1"),
    ("e1", "first Elf spell", "エルフ", "スペル", "1"),
    ("e1copy", "first Elf spell", "エルフ", "スペル", "1"),
    ("e2", "second Elf spell", "エルフ", "スペル", "2"),
    ("e3", "third Elf spell", "エルフ", "スペル", "1"),
    ("e4", "fourth Elf spell", "エルフ", "スペル", "1"),
    ("e5", "fifth Elf spell", "エルフ", "スペル", "1"),
    ("dragoncast", "dragon spell", "ドラゴン", "スペル", "0"),
    ("othercast", "other spell", "ニュートラル", "スペル", "0"),
    ("dragon", "dragon follower", "ドラゴン", "フォロワー", "1"),
];
fn prepared() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static FIXTURE: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    FIXTURE.get_or_init(|| {
        let(mut snapshot,keywords,mut input)=inputs().clone();
        for(number,name,class,kind,cost) in CONTROLS {
            let follower=kind=="フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":class,"card_type":kind,"cost":cost,"power":if follower {"1"} else {"-"},"hp":if follower {"9"} else {"-"},"traits":[],"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        let selected=repository::selection_with_rules_from(&snapshot,&keywords,&input);
        let mut tree=expanded::spell_tree().clone();
        let template=tree["cards"]["SD03-005"].clone();
        tree["cards"]=json!({});
        let mut costs=BTreeMap::new();
        for number in ["dragoncast","othercast"] {
            let mut card=template.clone();
            let mut marker=template["abilities"][0].clone();
            marker.as_object_mut().unwrap().remove("spell");
            marker["line"]=json!(1_u8);
            marker["keywords"]=json!(["quick"]);
            marker["sources"][0].as_object_mut().unwrap().remove("ability_scope");
            let mut spell=template["abilities"][0].clone();
            spell["spell"]=json!({"do":[{"draw":{"n":1_u8}}]});
            card["abilities"]=json!([marker,spell]);
            tree["cards"][number]=card;costs.insert(number.to_owned(),Nat(0));
        }
        (selected,lowerer().lower_with_printed_costs(&tree,&costs).unwrap())
    })
}
fn build(initial: &Value) -> Game {
    let (selected, programs) = prepared();
    let mut board = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-bp09-first",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        board
            .load_internal_card(number, programs.get(number).unwrap_or(&empty))
            .unwrap();
    }
    board
}
fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0..4_u8)
                .map(|n| json!({"id":format!("{seat}-draw{n}"),"card":"plain"}))
                .collect::<Vec<_>>()
        );
    }
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"plain"}]);
    initial
}
fn state(board: &Game) -> Value {
    board.projection(View::Referee).unwrap()
}
fn checks(board: &mut Game) {
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}
fn finish(board: &mut Game, mut play: CardPlay, take: usize) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => {
                let ids = domain
                    .members
                    .iter()
                    .take(take.min(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap()))
                    .map(|member| member.handle)
                    .collect();
                if matches!(&question.domain, InputDomain::Targets(_)) {
                    Answer::Targets(ids)
                } else {
                    Answer::CostMaterials(ids)
                }
            }
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(take.min(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap()))
                    .map(|member| member.handle)
                    .collect(),
            )),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            _ => panic!("unexpected decision: {:?}", question.domain),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed action has available resources"
    );
    checks(board);
}
fn triggers(board: &mut Game, take: usize) {
    checks(board);
    while let Some(question) = board.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed trigger")
        };
        let play = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(board, play, take);
    }
}
fn play(board: &mut Game, id: &str, take: usize) {
    let step = board.play_internal_card(id, PLAYER).unwrap();
    finish(board, step, take);
    triggers(board, take);
}
fn activate(board: &mut Game, number: &str, take: usize) {
    let id = prepared().0.current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = board.activate_internal_card("source", id, PLAYER).unwrap();
    finish(board, step, take);
    triggers(board, take);
}
fn evolve(board: &mut Game, source: &str, form: &str) {
    assert_eq!(
        board
            .internal_rule_action(
                &json!({"do":"evolve","source":source,"evolve_card":form,"pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
        "resolved",
        "the real base pays one PP to evolve"
    );
    checks(board);
}
fn quick(board: &mut Game, number: &str) {
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = board.internal_quick_choice().unwrap().unwrap();
    assert_eq!(
        question.actor, PLAYER,
        "the nonactive player receives the Quick window"
    );
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick options")
    };
    let chosen = options.iter().find(|option| option.card == number).unwrap();
    let step = board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Quick(Some(chosen.handle)),
        )
        .unwrap();
    finish(board, step, usize::MAX);
    triggers(board, usize::MAX);
}

fn pending_one(board: &mut Game, number: &str) -> CardPlay {
    checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending ability")
    };
    assert_eq!(options.len(), 1, "one printed ability is pending");
    assert_eq!(
        options[0].card, number,
        "the requested printing supplies the ability"
    );
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
}

#[test]
fn grasshopper_base_counts_names_instead_of_cards_and_ignores_other_classes_and_kinds() {
    for count in [4_usize, 5] {
        let mut initial = position("BP09-012", "hand");
        let mut grave: Vec<_> = ["e1", "e2", "e3", "e4", "e5"]
            .iter()
            .take(count)
            .enumerate()
            .map(|(n, card)| json!({"id":format!("old{n}"),"card":card}))
            .collect();
        for (id, card) in [
            ("duplicate", "e1copy"),
            ("wrong-class", "dragoncast"),
            ("wrong-kind", "elfamulet"),
        ] {
            grave.push(json!({"id":id,"card":card}));
        }
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        let mut board = build(&initial);
        play(&mut board, "source", usize::MAX);
        let current = state(&board);
        assert_eq!(
            current["objects"]["enemy"]["damage"],
            if count == 5 { 4_u8 } else { 0_u8 },
            "five distinct Elf-spell names are required"
        );
        assert_eq!(
            current["P1"]["pp"]["current"], 7_u8,
            "playing pays the printed three PP"
        );
        assert!(
            current["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "the entire follower is playable at both thresholds"
        );
    }
}

#[test]
fn grasshopper_real_evolution_searches_only_original_cost_one_or_less_elf_spells() {
    let mut initial = position("BP09-012", "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP09-013"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"too-expensive","card":"e2"},{"id":"wrong-class","card":"dragoncast"},{"id":"wrong-kind","card":"elf-follower"},{"id":"boundary","card":"e1"}]);
    let mut board = build(&initial);
    evolve(&mut board, "source", "form");
    let step = pending_one(&mut board, "BP09-013");
    let CardPlay::Awaiting(question) = &step else {
        panic!("deck search")
    };
    let InputDomain::Search(domain) = &question.domain else {
        panic!("search candidates")
    };
    assert_eq!(
        domain.cards.len(),
        1,
        "cost two, non-Elf, and nonspell cards are excluded"
    );
    assert_eq!(
        domain.cards[0].card, "e1",
        "the cost-one boundary qualifies"
    );
    finish(&mut board, step, 1);
    let current = state(&board);
    assert_eq!(
        current["P1"]["hand"],
        json!(["boundary"]),
        "the selected spell reaches hand"
    );
    assert_eq!(
        current["P1"]["pp"]["current"], 9_u8,
        "the real base pays evolution cost one"
    );
    assert_eq!(
        current["objects"]["source"]["power"], 4_u8,
        "the evolved face supplies printed power"
    );
    assert_eq!(
        current["objects"]["source"]["hp"], 4_u8,
        "the evolved face supplies printed health"
    );
}

fn assert_magnus_aura(before: &Value) {
    assert!(
        before["objects"]["imera"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃")),
        "a same-name ordinary follower gains the aura keyword"
    );
    assert!(
        before["objects"]["royal"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃")),
        "the real BP09-027 gains Assail from Magnus"
    );
    for id in ["source", "other", "enemy-imera"] {
        assert!(
            !before["objects"][id]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("指定攻撃")),
            "the aura excludes wrong names and opposing control"
        );
    }
}
fn reject_acted_activation(board: &mut Game, ability: AbilityId) {
    let mut result = board.activate_internal_card("source", ability, PLAYER);
    if let Ok(CardPlay::Awaiting(question)) = result {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("unpayable activation targets")
        };
        assert!(
            !domain.members.is_empty(),
            "the posture rejection case has a legal target"
        );
        result = board.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        );
    }
    assert!(
        matches!(result, Err(EngineFailure::IllegalPlay(_))),
        "an already acted source cannot complete payment: {result:?}"
    );
}
fn magnus_case(number: &str, take: usize) {
    let mut initial = position(number, "field");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"imera","card":"imera"}),
            json!({"id":"royal","card":"BP09-027"}),
            json!({"id":"other","card":"plain"}),
        ]);
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"enemy-imera","card":"imera"}));
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"removal","card":"BP09-113"}]);
    let mut board = build(&initial);
    let before = state(&board);
    assert_magnus_aura(&before);
    let ability = prepared().0.current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = board
        .activate_internal_card("source", ability, PLAYER)
        .unwrap();
    let CardPlay::Awaiting(question) = &step else {
        panic!("other Royal followers")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("activation targets")
    };
    assert_eq!(
        domain.members.len(),
        2,
        "source, non-Royal, and opponent followers are excluded"
    );
    finish(&mut board, step, take);
    let current = state(&board);
    let raised = ["imera", "royal"]
        .iter()
        .filter(|id| {
            current["objects"][**id]["power"].as_u64().unwrap()
                == before["objects"][**id]["power"]
                    .as_u64()
                    .unwrap()
                    .checked_add(1)
                    .unwrap()
        })
        .count();
    assert_eq!(
        raised, take,
        "exactly the chosen up-to-two followers gain one power"
    );
    assert_eq!(
        current["objects"]["source"]["acted"], true,
        "the activated cost acts Magnus"
    );
    assert_eq!(
        current["objects"]["source"]["power"], 2_u8,
        "Magnus cannot buff itself"
    );
    for id in ["imera", "royal"] {
        assert_eq!(
            current["objects"][id]["hp"], before["objects"][id]["hp"],
            "the ability does not add health"
        );
    }
    reject_acted_activation(&mut board, ability);
    assert_eq!(
        state(&board),
        current,
        "rejected repeat activation leaves the position unchanged"
    );
    remove_magnus(&mut board, number);
    for id in ["imera", "royal"] {
        assert!(
            !state(&board)["objects"][id]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("指定攻撃")),
            "the aura ends when Magnus leaves"
        );
    }
}
fn remove_magnus(board: &mut Game, number: &str) {
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = board.internal_quick_choice().unwrap().unwrap();
    assert_eq!(
        question.actor,
        PlayerId { seat: 2 },
        "the opposing player receives the removal window"
    );
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("opposing Quick removal")
    };
    let option = options
        .iter()
        .find(|option| option.card == "BP09-113")
        .unwrap();
    let step = board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Quick(Some(option.handle)),
        )
        .unwrap();
    let CardPlay::Awaiting(target) = step else {
        panic!("removal target")
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("removal target domain")
    };
    let source = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(number))
        .unwrap();
    let done = board
        .answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![source.handle]),
        )
        .unwrap();
    finish(board, done, 1);
}
#[test]
fn magnus_grants_assail_only_to_its_named_ally_and_acts_to_buff_zero_one_or_two_others() {
    for number in ["BP09-028", "BP09-P10"] {
        for take in 0..=2_usize {
            magnus_case(number, take);
        }
    }
}

#[test]
fn frontline_rampart_pays_act_and_departure_for_damage_including_opponent_quick_window() {
    for opponent_turn in [false, true] {
        let mut initial = position("BP09-034", "field");
        if opponent_turn {
            initial["turn"]["active"] = json!("P2");
        }
        let mut board = build(&initial);
        if opponent_turn {
            quick(&mut board, "BP09-034");
        } else {
            activate(&mut board, "BP09-034", 1);
        }
        let current = state(&board);
        assert_eq!(
            current["objects"]["enemy"]["damage"], 3_u8,
            "the chosen enemy takes three damage"
        );
        assert_eq!(
            current["P1"]["field"],
            json!([]),
            "payment removes the source amulet"
        );
        assert_eq!(
            current["P1"]["cemetery"],
            json!(["source"]),
            "the source is sent to cemetery as a cost"
        );
        assert_eq!(
            current["P1"]["pp"]["current"], 10_u8,
            "activation has no printed PP cost"
        );
    }
    let mut initial = position("BP09-034", "field");
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut board = build(&initial);
    let before = state(&board);
    let ability = prepared().0.current["BP09-034"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    assert!(
        matches!(
            board.activate_internal_card("source", ability, PLAYER),
            Err(EngineFailure::IllegalPlay(_))
        ),
        "without an enemy target activation is illegal"
    );
    assert_eq!(
        state(&board),
        before,
        "failed targeting does not act or sacrifice the source"
    );
    let mut acted_initial = position("BP09-034", "field");
    acted_initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"acted":true});
    let mut acted_board = build(&acted_initial);
    let acted_before = state(&acted_board);
    reject_acted_activation(&mut acted_board, ability);
    assert_eq!(
        state(&acted_board),
        acted_before,
        "failed posture payment does not sacrifice the amulet"
    );
}

fn dragon_armor_played(cast: &str, opponent_turn: bool) {
    let mut initial = position("BP09-057", "field");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"cast","card":cast}]);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"plain"}));
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"second","card":"plain"}),
            json!({"id":"charm","card":"amulet"}),
        ]);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"own-charm","card":"amulet"}));
    if opponent_turn {
        initial["turn"]["active"] = json!("P2");
    }
    let mut board = build(&initial);
    if opponent_turn {
        quick(&mut board, cast);
    } else {
        play(&mut board, "cast", 1);
    }
    let current = state(&board);
    for id in ["enemy", "second"] {
        assert_eq!(
            current["objects"][id]["damage"],
            if cast == "dragoncast" && !opponent_turn {
                2_u8
            } else {
                0_u8
            },
            "only an own-turn Dragon spell triggers the sweep"
        );
    }
    assert_eq!(
        current["objects"]["source"]["damage"], 0_u8,
        "the sweep excludes the source"
    );
    assert_eq!(
        current["objects"]["ally"]["damage"], 0_u8,
        "the sweep excludes allied followers"
    );
    assert!(
        current["P2"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("charm")),
        "the sweep leaves amulets intact"
    );
}
#[test]
fn dragon_armor_filters_own_turn_dragon_spells_and_recovers_pp_only_when_awakened() {
    for cast in ["dragoncast", "othercast"] {
        for opponent_turn in [false, true] {
            dragon_armor_played(cast, opponent_turn);
        }
    }
    dragon_armor_played("dragon", false);
    for maximum in [6_u8, 7] {
        let mut initial = position("BP09-057", "hand");
        initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
        let mut board = build(&initial);
        play(&mut board, "source", 1);
        assert_eq!(
            state(&board)["P1"]["pp"]["current"],
            if maximum == 7 { 4_u8 } else { 1_u8 },
            "five PP is paid before awakening restores two"
        );
    }
}

#[test]
fn dragonwing_assassin_counts_only_own_cemetery_dragon_spells_for_damage() {
    for count in [0_usize, 1, 3] {
        let mut initial = position("BP09-066", "hand");
        let mut grave: Vec<_> = (0..count)
            .map(|n| json!({"id":format!("spell{n}"),"card":"dragoncast"}))
            .collect();
        grave.extend([
            json!({"id":"wrong-kind","card":"dragon"}),
            json!({"id":"wrong-class","card":"othercast"}),
        ]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        initial["players"]["P2"]["zones"]["cemetery"] =
            json!([{"id":"opponent-spell","card":"dragoncast"}]);
        let mut board = build(&initial);
        play(&mut board, "source", 1);
        assert_eq!(
            state(&board)["objects"]["enemy"]["damage"],
            count,
            "counted damage excludes nonspells, other classes and opponent cemetery"
        );
    }
}

fn wrath_initial(own_evolved: bool, opponent_evolved: bool, ex_full: bool) -> Value {
    let mut initial = position("BP09-113", "hand");
    if ex_full {
        initial["players"]["P1"]["zones"]["ex"] = json!(
            (0..5_u8)
                .map(|n| json!({"id":format!("ex{n}"),"card":"plain"}))
                .collect::<Vec<_>>()
        );
    }
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ally","card":if own_evolved {"BP09-025"} else {"plain"}}]);
    if own_evolved {
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"real-form","card":"BP09-026"}]);
    }
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"second-enemy","card":if opponent_evolved {"BP09-025"} else {"plain"}}));
    if opponent_evolved {
        initial["players"]["P2"]["zones"]["field"][1]["state"] =
            json!({"evolved":true,"evolved_with":"opponent-form"});
        initial["players"]["P2"]["zones"]["evolution"] =
            json!([{"id":"opponent-form","card":"BP09-026"}]);
    }
    initial
}
fn divine_wrath_case(own_evolved: bool, opponent_evolved: bool, active_turn: &str, ex_full: bool) {
    let initial = wrath_initial(own_evolved, opponent_evolved, ex_full);
    let mut board = build(&initial);
    if own_evolved {
        evolve(&mut board, "ally", "real-form");
    }
    if active_turn == "P2" {
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        for _ in 0..4_u8 {
            let Some(question) = board.internal_quick_choice().unwrap() else {
                break;
            };
            let step = board
                .answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap();
            finish(&mut board, step, 1);
        }
        board
            .internal_rule_action(&json!({"do":"end-discard","select":[]}))
            .unwrap();
        checks(&mut board);
        quick(&mut board, "BP09-113");
    } else {
        play(&mut board, "source", 1);
    }
    let current = state(&board);
    if opponent_evolved {
        assert_eq!(
            current["objects"]["second-enemy"]["evolved_with"], "opponent-form",
            "the unchosen opponent still has a real evolved follower"
        );
        assert!(
            current["P2"]["evolution"]
                .as_array()
                .unwrap()
                .contains(&json!("opponent-form")),
            "the real evolution resource remains associated with that follower"
        );
    }
    assert!(
        current["P2"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("enemy")),
        "the selected opposing follower is destroyed"
    );
    assert_eq!(
        current["P1"]["ex"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        own_evolved && !ex_full,
        "return to EX needs an own evolved follower and capacity"
    );
    assert_eq!(
        current["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        !own_evolved || ex_full,
        "otherwise the spell resolves to cemetery"
    );
    if own_evolved && !ex_full && active_turn == "P1" {
        play(&mut board, "source", 1);
        let replayed = state(&board);
        assert!(
            replayed["P2"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("second-enemy")),
            "the returned complete spell can be cast again from EX"
        );
        assert_eq!(
            replayed["P1"]["pp"]["current"], 1_u8,
            "real evolution and two four-PP casts are all paid"
        );
        assert!(
            replayed["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "the second cast returns to EX again"
        );
    }
}
#[test]
fn divine_wrath_destroys_then_returns_to_ex_only_with_an_own_real_evolution() {
    for own_evolved in [false, true] {
        for opponent_evolved in [false, true] {
            for active_turn in ["P1", "P2"] {
                for ex_full in [false, true] {
                    divine_wrath_case(own_evolved, opponent_evolved, active_turn, ex_full);
                }
            }
        }
    }
}
