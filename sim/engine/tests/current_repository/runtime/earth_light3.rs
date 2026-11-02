#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed Earth Rite scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use serde_json::Map;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, field: bool, stacks: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"][if field { "field" } else { "hand" }] =
        json!([{"id":"source","card":card}]);
    let own = initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap();
    if stacks > 0 {
        own.push(
            json!({"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":stacks}}}),
        );
    }
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-097"}]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "earth-light3",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay, accepted: bool, leader: bool) {
    loop {
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::OptionalCost => Answer::OptionalCost(accepted),
                InputDomain::CostMaterials(domain) => {
                    assert!(accepted, "declining Earth Rite never selects materials");
                    assert_eq!(domain.members.len(), 1, "only the own field Stack can pay");
                    Answer::CostMaterials(vec![domain.members[0].handle])
                }
                InputDomain::Targets(domain) => {
                    let target = domain
                        .members
                        .iter()
                        .find(|m| m.card.is_none() == leader)
                        .unwrap();
                    Answer::Targets(vec![target.handle])
                }
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("unexpected printed decision"),
            };
            play = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(pending) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("printed pending ability");
        };
        play = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
    }
}

fn play(game: &mut Game, accepted: bool, leader: bool) {
    let action = game.play_internal_card("source", actor()).unwrap();
    finish(game, action, accepted, leader);
}

fn count(view: &Value, zone: &str, name: &str) -> usize {
    view["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| view["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

fn printed_id(card: &str) -> AbilityId {
    selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .unwrap()
        .node
        .id
}

fn activate_printed(game: &mut Game, card: &str) -> CardPlay {
    game.activate_internal_card("source", printed_id(card), actor())
        .unwrap()
}

struct EvolutionFixture {
    selected: SelectedEffects,
    programs: BTreeMap<String, WellFormedCardProgram>,
}

// External bases keep evolved-card scenarios independent of separate base-card admissions.
fn evolution_fixture() -> &'static EvolutionFixture {
    static FIXTURE: OnceLock<EvolutionFixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (snapshot, keywords, mut input) = inputs().clone();
        let mut snapshot = snapshot.trim_end().to_owned();
        let mut cards = Map::new();
        let mut tree = expanded::tree().clone();
        let mut programs = Map::new();
        for form in ["BP16-043", "BP16-P10", "BP21-046", "BP21-P23", "BP22-041", "BP22-SL13", "BP22-U03"] {
            let record = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap()).find(|record| record["number"] == form).unwrap();
            let base = format!("earth-base-{form}");
            snapshot.push('\n');
            snapshot.push_str(&json!({"number":base,"faces":[{"name":record["faces"][0]["name"],"card_class":record["faces"][0]["card_class"],"card_type":"フォロワー","cost":"1","power":"1","hp":"1","traits":[],"text":null,"sections":[]}]}).to_string());
            cards.insert(base.clone(), json!({"status":"complete","abilities":[]}));
            programs.insert(base, tree["cards"]["synthetic-evolution"].clone());
        }
        input["documents"].as_array_mut().unwrap().push(json!(["earth-bases.yaml",json!({"format":1_u8,"kind":"effect_set","cards":cards}).to_string()]));
        let selected = repository::select(&snapshot, &keywords, &input).unwrap();
        tree["cards"] = Value::Object(programs);
        EvolutionFixture { selected, programs: lowerer().lower(&tree).unwrap() }
    })
}

fn evolved(form: &str, stacks: u8, super_evolution: bool, leader: bool) -> Game {
    let base = format!("earth-base-{form}");
    let mut initial = position(&base, true, stacks);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["sep"] = json!(1_u8);
    let fixture = evolution_fixture();
    let mut game = Game::from_selected_effects(
        fixture.selected.clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "earth-evolution",
    )
    .unwrap();
    game.load_internal_card(&base, &fixture.programs[&base])
        .unwrap();
    let payment = if super_evolution {
        json!({"pp":1_u8,"sep":1_u8})
    } else {
        json!({"pp":1_u8})
    };
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":payment})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the permitted evolution resolves"
    );
    finish(&mut game, CardPlay::Cancelled, true, leader);
    game
}

#[test]
fn bp16_043_all_printings_count_the_created_golem_and_draw_only_on_super_evolution() {
    for form in ["BP16-043", "BP16-P10"] {
        for super_evolution in [false, true] {
            let game = evolved(form, 1, super_evolution, false);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(count(&view, "field", "攻撃型ゴーレム"), 1);
            assert_eq!(count(&view, "ex", "防御型ゴーレム"), 1);
            assert_eq!(
                view["objects"]["soil"]["counters"]["スタックカウンター"], 2_u8,
                "the field golem triggers Stack +1; the EX golem does not"
            );
            assert_eq!(
                view["P1"]["hand"],
                if super_evolution {
                    json!(["first", "second"])
                } else {
                    json!([])
                }
            );
        }
    }
}

#[test]
fn bp16_044_hand_and_field_activations_have_distinct_costs_and_destinations() {
    let mut hand = game(&position("BP16-044", false, 1));
    let action = hand
        .activate_internal_card("source", AbilityId(4), actor())
        .unwrap();
    finish(&mut hand, action, true, false);
    let view = hand.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 8_u8);
    assert_eq!(view["P1"]["cemetery"], json!(["source"]));
    assert_eq!(view["P1"]["hand"], json!(["first"]));
    assert_eq!(
        view["objects"]["soil"]["counters"]["スタックカウンター"],
        3_u8
    );
    assert_eq!(count(&view, "field", "ガーディアンゴーレム"), 0);
    let mut field = game(&position("BP16-044", true, 1));
    let field_action = field
        .activate_internal_card("source", AbilityId(1), actor())
        .unwrap();
    finish(&mut field, field_action, true, false);
    let field_view = field.projection(View::Referee).unwrap();
    assert_eq!(field_view["objects"]["source"]["acted"], true);
    assert_eq!(field_view["P1"]["pp"]["current"], 10_u8);
    assert_eq!(field_view["P1"]["cemetery"], json!(["soil"]));
    assert_eq!(count(&field_view, "field", "ガーディアンゴーレム"), 2);
}

fn field_damage(view: &Value) {
    assert_eq!(
        view["objects"]["source"]["acted"], true,
        "the source pays its act cost"
    );
    assert_eq!(
        view["objects"]["enemy"]["damage"], 1_u8,
        "each opposing follower receives one damage"
    );
    assert_eq!(
        view["P2"]["leader"]["life"], 20_u8,
        "the opposing leader is not a follower"
    );
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["soil"]),
        "two removed Stack immediately clear their holder"
    );
}

#[test]
fn bp21_046_all_printings_evolve_then_use_the_two_stack_activation() {
    for form in ["BP21-046", "BP21-P23"] {
        let mut game = evolved(form, 1, false, false);
        let action = activate_printed(&mut game, form);
        finish(&mut game, action, true, false);
        let view = game.projection(View::Referee).unwrap();
        field_damage(&view);
        assert_eq!(view["objects"]["source"]["power"], 2_u8);
    }
}

#[test]
fn bp22_041_all_printings_damage_a_leader_on_evolution_and_a_follower_by_paid_activation() {
    for form in ["BP22-041", "BP22-SL13", "BP22-U03"] {
        let mut game = evolved(form, 2, false, true);
        let action = activate_printed(&mut game, form);
        finish(&mut game, action, true, false);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["leader"]["life"], 17_u8);
        assert_eq!(view["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(view["P1"]["pp"]["current"], 8_u8);
        assert_eq!(view["objects"]["source"]["acted"], false);
        assert_eq!(
            view["objects"]["soil"]["counters"]["スタックカウンター"], 1_u8,
            "the second activation still has enough Earth resources"
        );
        game.activate_internal_card("source", printed_id(form), actor())
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), view);
    }
}

#[test]
fn bp22_044_all_printings_replace_a_fully_consumed_stack_or_increase_the_survivor() {
    for card in ["BP22-044", "BP22-P22"] {
        for initial_stack in [2_u8, 3] {
            let mut game = game(&position(card, false, initial_stack));
            play(&mut game, true, false);
            let action = activate_printed(&mut game, card);
            finish(&mut game, action, true, false);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["objects"]["source"]["acted"], true);
            if initial_stack == 2 {
                assert_eq!(view["P1"]["cemetery"], json!(["soil"]));
                assert_eq!(count(&view, "field", "大地の魔片"), 1);
                let shard = view["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|id| view["objects"][id.as_str().unwrap()]["name"] == "大地の魔片")
                    .unwrap()
                    .as_str()
                    .unwrap();
                assert_eq!(
                    view["objects"][shard]["counters"]["スタックカウンター"],
                    6_u8
                );
            } else {
                assert_eq!(view["P1"]["cemetery"], json!([]));
                assert_eq!(
                    view["objects"]["soil"]["counters"]["スタックカウンター"],
                    7_u8
                );
                assert_eq!(count(&view, "field", "大地の魔片"), 0);
            }
        }
    }
}
