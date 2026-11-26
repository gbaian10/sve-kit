use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };
type SupportCard = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);
const CONTROLS: [SupportCard; 16] = [
    ("one", "ordinary", "ニュートラル", "フォロワー", "1", &[]),
    ("royal2", "royal2", "ロイヤル", "フォロワー", "2", &[]),
    ("hunter3", "hunter3", "エルフ", "フォロワー", "3", &["狩人"]),
    ("elf4", "elf4", "エルフ", "フォロワー", "4", &[]),
    ("elfamulet", "elfamulet", "エルフ", "アミュレット", "5", &[]),
    ("amulet", "amulet", "ニュートラル", "アミュレット", "1", &[]),
    (
        "dragon3",
        "dragon3",
        "ドラゴン",
        "フォロワー",
        "3",
        &["竜族"],
    ),
    ("night5", "night5", "ナイトメア", "フォロワー", "5", &[]),
    ("weak", "weak", "ニュートラル", "フォロワー", "1", &[]),
    (
        "116-base",
        "ハイエンチャンター",
        "ニュートラル",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "medusa-form",
        "スウィート★メドゥーサ",
        "ウィッチ",
        "フォロワー・エボルヴ",
        "-",
        &[],
    ),
    ("oppsend", "oppsend", "ニュートラル", "フォロワー", "1", &[]),
    ("wipe", "wipe", "ニュートラル", "スペル", "0", &[]),
    ("selfwipe", "selfwipe", "ニュートラル", "スペル", "0", &[]),
    (
        "bounceopp",
        "bounceopp",
        "ニュートラル",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "bounceown",
        "bounceown",
        "ニュートラル",
        "フォロワー",
        "1",
        &[],
    ),
];
// Synthetic supports isolate filters and evolution forms from unrelated admission gaps.
fn prepared() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static READY: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, class, kind, cost, traits) in CONTROLS {
            let follower = kind.starts_with("フォロワー");
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":class,"card_type":kind,"cost":cost,"power":if number == "weak" {"1"} else if follower {"2"} else {"-"},"hp":if follower {"8"} else {"-"},"traits":traits,"text":if kind.contains("エボルヴ") {json!("")} else {Value::Null},"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        let selected = repository::selection_with_rules_from(&snapshot, &keywords, &input);
        let mut tree = expanded::tree().clone();
        tree["cards"] = json!({});
        let template = expanded::tree()["cards"]["synthetic-evolution"].clone();
        for (number, ..) in CONTROLS {
            if number.ends_with("-base") { tree["cards"][number] = template.clone(); }
        }
        let mut sender=template["abilities"][0].clone();
        sender.as_object_mut().unwrap().remove("evolve");
        sender["activate"]=json!({"cost":{"pp":0_u8},"do":[{"move":{"ref":{"who":"opp","zone":"field"},"to":"cemetery"}}]});
        tree["cards"]["oppsend"]=json!({"meta":template["meta"],"abilities":[sender]});
        for (number,who) in [("bounceopp","opp"),("bounceown","self")] {
            let mut bounce=tree["cards"]["oppsend"]["abilities"][0].clone();
            let mut query=json!({"who":who,"zone":"field","kind":"follower"});
            if number=="bounceown" {query["name"]=json!("dragon3");}
            bounce["activate"]=json!({"cost":{"pp":0_u8},"do":[{"move":{"ref":query,"to":"hand"}}]});
            tree["cards"][number]=json!({"meta":template["meta"],"abilities":[bounce]});
        }
        let mut programs=lowerer().lower(&tree).unwrap();
        let mut spells=expanded::spell_tree().clone();
        let spell=spells["cards"]["SD03-005"].clone();
        spells["cards"]=json!({});
        let mut costs=BTreeMap::new();
        for (number,who) in [("wipe","opp"),("selfwipe","self")] {
            let mut card=spell.clone();
            card["abilities"][0]["spell"]=json!({"do":[{"destroy":{"ref":{"who":who,"zone":"field","kind":"follower"}}}]});
            spells["cards"][number]=card;costs.insert(number.to_owned(),Nat(0));
        }
        programs.extend(lowerer().lower_with_printed_costs(&spells,&costs).unwrap());
        (selected,programs)
    })
}
fn new_game(initial: &Value) -> Game {
    let (selected, bases) = prepared();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-bp08-extra",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, _, _, kind, ..) in CONTROLS {
        if kind.contains("エボルヴ") {
            continue;
        }
        game.load_internal_card(number, bases.get(number).unwrap_or(&empty))
            .unwrap();
    }
    game
}
fn position(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..10_u8)
            .map(|i| json!({"id":format!("deck{i}"),"card":"one"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["deck"] = copies("one", "opponent-deck", 3);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"one","state":{"acted":true}}]);
    initial
}
#[derive(Clone, Copy)]
struct Decisions {
    mode: usize,
    accept: bool,
    take: usize,
}
const DEFAULT: Decisions = Decisions {
    mode: 0,
    accept: true,
    take: usize::MAX,
};
fn finish(game: &mut Game, step: CardPlay) {
    finish_with(game, step, DEFAULT);
}
fn finish_with(game: &mut Game, mut step: CardPlay, policy: Decisions) {
    while let CardPlay::Awaiting(question) = step {
        let public = game.projection(View::P2).unwrap();
        for secret in ["secret1", "secret2", "secret3", "secret4"] {
            assert!(
                !public["known_cards"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["id"] == secret),
                "unselected looked cards stay private at every decision"
            );
        }
        let answer = match &question.domain {
            InputDomain::Targets(d) | InputDomain::CostMaterials(d) => {
                let count = usize::try_from(d.cardinality.maximum.unwrap().0)
                    .unwrap()
                    .min(policy.take);
                let ids = d.members.iter().take(count).map(|m| m.handle).collect();
                if matches!(&question.domain, InputDomain::CostMaterials(_)) {
                    Answer::CostMaterials(ids)
                } else {
                    Answer::Targets(ids)
                }
            }
            InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
                d.cards
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap().0)
                            .unwrap()
                            .min(policy.take),
                    )
                    .map(|m| m.handle)
                    .collect(),
            )),
            InputDomain::Order(d) => Answer::Order(d.iter().rev().map(|m| m.handle).collect()),
            InputDomain::OptionalCost => Answer::OptionalCost(policy.accept),
            InputDomain::Integer(bounds) => {
                assert_eq!(
                    bounds.minimum,
                    Nat(0),
                    "the only integer decision is optional acceptance"
                );
                assert_eq!(
                    bounds.maximum,
                    Some(Nat(1)),
                    "optional effects use a boolean decision"
                );
                Answer::Integer(Nat(u64::from(policy.accept)))
            }
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[policy.mode]]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::TokenCapacity { .. } => Answer::TokenCapacity(0),
            _ => panic!("unexpected decision: {:?}", question.domain),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(step, CardPlay::Cancelled),
        "the printed action has the required resources"
    );
    checks(game);
}

fn named(state: &Value, zone: &str, name: &str) -> usize {
    state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}
fn copies(card: &str, prefix: &str, count: usize) -> Value {
    json!(
        (0..count)
            .map(|i| json!({"id":format!("{prefix}{i}"),"card":card}))
            .collect::<Vec<_>>()
    )
}
fn play(game: &mut Game, id: &str) {
    play_as(game, id, PLAYER);
}
fn play_as(game: &mut Game, id: &str, actor: PlayerId) {
    let step = game.play_internal_card(id, actor).unwrap();
    finish(game, step);
}
fn pending(game: &mut Game) -> CardPlay {
    checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    assert_eq!(options.len(), 1, "only the tested ability is pending");
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}
fn trigger(game: &mut Game) {
    let step = pending(game);
    finish(game, step);
}
fn support(game: &mut Game, number: &str, id: &str) {
    let ability = prepared().1[number].card().abilities[0].node.id;
    let step = game.activate_internal_card(id, ability, PLAYER).unwrap();
    finish(game, step);
}

fn only_target(game: &mut Game, step: CardPlay, allowed: &[&str]) {
    let CardPlay::Awaiting(question) = step else {
        panic!("the printed mandatory target")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("opposing field follower target")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the opposing enemy qualifies, never the own high-cost control"
    );
    let chosen = domain.members.first().unwrap();
    assert!(
        allowed.contains(&chosen.card.as_deref().unwrap()),
        "the sole public candidate is the enemy or its real evolution face"
    );
    let selected = chosen.handle;
    let resumed = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![selected]),
        )
        .unwrap();
    finish(game, resumed);
}
fn activation(number: &str, line: u32) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line)
        .unwrap()
        .node
        .id
}
fn pending_damage(game: &mut Game, count: usize, leader: bool) {
    checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("each qualifying departure triggers once")
    };
    assert_eq!(
        options.len(),
        count,
        "one pending ability is queued per qualifying object in the same batch"
    );
    let CardPlay::Awaiting(target) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("the printed recipient is selected")
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("leader or follower formal target")
    };
    assert_eq!(
        domain.members.len(),
        2,
        "only the opposing leader and the opposing field follower are legal"
    );
    let member = domain
        .members
        .iter()
        .find(|m| m.card.is_none() == leader)
        .unwrap();
    let step = game
        .answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![member.handle]),
        )
        .unwrap();
    finish(game, step);
}

#[test]
fn furious_dragon_queen_creates_one_or_three_guards_then_observes_the_other_dragons_in_a_simultaneous_death_batch()
 {
    for number in ["BP08-052", "BP08-SL10"] {
        for awakened in [false, true] {
            for leader in [false, true] {
                queen_guard_batch(number, awakened, leader);
            }
        }
        queen_departure_exclusions(number);
    }
}
fn queen_guard_batch(number: &str, awakened: bool, leader: bool) {
    let mut initial = position(number, "hand");
    initial["players"]["P1"]["pp"] =
        json!({"current":if awakened {7_u8} else {6},"max":if awakened {7_u8} else {6}});
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"wipe","card":"selfwipe"}));
    let mut game = new_game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let count = if awakened { 3_usize } else { 1 };
    let created_guards = game.projection(View::Referee).unwrap();
    assert_eq!(
        named(&created_guards, "field", "竜帝の近衛兵"),
        count,
        "Awakened replaces the one-token creation with three"
    );
    for id in created_guards["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| id.as_str() != Some("source"))
    {
        assert!(
            created_guards["objects"][id.as_str().unwrap()]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進")),
            "every real Guard token has Rush"
        );
    }
    play(&mut game, "wipe");
    for remaining in (1..=count).rev() {
        pending_damage(&mut game, remaining, leader);
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["leader"]["life"],
        20_usize
            .checked_sub(if leader { count } else { 0 })
            .unwrap(),
        "each qualifying other Dragon can damage the opposing leader"
    );
    assert_eq!(
        state["objects"]["enemy"]["damage"],
        if leader { 0 } else { count },
        "the follower recipient is also available"
    );
    assert_eq!(
        state["P1"]["field"],
        json!([]),
        "Q1084 observes the other Dragons even when the Queen leaves in the same batch"
    );
}
fn queen_departure_exclusions(number: &str) {
    for scenario in 0..3_u8 {
        let mut excluded = position(number, "field");
        let opposing_turn = scenario == 2;
        let actor = if opposing_turn { "P2" } else { "P1" };
        if opposing_turn {
            excluded["turn"]["active"] = json!("P2");
        }
        let victim_owner = if scenario == 1 { "P2" } else { "P1" };
        let victim_card = if scenario == 0 { "one" } else { "dragon3" };
        excluded["players"][victim_owner]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"excluded","card":victim_card}));
        excluded["players"][actor]["zones"]["hand"] =
            json!([{"id":"wipe","card":if scenario==0 {"selfwipe"} else {"wipe"}}]);
        let mut excluded_game = new_game(&excluded);
        play_as(
            &mut excluded_game,
            "wipe",
            PlayerId {
                seat: if opposing_turn { 2 } else { 1 },
            },
        );
        checks(&mut excluded_game);
        assert!(
            excluded_game.projection(View::Referee).unwrap()[victim_owner]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("excluded")),
            "the wrong-class, wrong-controller, or off-turn Dragon really departed"
        );
        assert!(
            excluded_game.internal_pending_choice().unwrap().is_none(),
            "class, controller, and own-turn restrictions each exclude a real departure"
        );
    }
    let mut bounced = position(number, "field");
    bounced["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"tool","card":"bounceown"}),
            json!({"id":"returned","card":"dragon3"}),
        ]);
    let mut bounced_game = new_game(&bounced);
    support(&mut bounced_game, "bounceown", "tool");
    checks(&mut bounced_game);
    assert_eq!(
        bounced_game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["returned"]),
        "the other own Dragon really returned to hand while the Queen stayed on field"
    );
    assert!(
        bounced_game.internal_pending_choice().unwrap().is_none(),
        "a Dragon returned to hand does not satisfy the cemetery destination"
    );
}

#[test]
fn sylvia_can_destroy_any_enemy_and_damage_its_controller_or_pay_two_and_discard_itself_from_hand_for_a_cost_five_enemy()
 {
    for number in ["BP08-105", "BP08-SL21"] {
        sylvia_fanfare(number);
        sylvia_evolved_hand_target(number);
        sylvia_hand_target_boundaries(number);
    }
}
fn sylvia_fanfare(number: &str) {
    let mut initial = position(number, "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"own-five","card":"night5"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"one","owner":"P1"}]);
    let mut game = new_game(&initial);
    play(&mut game, "source");
    let target_step = pending(&mut game);
    only_target(&mut game, target_step, &["one"]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["leader"]["life"], 15_u8,
        "leader_of retains the selected controller after destruction sends an owned card elsewhere"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["enemy"]),
        "the destroyed follower still returns to its owner's cemetery"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 3_u8,
        "the printed seven-cost follower is paid normally"
    );
}
fn sylvia_evolved_hand_target(number: &str) {
    let mut evolved_initial = position(number, "hand");
    evolved_initial["players"]["P1"]["zones"]["field"] = json!([{"id":"own-five","card":"night5"}]);
    evolved_initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-086","state":{"evolved":true,"evolved_with":"enemy-form","acted":true}}]);
    evolved_initial["players"]["P2"]["zones"]["evolution"] =
        json!([{"id":"enemy-form","card":"BP01-087"}]);
    let mut evolved_game = new_game(&evolved_initial);
    let action = evolved_game
        .activate_internal_card("source", activation(number, 2), PLAYER)
        .unwrap();
    only_target(&mut evolved_game, action, &["BP01-086", "BP01-087"]);
    let evolved_state = evolved_game.projection(View::Referee).unwrap();
    assert_eq!(
        evolved_state["P2"]["cemetery"],
        json!(["enemy"]),
        "Q1133 uses the original five-cost base even though its evolved printing has no printed cost"
    );
    assert_eq!(
        evolved_state["P2"]["evolve_deck"],
        json!(["enemy-form"]),
        "the real evolved printing returns to its evolution deck when its base is destroyed"
    );
    assert_eq!(
        evolved_state["P1"]["pp"]["current"], 8_u8,
        "the same two-PP hand activation applies to an evolved target"
    );
}
fn sylvia_hand_target_boundaries(number: &str) {
    for eligible in [false, true] {
        let mut eligible_initial = position(number, "hand");
        eligible_initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"own-five","card":"night5"}]);
        eligible_initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"cheap","card":"elf4"},{"id":"wrong-kind","card":"elfamulet"}]);
        if eligible {
            eligible_initial["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"found","card":"night5"}));
        }
        let mut eligible_game = new_game(&eligible_initial);
        let before = eligible_game.projection(View::Referee).unwrap();
        let eligible_action =
            eligible_game.activate_internal_card("source", activation(number, 2), PLAYER);
        if !eligible {
            assert!(
                matches!(eligible_action, Err(EngineFailure::IllegalPlay(_))),
                "no qualifying target prevents both PP payment and the self-discard"
            );
            assert_eq!(
                eligible_game.projection(View::Referee).unwrap(),
                before,
                "an illegal activation leaves all public state unchanged"
            );
            continue;
        }
        finish(&mut eligible_game, eligible_action.unwrap());
        let eligible_state = eligible_game.projection(View::Referee).unwrap();
        assert_eq!(
            eligible_state["P1"]["pp"]["current"], 8_u8,
            "the hand ability pays two rather than the play cost"
        );
        assert_eq!(
            eligible_state["P1"]["hand"],
            json!([]),
            "the source itself is discarded as a mandatory cost"
        );
        assert_eq!(
            eligible_state["P1"]["cemetery"],
            json!(["source"]),
            "the discarded complete card reaches its cemetery"
        );
        assert_eq!(
            eligible_state["P2"]["field"],
            json!(["cheap", "wrong-kind"]),
            "only the original-cost-five follower qualifies"
        );
        assert_eq!(
            eligible_state["P2"]["leader"]["life"], 20_u8,
            "the hand activation does not include the unrelated Fanfare damage"
        );
    }
}
