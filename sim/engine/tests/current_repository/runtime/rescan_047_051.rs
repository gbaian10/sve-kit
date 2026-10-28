#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate real-card scenarios."
)]
use super::ex_select::{expanded, rule_fixtures};
use super::*;
use alloc::collections::BTreeMap;
use serde_json::Map;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, zone: &str, pp: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["pp"]["current"] = json!(pp);
    initial
}

fn trigger(game: &mut Game, card: &str) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("trigger")
    };
    assert_eq!(options.len(), 1_usize, "one real-card ability is pending");
    assert_eq!(
        options[0].card, card,
        "the actual printing supplies the ability"
    );
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
}

fn pass(game: &mut Game) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(
            game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ),
        "Quick Pass completes the stored attack"
    );
}

fn tokens(state: &Value, zone: &str, card: &str, count: usize) {
    let objects = state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| &state["objects"][id.as_str().unwrap()])
        .filter(|object| object["card"] == card)
        .collect::<Vec<_>>();
    assert_eq!(
        objects.len(),
        count,
        "the admitted token reaches the printed destination"
    );
    for token in objects {
        assert_eq!(
            token["power"], 1_u8,
            "token power comes from the real snapshot"
        );
        assert_eq!(
            token["hp"], 1_u8,
            "token health comes from the real snapshot"
        );
    }
}

// Only the external companion is synthetic; each real card retains its complete selected program.
struct Companions {
    selected: SelectedEffects,
    programs: BTreeMap<String, WellFormedCardProgram>,
}
fn companions() -> &'static Companions {
    static FIXTURE: OnceLock<Companions> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (snapshot, keywords, mut input) = inputs().clone();
        let mut snapshot = snapshot.trim_end().to_owned();
        let mut cards = Map::new();
        let mut tree = expanded::tree().clone();
        let template = tree["cards"]["synthetic-evolution"].clone();
        let mut programs = Map::new();
        for (companion, real, base) in [
            ("rescan047-sister", "SD06-012", true),
            ("rescan047-keeper", "SD01-003", false),
        ] {
            let record = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap())
                .find(|record| record["number"] == real).unwrap();
            let synthetic = json!({"number":companion,"faces":[{
                "name":record["faces"][0]["name"],"card_class":record["faces"][0]["card_class"],
                "card_type":if base {"フォロワー"} else {"フォロワー・エボルヴ"},
                "cost":if base {"1"} else {"-"},"power":"1","hp":"1","traits":[],"text":null,"sections":[]
            }]});
            snapshot.push('\n'); snapshot.push_str(&synthetic.to_string());
            cards.insert(companion.into(), json!({"status":"complete","abilities":[]}));
            if base { programs.insert(companion.into(), template.clone()); }
        }
        input["documents"].as_array_mut().unwrap().push(json!(["rescan047-companions.yaml", json!({"format":1_u8,"kind":"effect_set","cards":cards}).to_string()]));
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(root.join("authored/rules/engine/index.yaml")).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot,json!([]),json!([]))["input"].clone();
        let selected = Catalog::from_current_selection_with_rules(&snapshot,&keywords,&input,&rules.to_string(),&EngineIdentityInput::LegacyJp).unwrap();
        tree["cards"] = Value::Object(programs);
        Companions { selected, programs: lowerer().lower(&tree).unwrap() }
    })
}
fn companion_game(initial: &Value) -> Game {
    let fixture = companions();
    let mut game = Game::from_selected_effects(
        fixture.selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "rescan047",
    )
    .unwrap();
    for (card, program) in &fixture.programs {
        game.load_internal_card(card, program).unwrap();
    }
    game
}
fn evolve(game: &mut Game, pp: u8) {
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp}}),
        )
        .unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "the normal evolution rule succeeds"
    );
    assert!(
        step.events
            .iter()
            .any(|event| event["kind"] == "進化" && event["with"] == "form"),
        "evolution consumes the companion resource"
    );
}

fn play(game: &mut Game) {
    assert!(
        matches!(
            game.play_internal_card("source", actor()).unwrap(),
            CardPlay::Entered(_)
        ),
        "the real follower enters for its printed cost"
    );
}
fn target_spell(game: &mut Game) {
    let CardPlay::Awaiting(q) = game.play_internal_card("source", actor()).unwrap() else {
        panic!("target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("targets")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "only the opposing follower is eligible"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
}
fn enemy(initial: &mut Value) {
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
}
#[test]
fn sd01_003_real_base_requires_one_pp() {
    let mut initial = position("SD01-003", "field", 0);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"rescan047-keeper"}]);
    let mut denied = companion_game(&initial);
    if let Ok(step) = denied.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":0_u8}}),
    ) {
        assert_ne!(step.outcome, "resolved");
    }
    assert_eq!(
        denied.projection(View::Referee).unwrap()["P1"]["evolve_deck"],
        json!(["form"])
    );
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    let mut game = companion_game(&initial);
    evolve(&mut game, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["objects"]["source"]["evolved"], true);
}
#[test]
fn sd01_016_damages_the_enemy_and_creates_one_fairy_in_ex() {
    let mut initial = position("SD01-016", "hand", 2);
    enemy(&mut initial);
    let mut game = game(&initial);
    target_spell(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
    tokens(&state, "ex", "BP01-T03", 1);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P2"]["ex"], json!([]));
}
#[test]
fn sd01_011_lastword_creates_one_real_fairy_in_ex() {
    let initial = position("SD01-011", "field", 0);
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: subjects(),
        },
    );
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game, "SD01-011");
    let state = game.projection(View::Referee).unwrap();
    tokens(&state, "ex", "BP01-T03", 1);
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert_eq!(state["P2"]["ex"], json!([]));
}
#[test]
fn sd02_013_real_storm_form_attacks_the_leader_on_entry_turn() {
    let mut initial = position("PR-005", "field", 3);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"PR-005","state":{"entered_this_turn":true}}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"SD02-013"}]);
    let mut game = game(&initial);
    evolve(&mut game, 3);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    pass(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
    assert_eq!(state["objects"]["source"]["keywords"], json!(["疾走"]));
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
}
#[test]
fn sd02_010_fanfare_creates_one_knight_in_field() {
    let mut game = game(&position("SD02-010", "hand", 2));
    play(&mut game);
    trigger(&mut game, "SD02-010");
    let state = game.projection(View::Referee).unwrap();
    tokens(&state, "field", "BP17-T11", 1);
    tokens(&state, "ex", "BP17-T11", 0);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["P2"]["field"], json!([]));
}
#[test]
fn sd03_015_damages_only_an_enemy_and_draws_one() {
    let mut initial = position("SD03-015", "hand", 3);
    enemy(&mut initial);
    let mut game = game(&initial);
    target_spell(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    assert_eq!(state["P1"]["deck"], json!(["second"]));
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}
fn mage_position(pp: u8) -> Value {
    let mut initial = position("SD03-003", "field", pp);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"SD03-004"}]);
    initial
}
#[test]
fn sd03_003_real_base_pays_exactly_one_pp_for_its_real_form() {
    let mut denied = game(&mage_position(0));
    assert_eq!(
        denied
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
        "cannot-evolve"
    );
    assert_eq!(
        denied.projection(View::Referee).unwrap()["P1"]["evolve_deck"],
        json!(["form"])
    );
    let mut game = game(&mage_position(1));
    evolve(&mut game, 1);
    trigger(&mut game, "SD03-004");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["objects"]["source"]["evolved"], true);
}
#[test]
fn sd03_004_real_evolution_damages_every_enemy_follower_only() {
    let mut initial = mage_position(1);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"SD03-003"},{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy-a","card":"BP01-037"},{"id":"enemy-b","card":"BP01-037"}]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    trigger(&mut game, "SD03-004");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy-a"]["damage"], 2_u8);
    assert_eq!(state["objects"]["enemy-b"]["damage"], 2_u8);
    assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
    assert_eq!(state["objects"]["source"]["damage"], 0_u8);
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
}
#[test]
fn sd04_001_fanfare_destroys_all_enemy_five_health_followers_only() {
    let mut initial = position("SD04-001", "hand", 8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy-a","card":"BP01-037"},{"id":"enemy-b","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game, "SD04-001");
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy-a", "enemy-b"]));
    assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
    assert_eq!(state["objects"]["source"]["damage"], 0_u8);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
}
fn bird_position() -> Value {
    let mut initial = position("SD04-009", "field", 0);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-T03","state":{"acted":true}}]);
    initial
}
// Returns the state after the attack-time buff and before combat damage.
fn attack_bird(game: &mut Game, card: &str) -> Value {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "the normal attack starts"
    );
    trigger(game, card);
    game.resume_internal_card().unwrap();
    let before = game.projection(View::Referee).unwrap();
    pass(game);
    before
}
#[test]
fn sd04_009_attack_increases_only_its_power() {
    let mut game = game(&bird_position());
    let before = attack_bird(&mut game, "SD04-009");
    assert_eq!(before["objects"]["source"]["power"], 4_u8);
    assert_eq!(before["objects"]["source"]["hp"], 3_u8);
    assert_eq!(before["objects"]["source"]["damage"], 0_u8);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert_eq!(state["objects"]["source"]["max_hp"], 3_u8);
    assert_eq!(state["objects"]["source"]["hp"], 2_u8);
    assert_eq!(state["objects"]["source"]["damage"], 1_u8);
    assert_eq!(state["P2"]["field"], json!([]));
}
#[test]
fn sd04_010_free_real_evolution_attacks_and_increases_power_and_health() {
    let mut initial = bird_position();
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"SD04-010"}]);
    let mut game = game(&initial);
    evolve(&mut game, 0);
    let before = attack_bird(&mut game, "SD04-010");
    assert_eq!(before["objects"]["source"]["power"], 4_u8);
    assert_eq!(before["objects"]["source"]["hp"], 4_u8);
    assert_eq!(before["objects"]["source"]["damage"], 0_u8);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert_eq!(state["objects"]["source"]["max_hp"], 4_u8);
    assert_eq!(state["objects"]["source"]["hp"], 3_u8);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["objects"]["source"]["damage"], 1_u8);
}
#[test]
fn sd04_012_fanfare_creates_a_dragon_only_at_seven_maximum_pp() {
    for maximum in [6_u8, 7_u8] {
        let mut initial = position("SD04-012", "hand", 2);
        initial["players"]["P1"]["pp"]["max"] = json!(maximum);
        let mut game = game(&initial);
        play(&mut game);
        trigger(&mut game, "SD04-012");
        let state = game.projection(View::Referee).unwrap();
        let ex = state["P1"]["ex"].as_array().unwrap();
        assert_eq!(ex.len(), usize::from(maximum == 7));
        if let Some(id) = ex.first() {
            let token = &state["objects"][id.as_str().unwrap()];
            assert_eq!(token["card"], "BP01-T11");
            assert_eq!(token["power"], 5_u8);
            assert_eq!(token["hp"], 5_u8);
        }
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        assert_eq!(state["P2"]["ex"], json!([]));
    }
}
#[test]
fn sd06_012_evolution_heals_and_retains_ward() {
    let mut initial = position("rescan047-sister", "field", 1);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"SD06-012"}]);
    let mut game = companion_game(&initial);
    evolve(&mut game, 1);
    trigger(&mut game, "SD06-012");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(state["objects"]["source"]["keywords"], json!(["守護"]));
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert_eq!(state["objects"]["source"]["hp"], 4_u8);
}

fn targeted_trigger(game: &mut Game, card: &str) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("formal trigger")
    };
    assert_eq!(options.len(), 1_usize, "one printed Fanfare is pending");
    assert_eq!(options[0].card, card, "the real card supplies Fanfare");
    game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}
fn resolve_one_target(game: &mut Game, play: CardPlay) {
    let CardPlay::Awaiting(q) = play else {
        panic!("formal target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("formal targets")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "only the printed recipient is eligible"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
}
#[test]
fn sd02_009_fanfare_buffs_only_another_own_follower() {
    for available in [false, true] {
        let mut initial = position("SD02-009", "hand", 3);
        initial["players"]["P1"]["zones"]["field"] = if available {
            json!([{"id":"ally","card":"BP01-037"}])
        } else {
            json!([])
        };
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        play(&mut game);
        let play = targeted_trigger(&mut game, "SD02-009");
        if available {
            resolve_one_target(&mut game, play);
        } else {
            assert_eq!(play, CardPlay::Cancelled);
        }
        let state = game.projection(View::Referee).unwrap();
        if available {
            assert_eq!(state["objects"]["ally"]["power"], 6_u8);
            assert_eq!(state["objects"]["ally"]["hp"], 6_u8);
        }
        assert_eq!(state["objects"]["source"]["power"], 3_u8);
        assert_eq!(state["objects"]["source"]["hp"], 3_u8);
        assert_eq!(state["objects"]["enemy"]["power"], 5_u8);
        assert_eq!(state["objects"]["enemy"]["hp"], 5_u8);
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    }
}
#[test]
fn sd04_011_fanfare_damages_only_a_required_opposing_follower() {
    for available in [false, true] {
        let mut initial = position("SD04-011", "hand", 4);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = if available {
            json!([{"id":"enemy","card":"BP01-037"}])
        } else {
            json!([])
        };
        let mut game = game(&initial);
        play(&mut game);
        let play = targeted_trigger(&mut game, "SD04-011");
        if available {
            resolve_one_target(&mut game, play);
        } else {
            assert_eq!(play, CardPlay::Cancelled);
        }
        let state = game.projection(View::Referee).unwrap();
        if available {
            assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
        }
        assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
        assert_eq!(state["objects"]["source"]["damage"], 0_u8);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    }
}

#[test]
fn sd01_005_same_line_fanfare_and_lastword_each_create_one_fairy_in_ex() {
    let mut initial = position("SD01-005", "hand", 3);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: subjects(),
        },
    );
    play(&mut game);
    trigger(&mut game, "SD01-005");
    game.resume_internal_card().unwrap();
    let entered = game.projection(View::Referee).unwrap();
    tokens(&entered, "ex", "BP01-T03", 1);
    assert_eq!(entered["P1"]["pp"]["current"], 0_u8);
    assert!(game.internal_pending_choice().unwrap().is_none());
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game, "SD01-005");
    game.resume_internal_card().unwrap();
    let departed = game.projection(View::Referee).unwrap();
    tokens(&departed, "ex", "BP01-T03", 2);
    assert!(
        departed["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert_eq!(departed["P2"]["ex"], json!([]));
    assert!(game.internal_pending_choice().unwrap().is_none());
}
