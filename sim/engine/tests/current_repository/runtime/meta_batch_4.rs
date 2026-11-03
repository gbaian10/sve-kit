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

fn acted_enemy() -> Value {
    json!({"id":"enemy","card":"BP01-037","state":{"acted":true,"hp":20_u8,"max_hp":20_u8}})
}

#[test]
fn bp11_057_its_horse_entering_strikes_an_enemy() {
    let mut game = game(&in_hand("BP11-057"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "従順な駿馬").len(), 1);
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
}

#[test]
fn bp11_050_makes_an_earth_sigil_and_adds_two_stacks() {
    let mut game = game(&in_hand("BP11-050"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    let sigils = named(&state, &state["P1"]["field"], "大地の魔片");
    assert_eq!(sigils.len(), 1);
    assert_eq!(
        state["objects"][sigils[0].as_str()]["counters"]["スタックカウンター"],
        3_u8
    );
}

#[test]
fn pr_120_deals_damage_equal_to_its_follower_count() {
    let mut initial = in_hand("PR-120");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"f1","card":"BP01-171"},{"id":"f2","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
}

#[test]
fn bp11_038_its_bike_entering_strikes_and_loots() {
    let mut game = game(&in_hand("BP11-038"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "機動二輪車").len(), 1);
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 1, "draws one and discards one");
}

#[test]
fn bp11_039_six_original_costs_bring_an_armored_car_that_strikes() {
    let mut initial = on_field("BP11-038");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP11-039"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"c1","card":"BP01-171"},{"id":"c2","card":"BP20-040"},{"id":"c3","card":"BP06-044"},
        {"id":"c4","card":"BP01-033"},{"id":"c5","card":"BP01-037"},{"id":"c7","card":"BP22-071"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "魔導装甲車").len(), 1);
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8, "the vehicle entering triggers");
}

#[test]
fn bp11_106_lastword_may_place_all_three_vehicles() {
    let mut initial = on_field("BP11-105");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP11-106"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([acted_enemy()]);
    initial["players"]["P2"]["zones"]["field"][0]["state"]["power"] = json!(10_u8);
    let mut game = game(&initial);
    evolve(&mut game, 4);
    attack(&mut game, "source", "enemy");
    let state = game.projection(View::Referee).unwrap();
    for name in ["従順な駿馬", "機動二輪車", "魔導装甲車"] {
        assert_eq!(named(&state, &state["P1"]["field"], name).len(), 1, "{name}");
    }
}

#[test]
fn bp13_029_paid_fanfare_summons_royal_tokens_that_each_strike() {
    let mut initial = in_hand("BP13-029");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "スティールナイト").len(), 1);
    assert_eq!(named(&state, &state["P1"]["field"], "シールドガーディアン").len(), 1);
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P1"]["pp"]["current"], 6_u8, "1 + 3");
}

#[test]
fn bp18_063_evolution_deals_three_with_a_strong_martial_dragonewt() {
    let mut initial = on_field("BP18-062");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP18-063"}]);
    push(&mut initial, "P1", "field", json!({"id":"strong","card":"BP18-071"}));
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
}

#[test]
fn pr_296_first_activation_acts_an_enemy() {
    let mut game = game(&on_field("PR-296"));
    activate(&mut game, "PR-296", 0).unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["acted"], true);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}
