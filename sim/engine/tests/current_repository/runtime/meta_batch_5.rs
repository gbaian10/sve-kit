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

#[test]
fn bp10_015_paid_fanfare_bottoms_an_enemy_and_draws() {
    let mut initial = in_hand("BP10-015");
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["deck"], json!(["top", "enemy"]));
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    assert_eq!(state["P1"]["pp"]["current"], 4_u8, "1 + 5");
}

#[test]
fn bp12_045_refunds_six_with_no_cemetery_follower_then_strikes_on_witch_spells() {
    let mut initial = in_hand("BP12-045");
    push(&mut initial, "P1", "hand", json!({"id":"spell","card":"BP01-063"}));
    // The test spell's optional Earth Rite needs a stack amulet to pay.
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let refunded = game.projection(View::Referee).unwrap();
    assert_eq!(refunded["P1"]["pp"]["current"], 9_u8, "10 - 7 + 6");
    play(&mut game, "spell");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn bp16_100_activation_summons_a_holy_tiger_that_acts_an_enemy() {
    let mut initial = on_field("BP16-100");
    push(&mut initial, "P1", "field", json!({"id":"other","card":"BP01-075"}));
    let mut game = game(&initial);
    activate(&mut game, "BP16-100", 0).unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "ホーリータイガー").len(), 1);
    assert!(state["P1"]["cemetery"].as_array().unwrap().contains(&json!("other")));
    assert_eq!(state["objects"]["enemy"]["acted"], true);
}

#[test]
fn bp19_062_awakened_fanfare_fields_a_storming_orca_and_discounts_ex_ocean_cards() {
    let mut game = game(&in_hand("BP19-062"));
    play(&mut game, "source");
    let placed = game.projection(View::Referee).unwrap();
    let fielded = named(&placed, &placed["P1"]["field"], "オルカ");
    assert_eq!(fielded.len(), 1);
    assert!(has_keyword(&placed, &fielded[0], "疾走"));
    assert_eq!(placed["P1"]["leader"]["life"], 21_u8);
    let in_ex = named(&placed, &placed["P1"]["ex"], "オルカ");
    assert_eq!(in_ex.len(), 1);
    activate(&mut game, "BP19-062", 0).unwrap();
    play(&mut game, &in_ex[0]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 6_u8, "10 - 3 - (2 - 1)");
}

#[test]
fn bp19_100_heals_one_at_its_end_phase() {
    let mut initial = on_field("BP19-100");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = game(&initial);
    end_phase(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 11_u8);
}

#[test]
fn pr_033_assail_attacks_a_standing_follower() {
    let mut game = game(&on_field("PR-033"));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "assail permits attacking a standing follower"
    );
}

#[test]
fn sp01_032_fanfare_strikes_and_end_phase_drains_with_three_vampires() {
    let mut initial = in_hand("SP01-032");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"v1","card":"BP16-085"},{"id":"v2","card":"BP16-085"},{"id":"v3","card":"BP16-085"}
    ]);
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = game(&initial);
    play_with(&mut game, "source", &[1]);
    let struck = game.projection(View::Referee).unwrap();
    assert_eq!(struck["objects"]["enemy"]["damage"], 4_u8);
    end_phase(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
    assert_eq!(state["P1"]["leader"]["life"], 13_u8);
}
