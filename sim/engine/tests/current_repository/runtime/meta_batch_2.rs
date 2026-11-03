#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn in_hand(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},{"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"},{"id":"fourth","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn on_field(card: &str) -> Value {
    let mut initial = in_hand(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn push(initial: &mut Value, player: &str, zone: &str, object: Value) {
    let slot = &mut initial["players"][player]["zones"][zone];
    if slot.is_null() {
        *slot = json!([]);
    }
    slot.as_array_mut().unwrap().push(object);
}

fn fill(initial: &mut Value, zone: &str, card: &str, count: usize) {
    let start = initial["players"]["P1"]["zones"][zone]
        .as_array()
        .map_or(0, Vec::len);
    for i in (start..).take(count) {
        push(initial, "P1", zone, json!({"id":format!("{zone}{i}"),"card":card}));
    }
}

fn take(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |most| usize::try_from(most.0).unwrap())
        .min(available)
}

fn finish_with(game: &mut Game, mut play: CardPlay, modes: &[u64]) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(modes.iter().copied().map(ModeId).collect()),
            InputDomain::Integer(bounds) => Answer::Integer(bounds.maximum.unwrap_or(bounds.minimum)),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(take(&domain.cardinality, domain.cards.len()))
                    .map(|card| card.handle)
                    .collect(),
            )),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().map(|card| card.handle).collect())
            }
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()))
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()))
                    .map(|member| member.handle)
                    .collect(),
            ),
            other => panic!("unexpected decision: {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap_or_else(|e| panic!("{:?}: {e:?}", question.domain));
    }
}

fn triggers_with(game: &mut Game, modes: &[u64]) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
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
        finish_with(game, step, modes);
    }
}

fn triggers(game: &mut Game) {
    triggers_with(game, &[1]);
}

fn play_with(game: &mut Game, id: &str, modes: &[u64]) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish_with(game, step, modes);
    triggers_with(game, modes);
}

fn play(game: &mut Game, id: &str) {
    play_with(game, id, &[1]);
}

fn evolve(game: &mut Game, pp: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the base pays its printed evolution cost"
    );
    triggers(game);
}

fn activation(card: &str, index: usize) -> AbilityId {
    selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .filter(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .nth(index)
        .unwrap()
        .node
        .id
}

fn activate(game: &mut Game, card: &str, index: usize) -> Result<(), EngineFailure> {
    let step = game.activate_internal_card("source", activation(card, index), actor())?;
    finish_with(game, step, &[1]);
    triggers(game);
    Ok(())
}

fn attack(game: &mut Game, attacker: &str, target: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the attack is declared"
    );
    loop {
        triggers(game);
        let Some(question) = game.internal_quick_choice().unwrap() else {
            break;
        };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish_with(game, step, &[1]);
    }
}

fn named(state: &Value, zone: &Value, name: &str) -> Vec<String> {
    zone.as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .filter(|id| state["objects"][id]["name"] == name)
        .collect()
}

fn has_keyword(state: &Value, id: &str, keyword: &str) -> bool {
    state["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}

fn end_phase(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the active player enters its end phase"
    );
    triggers(game);
}

fn acted_enemy() -> Value {
    json!({"id":"enemy","card":"BP01-037","state":{"acted":true,"hp":20_u8,"max_hp":20_u8}})
}

fn soil() -> Value {
    json!({"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}})
}

#[test]
fn bp10_114_sends_a_cemetery_angel_and_an_enemy_follower_to_their_ex_areas() {
    let mut initial = in_hand("BP10-114");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"angel","card":"BP05-110"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["angel"]));
    assert_eq!(state["P2"]["ex"], json!(["enemy"]));
}

#[test]
fn bp07_003_gives_fairy_tokens_storm_assail_and_power() {
    let mut initial = in_hand("BP07-003");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fairy","card":"BP01-T03"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["fairy"]["power"], 3_u8);
    assert!(has_keyword(&state, "fairy", "疾走") && has_keyword(&state, "fairy", "指定攻撃"));
    let wisps = named(&state, &state["P1"]["ex"], "フェアリーウィスプ");
    assert_eq!(wisps.len(), 1);
    play(&mut game, &wisps[0]);
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["objects"][wisps[0].as_str()]["power"], 3_u8, "1 + 2 on entry");
}

#[test]
fn ebd01_013_buffs_fairy_tokens_and_grants_assail() {
    let mut initial = in_hand("EBD01-013");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fairy","card":"BP01-T03"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["fairy"]["power"], 2_u8);
    assert_eq!(state["objects"]["fairy"]["hp"], 2_u8);
    assert!(has_keyword(&state, "fairy", "指定攻撃"));
}

#[test]
fn bp11_042_earth_rite_grants_the_chosen_keyword() {
    let mut initial = on_field("BP11-042");
    push(&mut initial, "P1", "field", json!({"id":"ally","card":"BP01-171"}));
    push(&mut initial, "P1", "field", soil());
    let mut game = game(&initial);
    activate(&mut game, "BP11-042", 0).unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert!(has_keyword(&state, "ally", "突進"));
    assert!(state["P1"]["cemetery"].as_array().unwrap().contains(&json!("soil")));
}

#[test]
fn bp11_043_evolution_puts_an_enemy_follower_in_its_ex_area() {
    let mut initial = on_field("BP11-042");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP11-043"}]);
    let mut game = game(&initial);
    evolve(&mut game, 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["ex"], json!(["enemy"]));
}

#[test]
fn bp11_082_lastword_places_a_bike_and_mills_one() {
    let mut initial = on_field("BP11-082");
    initial["players"]["P2"]["zones"]["field"] = json!([acted_enemy()]);
    let mut game = game(&initial);
    attack(&mut game, "source", "enemy");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "機動二輪車").len(), 1);
    assert!(state["P1"]["cemetery"].as_array().unwrap().contains(&json!("first")));
}

#[test]
fn bp06_046_seven_onmyoji_cards_or_spells_summon_a_warded_katashiro() {
    let mut initial = in_hand("BP06-046");
    fill(&mut initial, "cemetery", "BP06-044", 4);
    fill(&mut initial, "cemetery", "BP01-114", 3);
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    let dolls = named(&state, &state["P1"]["field"], "形代");
    assert_eq!(dolls.len(), 1);
    assert!(has_keyword(&state, &dolls[0], "守護"), "a shikigami entering gains ward");
    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
}

#[test]
fn bp16_085_second_mode_places_two_forest_bats() {
    let mut game = game(&in_hand("BP16-085"));
    play_with(&mut game, "source", &[2]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["ex"], "フォレストバット").len(), 2);
}

#[test]
fn bp20_015_discards_a_matching_card_to_draw_two() {
    let mut initial = in_hand("BP20-015");
    push(&mut initial, "P1", "hand", json!({"id":"fuel","card":"BP20-015"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["fuel"]));
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 2);
}

#[test]
fn bp21_037_places_pain_with_ten_academy_cards_in_the_cemetery() {
    let mut initial = in_hand("BP21-037");
    fill(&mut initial, "cemetery", "BP21-037", 10);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["ex"], "痛苦").len(), 1);
}

#[test]
fn bp16_019_end_phase_may_place_a_steel_knight_in_ex() {
    let mut game = game(&on_field("BP16-019"));
    end_phase(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["ex"], "スティールナイト").len(), 1);
}

#[test]
fn bp22_113_buffs_other_goblins_on_entry() {
    let mut initial = on_field("BP22-113");
    push(&mut initial, "P1", "hand", json!({"id":"goblin","card":"BP01-171"}));
    let mut game = game(&initial);
    play(&mut game, "goblin");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["goblin"]["power"], 3_u8);
}

#[test]
fn bp22_114_evolution_discounts_a_cheap_goblin_from_the_top_four() {
    let mut initial = on_field("BP22-113");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-114"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP22-113"},{"id":"a","card":"BP01-037"},
        {"id":"b","card":"BP01-037"},{"id":"c","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    let looked = game.projection(View::Referee).unwrap();
    assert_eq!(looked["P1"]["ex"], json!(["goblin"]));
    play(&mut game, "goblin");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 9_u8, "the 2-cost goblin costs 0");
}

#[test]
fn bp05_100_second_mode_banishes_a_low_health_enemy() {
    let mut initial = in_hand("BP05-100");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut game = game(&initial);
    play_with(&mut game, "source", &[2]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["cemetery"], json!([]), "banished, not destroyed");
    assert_eq!(state["P2"]["banish"], json!(["enemy"]));
}

#[test]
fn bp06_061_and_062_awakened_cost_one_and_destroy_an_acted_enemy() {
    for card in ["BP06-061", "BP06-062"] {
        let mut initial = in_hand(card);
        initial["players"]["P2"]["zones"]["field"] = json!([acted_enemy()]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]), "{card}");
        assert_eq!(state["P1"]["pp"]["current"], 9_u8, "{card} costs 2 - 1");
        assert_eq!(state["P2"]["leader"]["life"], 20_u8, "{card}: no Filrain on the field");
    }
}

#[test]
fn bp05_044_chooses_two_modes_with_two_wizards() {
    let mut initial = in_hand("BP05-044");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"w1","card":"BP22-044"},{"id":"w2","card":"BP22-044"}
    ]);
    let mut game = game(&initial);
    play_with(&mut game, "source", &[2, 3]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
}

#[test]
fn cp01_059_gains_two_power_with_five_uma_musume_in_the_cemetery() {
    let mut initial = in_hand("CP01-059");
    fill(&mut initial, "cemetery", "BP22-R20", 5);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "meta-batch-2",
    )
    .unwrap();
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert!(has_keyword(&state, "source", "突進"));
}
