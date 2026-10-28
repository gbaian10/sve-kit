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
            ("rescan046-gildaria", "PR-559", true),
            ("rescan046-water", "PR-031", true),
            ("rescan046-floral", "PR-089", false),
            ("rescan046-priest", "PR-093", false),
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
        input["documents"].as_array_mut().unwrap().push(json!(["rescan046-companions.yaml", json!({"format":1_u8,"kind":"effect_set","cards":cards}).to_string()]));
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
        "rescan046",
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

#[test]
fn pr_559_evolution_damages_every_enemy_follower_and_leader_only() {
    let mut initial = position("rescan046-gildaria", "field", 1);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"PR-559"}]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"rescan046-gildaria"},{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy-a","card":"BP01-037"},{"id":"enemy-b","card":"BP01-037"}]);
    let mut game = companion_game(&initial);
    evolve(&mut game, 1);
    trigger(&mut game, "PR-559");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy-a"]["damage"], 4_u8);
    assert_eq!(state["objects"]["enemy-b"]["damage"], 4_u8);
    assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}

fn item(card: &str) {
    let mut game = game(&position(card, "ex", 4));
    resolved(game.play_internal_card("source", actor()).unwrap());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["leader"]["life"], 21_u8,
        "the spell token heals its controller"
    );
    assert_eq!(
        state["P2"]["leader"]["life"], 20_u8,
        "the opponent is unaffected"
    );
    assert_eq!(
        state["P1"]["hand"],
        json!(["first"]),
        "exactly the top card is drawn"
    );
    assert_eq!(
        state["P1"]["deck"],
        json!(["second"]),
        "the remaining deck order is preserved"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 0_u8,
        "the printed four PP is paid"
    );
    assert_eq!(state["P1"]["ex"], json!([]), "the token leaves EX");
    assert_eq!(
        state["P1"]["cemetery"],
        json!([]),
        "the spent spell token ceases to exist"
    );
}
#[test]
fn pr_155_spell_token_pays_heals_and_draws() {
    item("PR-155");
}
#[test]
fn pr_156_spell_token_pays_heals_and_draws() {
    item("PR-156");
}

#[test]
fn pr_011_real_lilim_pair_attacks_and_heals_for_both_printings() {
    for (base, form) in [("PR-092", "PR-011"), ("SD05-012", "SD05-013")] {
        let mut initial = position(base, "field", 1);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        let mut game = game(&initial);
        evolve(&mut game, 1);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        trigger(&mut game, form);
        game.resume_internal_card().unwrap();
        pass(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 22_u8);
        assert_eq!(state["P2"]["leader"]["life"], 17_u8);
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    }
}

#[test]
fn pr_014_banishes_only_an_enemy_and_heals_for_both_printings() {
    for card in ["PR-014", "SD06-005"] {
        let mut initial = position(card, "hand", 4);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        let CardPlay::Awaiting(q) = game.play_internal_card("source", actor()).unwrap() else {
            panic!("target")
        };
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("targets")
        };
        assert_eq!(domain.members.len(), 1_usize);
        resolved(
            game.answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["banish"], json!(["enemy"]));
        assert_eq!(state["P2"]["cemetery"], json!([]));
        assert_eq!(state["P1"]["field"], json!(["ally"]));
        assert_eq!(state["P1"]["leader"]["life"], 22_u8);
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    }
}

fn evolution_fee(card: &str, form: &str) {
    let mut initial = position(card, "field", 0);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut denied = companion_game(&initial);
    if let Ok(step) = denied.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":0_u8}}),
    ) {
        assert_ne!(
            step.outcome, "resolved",
            "the printed PP cost cannot be bypassed"
        );
    }
    assert_eq!(
        denied.projection(View::Referee).unwrap()["P1"]["evolve_deck"],
        json!(["form"]),
        "rejected evolution preserves its resource"
    );
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    let mut game = companion_game(&initial);
    evolve(&mut game, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 0_u8,
        "the real base supplies its one-PP evolution cost"
    );
    assert_eq!(
        state["P1"]["evolve_deck"],
        json!([]),
        "the resource leaves the evolution deck"
    );
    assert_eq!(
        state["objects"]["source"]["evolved"], true,
        "the real base evolves"
    );
}
#[test]
fn pr_089_real_base_requires_one_pp_for_both_printings() {
    for card in ["PR-089", "SD02-003"] {
        evolution_fee(card, "rescan046-floral");
    }
}
#[test]
fn pr_093_real_base_requires_one_pp_for_both_printings() {
    for card in ["PR-093", "SD06-003"] {
        evolution_fee(card, "rescan046-priest");
    }
}

#[test]
fn pr_028_spell_creates_a_bat_in_field_and_ex_for_both_printings() {
    for card in ["PR-028", "SD05-016"] {
        let mut game = game(&position(card, "hand", 1));
        resolved(game.play_internal_card("source", actor()).unwrap());
        let state = game.projection(View::Referee).unwrap();
        tokens(&state, "field", "BP01-T15", 1);
        tokens(&state, "ex", "BP01-T15", 1);
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        assert_eq!(state["P1"]["cemetery"], json!(["source"]));
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(state["P2"]["ex"], json!([]));
    }
}

#[test]
fn pr_031_evolution_creates_a_real_fairy_for_both_printings() {
    for form in ["PR-031", "SD01-012"] {
        let mut initial = position("rescan046-water", "field", 1);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        let mut game = companion_game(&initial);
        evolve(&mut game, 1);
        trigger(&mut game, form);
        let state = game.projection(View::Referee).unwrap();
        tokens(&state, "field", "BP01-T03", 1);
        tokens(&state, "ex", "BP01-T03", 0);
        assert_eq!(state["P2"]["field"], json!([]));
    }
}

#[test]
fn pr_092_real_fanfare_creates_one_bat_in_ex_for_both_printings() {
    for card in ["PR-092", "SD05-012"] {
        let mut game = game(&position(card, "hand", 2));
        assert!(matches!(
            game.play_internal_card("source", actor()).unwrap(),
            CardPlay::Entered(_)
        ));
        trigger(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        tokens(&state, "ex", "BP01-T15", 1);
        tokens(&state, "field", "BP01-T15", 0);
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        assert_eq!(state["P2"]["ex"], json!([]));
    }
}

#[test]
fn pr_155_and_156_carry_the_official_magic_item_battle_name_only() {
    for card in ["PR-155", "PR-156"] {
        let mut initial = position(card, "ex", 4);
        initial["players"]["P1"]["zones"]["ex"] =
            json!([{"id":"source","card":card},{"id":"repair","card":"BP07-T02"}]);
        let game = game(&initial);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["name"], "魔法のアイテム",
            "the battle name comes from the official ruling, not the printed name"
        );
        assert_eq!(
            state["objects"]["repair"]["name"], "リペアモード",
            "a different real token keeps its own battle name"
        );
    }
}
