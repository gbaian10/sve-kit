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

fn soil() -> Value {
    json!({"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}})
}

#[test]
fn bp12_071_fanfare_mills_two_and_returns_from_a_ten_card_cemetery() {
    let mut game = game(&in_hand("BP12-071"));
    play(&mut game, "source");
    let milled = game.projection(View::Referee).unwrap();
    assert_eq!(milled["P1"]["cemetery"].as_array().unwrap().len(), 2);

    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"source","card":"BP12-071"}]);
    fill(&mut initial, "cemetery", "BP01-171", 8);
    let mut short = super::game(&initial);
    assert!(activate(&mut short, "BP12-071", 0).is_err(), "nine cemetery cards are not enough");
    fill(&mut initial, "cemetery", "BP12-072", 1);
    let mut full = super::game(&initial);
    activate(&mut full, "BP12-071", 0).unwrap();
    let state = full.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp10_006_buffs_mercenaries_and_discounts_its_look_pick() {
    let mut initial = on_field("BP10-005");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-006"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"cheap","card":"BP20-040"},{"id":"a","card":"BP01-037"},
        {"id":"b","card":"BP01-037"},{"id":"c","card":"BP01-037"}
    ]);
    push(&mut initial, "P1", "hand", json!({"id":"merc","card":"BP11-115"}));
    let mut game = game(&initial);
    evolve(&mut game, 1);
    let looked = game.projection(View::Referee).unwrap();
    assert_eq!(looked["P1"]["ex"], json!(["cheap"]));
    play(&mut game, "cheap");
    let discounted = game.projection(View::Referee).unwrap();
    assert_eq!(discounted["P1"]["pp"]["current"], 9_u8, "the picked 2-cost costs 0");
    play(&mut game, "merc");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["merc"]["power"], 3_u8);
    assert!(has_keyword(&state, "merc", "突進"));
}

#[test]
fn bp11_115_fanfare_may_place_its_bike_in_ex() {
    let mut initial = in_hand("BP11-115");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    play_with(&mut game, "source", &[2]);
    let placed = game.projection(View::Referee).unwrap();
    assert_eq!(named(&placed, &placed["P1"]["ex"], "機動二輪車").len(), 1);
}

#[test]
fn bp09_048_earth_rite_deals_two() {
    let mut initial = on_field("BP09-048");
    push(&mut initial, "P1", "field", soil());
    let mut game = game(&initial);
    activate(&mut game, "BP09-048", 0).unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert!(state["P1"]["cemetery"].as_array().unwrap().contains(&json!("soil")));
}

#[test]
fn bp20_041_deals_two_once_per_turn_when_an_idol_amulet_enters() {
    let mut initial = on_field("BP20-041");
    push(&mut initial, "P1", "hand", json!({"id":"amulet1","card":"BP20-055"}));
    push(&mut initial, "P1", "hand", json!({"id":"amulet2","card":"BP20-055"}));
    let mut game = game(&initial);
    play(&mut game, "amulet1");
    play(&mut game, "amulet2");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8, "only the first amulet triggers");
}

#[test]
fn bp16_067_fanfare_draws_and_discards_then_activates_with_big_dragons() {
    let mut game = game(&in_hand("BP16-067"));
    play(&mut game, "source");
    let drawn = game.projection(View::Referee).unwrap();
    assert_eq!(drawn["P1"]["hand"].as_array().unwrap().len(), 0);
    assert_eq!(drawn["P1"]["cemetery"].as_array().unwrap().len(), 1);

    let mut initial = on_field("BP16-067");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([]);
    fill(&mut initial, "cemetery", "BP22-071", 3);
    let mut short = super::game(&initial);
    assert!(activate(&mut short, "BP16-067", 0).is_err(), "three big dragons are not enough");
    fill(&mut initial, "cemetery", "BP22-071", 1);
    let mut ready = super::game(&initial);
    activate(&mut ready, "BP16-067", 0).unwrap();
    let state = ready.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn bp08_048_fanfare_makes_an_earth_sigil() {
    let mut game = game(&in_hand("BP08-048"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "大地の魔片").len(), 1);
}

#[test]
fn bp06_035_and_036_cost_less_per_onmyoji_or_spell_and_summon_tenko() {
    for card in ["BP06-035", "BP06-036"] {
        let mut initial = in_hand(card);
        fill(&mut initial, "cemetery", "BP06-044", 3);
        fill(&mut initial, "cemetery", "BP01-114", 3);
        // An onmyoji spell counts once (Q808).
        fill(&mut initial, "cemetery", "BP06-042", 1);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 2_u8, "{card}: 15 - 7 = 8");
        assert_eq!(named(&state, &state["P1"]["field"], "天后").len(), 1, "{card}");
    }
}

fn acted_enemy() -> Value {
    json!({"id":"enemy","card":"BP01-037","state":{"acted":true,"hp":20_u8,"max_hp":20_u8}})
}

#[test]
fn bp09_070_first_face_discounts_forest_bats_and_heals_on_their_entry() {
    let mut initial = on_field("BP09-070");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"bat","card":"BP06-T09"}]);
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    let mut game = game(&initial);
    play(&mut game, "bat");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 10_u8, "the 1-cost bat costs 0");
    assert_eq!(state["P1"]["leader"]["life"], 16_u8);
}

#[test]
fn dsd01b_001_lastword_makes_a_lingering_bolt_or_charges_existing_ones() {
    let mut initial = on_field("DSD01b-001");
    initial["players"]["P2"]["zones"]["field"] = json!([acted_enemy()]);
    let mut fresh = game(&initial);
    attack(&mut fresh, "source", "enemy");
    let made = fresh.projection(View::Referee).unwrap();
    assert_eq!(named(&made, &made["P1"]["field"], "遺されし電撃").len(), 1);

    push(&mut initial, "P1", "field", json!({"id":"bolt","card":"DSD01b-T01"}));
    let mut charged = game(&initial);
    attack(&mut charged, "source", "enemy");
    let state = charged.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "遺されし電撃"), vec!["bolt"]);
    assert_eq!(state["objects"]["bolt"]["counters"]["雷カウンター"], 1_u8);
}

#[test]
fn dsd01b_002_evolution_deals_three_only_with_an_amulet() {
    for (amulet, damage) in [(true, 3_u8), (false, 0)] {
        let mut initial = on_field("DSD01b-001");
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"DSD01b-002"}]);
        if amulet {
            push(&mut initial, "P1", "field", json!({"id":"bolt","card":"DSD01b-T01"}));
        }
        let mut game = game(&initial);
        evolve(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], damage, "amulet {amulet}");
    }
}
