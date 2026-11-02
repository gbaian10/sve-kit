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

fn finish(game: &mut Game, play: CardPlay, accepted: bool, leader: bool) {
    finish_with_target(game, play, accepted, leader, None);
}

fn finish_with_target(
    game: &mut Game,
    mut play: CardPlay,
    accepted: bool,
    leader: bool,
    expected: Option<&str>,
) {
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
                        .find(|m| {
                            expected.map_or_else(
                                || m.card.is_none() == leader,
                                |card| m.card.as_deref() == Some(card),
                            )
                        })
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

// These external bases isolate real evolved cards whose own bases still need other blocks.
fn evolution_fixture() -> &'static EvolutionFixture {
    static FIXTURE: OnceLock<EvolutionFixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (snapshot, keywords, mut input) = inputs().clone();
        let mut snapshot = snapshot.trim_end().to_owned();
        let mut cards = Map::new();
        let mut tree = expanded::tree().clone();
        let mut programs = Map::new();
        for form in ["BP12-047", "BP16-047"] {
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

fn evolved(form: &str, stacks: u8) -> Game {
    let base = format!("earth-base-{form}");
    let mut initial = position(&base, true, stacks);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
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
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the permitted evolution resolves"
    );
    finish(&mut game, CardPlay::Cancelled, true, false);
    game
}

#[test]
fn bp02_043_all_printings_buff_only_the_created_attack_golem_after_real_evolution() {
    for form in ["BP02-043", "BP02-P13"] {
        for accepted in [false, true] {
            let mut initial = position("BP02-042", true, 1);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
            let mut game = game(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved",
                "the permitted evolution resolves"
            );
            finish(&mut game, CardPlay::Cancelled, accepted, false);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(count(&view, "field", "攻撃型ゴーレム"), 1);
            let golem = view["P1"]["field"]
                .as_array()
                .unwrap()
                .iter()
                .find(|id| view["objects"][id.as_str().unwrap()]["name"] == "攻撃型ゴーレム")
                .unwrap();
            assert_eq!(
                view["objects"][golem.as_str().unwrap()]["power"],
                if accepted { 5_u8 } else { 3 }
            );
            assert_eq!(view["objects"]["source"]["power"], 4_u8);
        }
    }
}

#[test]
fn bp02_048_lastword_adds_two_to_an_existing_stack() {
    let mut initial = position("BP02-048", true, 3);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: subjects(),
        },
    );
    activate(&mut game, "P1");
    finish(&mut game, CardPlay::Cancelled, true, false);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["objects"]["soil"]["counters"]["スタックカウンター"], 4_u8,
        "destruction first replaces one Stack, then Lastword adds two"
    );
    assert!(
        view["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
}

#[test]
fn bp03_041_fanfare_storm_and_spellchain_are_independent() {
    for card in ["BP03-041", "BP03-P11", "PR-062"] {
        for accepted in [false, true] {
            let mut initial = position(card, false, 1);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0_u8..7_u8)
                    .map(|i| json!({"id":format!("spell{i}"),"card":"BP01-065"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            play(&mut game, accepted, false);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走")),
                accepted
            );
            assert_eq!(view["objects"]["enemy"]["damage"], 4_u8);
        }
    }
}

fn resurrection(base: &str, form: Option<&str>) {
    for accepted in [false, true] {
        let mut initial = position(base, true, 2);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        if let Some(number) = form {
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        }
        let mut game = with_producer(
            &initial,
            Action::Destroy {
                objects: subjects(),
            },
        );
        if form.is_some() {
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
                )
                .unwrap()
                .outcome,
                "resolved",
                "the permitted evolution resolves"
            );
        }
        activate(&mut game, "P1");
        finish(&mut game, CardPlay::Cancelled, accepted, false);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            accepted,
            "Earth payment decides whether the departed source returns"
        );
        if accepted {
            assert_eq!(
                view["objects"]["source"]["power"], 2_u8,
                "the base returns without the evolved face"
            );
            assert!(
                view["objects"]["source"]["evolved_with"].is_null(),
                "returning starts a fresh base generation"
            );
        }
        if form.is_some() {
            assert_eq!(
                view["P1"]["evolve_deck"],
                json!(["form"]),
                "the detached form returns to the evolve deck"
            );
        }
    }
}

#[test]
fn bp04_047_all_printings_pay_earth_to_resurrect_the_base() {
    for card in ["BP04-047", "BP04-P13"] {
        resurrection(card, None);
    }
}

#[test]
fn bp04_048_all_printings_return_the_base_after_evolved_lastword() {
    for form in ["BP04-048", "BP04-P14"] {
        resurrection("BP04-047", Some(form));
    }
}

#[test]
fn bp06_041_fanfare_and_lastword_pay_separately() {
    for card in ["BP06-041", "PR-138"] {
        let initial = position(card, false, 3);
        let mut game = with_producer(
            &initial,
            Action::Destroy {
                objects: subjects(),
            },
        );
        play(&mut game, true, false);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            5_u8
        );
        activate(&mut game, "P1");
        finish(&mut game, CardPlay::Cancelled, true, false);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["leader"]["life"], 17_u8);
        assert!(
            view["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("soil"))
        );
    }
}

#[test]
fn bp16_055_creates_a_guardian_only_when_earth_is_paid() {
    for accepted in [false, true] {
        let mut game = game(&position("BP16-055", false, 1));
        play(&mut game, accepted, false);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            count(&view, "field", "ガーディアンゴーレム"),
            usize::from(accepted)
        );
        assert_eq!(view["P1"]["pp"]["current"], 7_u8);
        assert_eq!(
            view["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("soil")),
            accepted
        );
    }
}

#[test]
fn bp22_040_all_printings_damage_the_target_only_after_earth_payment() {
    for card in ["BP22-040", "BP22-SL12"] {
        for accepted in [false, true] {
            let mut game = game(&position(card, false, 1));
            play(&mut game, accepted, false);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if accepted { 5_u8 } else { 0 }
            );
        }
    }
}

#[test]
fn bp10_053_all_printings_draw_only_after_paying_two_stack_from_one_amulet() {
    for card in ["BP10-053", "PR-298"] {
        for accepted in [false, true] {
            let mut game = game(&position(card, false, 2));
            play(&mut game, accepted, false);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["objects"]["enemy"]["damage"],
                if accepted { 4_u8 } else { 2 }
            );
            assert_eq!(
                view["P1"]["hand"],
                if accepted {
                    json!(["first"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("soil")),
                accepted
            );
        }
    }
}

#[test]
fn bp12_047_earth_buffs_only_another_golem_and_is_once_per_turn() {
    let mut game = evolved("BP12-047", 2);
    let entry = game.projection(View::Referee).unwrap();
    assert_eq!(count(&entry, "field", "防御型ゴーレム"), 1);
    assert_eq!(count(&entry, "field", "攻撃型ゴーレム"), 1);
    let defender = entry["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .find(|id| entry["objects"][id.as_str().unwrap()]["name"] == "防御型ゴーレム")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let action = activate_printed(&mut game, "BP12-047");
    finish_with_target(&mut game, action, true, false, Some("BP01-T09"));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"][&defender]["power"], 4_u8);
    assert_eq!(view["objects"][&defender]["hp"], 5_u8);
    assert_eq!(view["objects"]["source"]["power"], 6_u8);
    assert_eq!(view["objects"]["source"]["acted"], false);
    let before = view;
    game.activate_internal_card("source", printed_id("BP12-047"), actor())
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}

#[test]
fn bp14_045_all_printings_damage_then_increase_the_existing_stack() {
    for card in ["BP14-045", "PR-372"] {
        let mut game = game(&position(card, false, 1));
        play(&mut game, true, false);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(
            view["objects"]["soil"]["counters"]["スタックカウンター"],
            2_u8
        );
    }
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
fn bp21_045_all_printings_increase_stack_then_pay_two_to_damage_opposing_followers() {
    for card in ["BP21-045", "BP21-P22"] {
        let mut game = game(&position(card, false, 1));
        play(&mut game, true, false);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["soil"]["counters"]["スタックカウンター"],
            2_u8
        );
        let action = activate_printed(&mut game, card);
        finish(&mut game, action, true, false);
        field_damage(&game.projection(View::Referee).unwrap());
    }
}

#[test]
fn bp15_043_all_printings_offer_the_opposing_leader_or_follower() {
    for card in ["BP15-043", "PR-396"] {
        for leader in [false, true] {
            let mut game = game(&position(card, false, 1));
            play(&mut game, true, leader);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["objects"]["enemy"]["damage"],
                if leader { 0_u8 } else { 2 }
            );
            assert_eq!(
                view["P2"]["leader"]["life"],
                if leader { 18_u8 } else { 20 }
            );
        }
    }
}

#[test]
fn bp16_038_all_printings_pay_earth_for_assail_without_acting_the_source() {
    for card in ["BP16-038", "BP16-SL10", "PCS02-001"] {
        let mut game = game(&position(card, false, 1));
        play(&mut game, true, false);
        let action = activate_printed(&mut game, card);
        finish(&mut game, action, true, false);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], 2_u8);
        assert!(
            view["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("指定攻撃"))
        );
        assert_eq!(view["objects"]["source"]["acted"], false);
    }
}

#[test]
fn bp16_047_evolution_trigger_heals_then_increases_stack() {
    let game = evolved("BP16-047", 1);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["leader"]["life"], 12_u8);
    assert_eq!(
        view["objects"]["soil"]["counters"]["スタックカウンター"],
        2_u8
    );
}
