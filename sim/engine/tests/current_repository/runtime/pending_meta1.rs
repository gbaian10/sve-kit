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
        {"id":"third","card":"BP01-171"}
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

fn cards(prefix: &str, card: &str, count: usize) -> Vec<Value> {
    (0..count)
        .map(|i| json!({"id":format!("{prefix}{i}"),"card":card}))
        .collect()
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
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()).max(1))
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
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
            .unwrap();
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

fn play_with(game: &mut Game, id: &str, modes: &[u64]) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish_with(game, step, modes);
    triggers_with(game, modes);
}

fn play(game: &mut Game) {
    play_with(game, "source", &[1]);
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the base pays its printed evolution cost"
    );
    triggers_with(game, &[1]);
}

/// The `index`-th printed activated ability of the card (0-based).
fn printed_activation(card: &str, index: usize) -> AbilityId {
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

fn activate(game: &mut Game, card: &str, index: usize) {
    let step = game
        .activate_internal_card("source", printed_activation(card, index), actor())
        .unwrap();
    finish_with(game, step, &[1]);
    triggers_with(game, &[1]);
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

fn attack(game: &mut Game, attacker: &str, target: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the attack is declared"
    );
    loop {
        triggers_with(game, &[1]);
        let Some(question) = game.internal_quick_choice().unwrap() else {
            break;
        };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish_with(game, step, &[1]);
    }
}

#[test]
fn bp15_028_evolves_itself_when_the_opponent_has_ten_cards_in_the_cemetery() {
    let mut initial = in_hand("BP15-028");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-029"}]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!(cards("grave", "BP01-171", 10));
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["evolve_deck"], json!([]));
    // The evolved form's evolution ability places a golden dagger.
    assert_eq!(named(&state, &state["P1"]["ex"], "黄金の小剣").len(), 1);
}

#[test]
fn bp15_029_playing_the_golden_dagger_places_golden_boots_once() {
    let mut initial = on_field("BP15-028");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-029"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let evolved = game.projection(View::Referee).unwrap();
    let dagger = named(&evolved, &evolved["P1"]["ex"], "黄金の小剣");
    assert_eq!(dagger.len(), 1);
    play_with(&mut game, &dagger[0], &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(named(&state, &state["P1"]["ex"], "黄金の靴").len(), 1);
}

#[test]
fn bp15_004_evolves_with_three_face_up_forms_and_the_evolution_sweeps() {
    let mut initial = in_hand("BP15-004");
    let mut forms: Vec<Value> = (0..3_u8)
        .map(|i| json!({"id":format!("used{i}"),"card":"BP15-029","face_up":true}))
        .collect();
    forms.push(json!({"id":"form","card":"BP15-005"}));
    initial["players"]["P1"]["zones"]["evolve_deck"] = Value::Array(forms);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"sage","card":"BP01-037"},{"id":"goblin","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["evolve_deck"],
        json!(["used0", "used1", "used2"]),
        "the face-down form was used to evolve"
    );
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
    assert_eq!(state["P1"]["hand"], json!(["goblin"]), "the evolution searches a cheap follower");
}

#[test]
fn bp15_005_ally_evolution_hits_the_opponent_and_heals() {
    let mut initial = on_field("BP15-004");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-005"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"goblin","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["goblin"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
}

#[test]
fn bp14_025_deals_five_and_places_a_coin() {
    let mut game = game(&in_hand("BP14-025"));
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(named(&state, &state["P1"]["ex"], "輝く金貨").len(), 1);
}

#[test]
fn bp14_022_places_a_coin_and_gives_other_revelers_storm() {
    let mut initial = in_hand("BP14-022");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"reveler","card":"BP14-060"},{"id":"goblin","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["ex"], "輝く金貨").len(), 1);
    assert!(has_keyword(&state, "reveler", "疾走"));
    assert!(!has_keyword(&state, "goblin", "疾走"));
    assert!(!has_keyword(&state, "source", "疾走"));
}

#[test]
fn bp15_096_fields_a_cheap_ward_follower_then_destroys_with_three_wards() {
    let mut initial = in_hand("BP15-096");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"warden","card":"BP11-097"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"recruit","card":"BP11-097"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["P1"]["field"], json!(["warden", "source", "recruit"]));
    activate(&mut game, "BP15-096", 0);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}

#[test]
fn bp15_109_gives_a_ward_bishop_storm_with_three_ward_followers() {
    let mut initial = in_hand("BP15-109");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"warden","card":"BP11-097"},{"id":"warden2","card":"BP11-097"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert!(has_keyword(&state, "warden", "疾走"));
    assert!(!has_keyword(&state, "warden2", "疾走"));
}

#[test]
fn bp14_024_rushes_with_five_royal_followers_and_draws_on_death() {
    let mut initial = in_hand("BP14-024");
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cards("royal", "BP01-037", 5));
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    play(&mut game);
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["objects"]["source"]["power"], 2_u8);
    assert!(has_keyword(&entered, "source", "突進"));
    attack(&mut game, "source", "enemy");
    let state = game.projection(View::Referee).unwrap();
    assert!(state["P1"]["cemetery"].as_array().unwrap().contains(&json!("source")));
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}

#[test]
fn bp15_080_destroys_with_ten_two_cost_cards_in_the_cemetery() {
    let mut initial = in_hand("BP15-080");
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cards("two", "BP07-044", 10));
    let mut game = game(&initial);
    play(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}

#[test]
fn bp15_081_evolution_places_a_two_cost_card_in_ex_two_cheaper() {
    let mut initial = on_field("BP15-080");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-081"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"two","card":"BP15-080"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game);
    let placed = game.projection(View::Referee).unwrap();
    assert_eq!(placed["P1"]["ex"], json!(["two"]));
    let before = placed["P1"]["pp"]["current"].clone();
    play_with(&mut game, "two", &[1]);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        before,
        "the two-cost follower is free"
    );
}
