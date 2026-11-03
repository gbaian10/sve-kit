#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;

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

fn graves(count: u8) -> Value {
    json!(
        (0..count)
            .map(|i| json!({"id":format!("grave{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    )
}

fn finish_with(game: &mut Game, mut play: CardPlay, modes: &[u64]) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(modes.iter().copied().map(ModeId).collect()),
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::CostMaterials(domain) => {
                let most = domain
                    .cardinality
                    .maximum
                    .map_or(domain.members.len(), |n| usize::try_from(n.0).unwrap());
                Answer::CostMaterials(
                    domain
                        .members
                        .iter()
                        .take(most)
                        .map(|member| member.handle)
                        .collect(),
                )
            }
            other => panic!("unexpected decision: {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn triggers(game: &mut Game) {
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
        finish_with(game, step, &[1]);
    }
}

fn play_with(game: &mut Game, id: &str, modes: &[u64]) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish_with(game, step, modes);
    triggers(game);
}

fn play(game: &mut Game) {
    play_with(game, "source", &[1]);
}

fn evolve(game: &mut Game, source: &str) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":source,"evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the base pays its printed evolution cost"
    );
    triggers(game);
}

fn printed_activation(card: &str) -> AbilityId {
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

fn activate_with(game: &mut Game, card: &str, modes: &[u64]) {
    let step = game
        .activate_internal_card("source", printed_activation(card), actor())
        .unwrap();
    finish_with(game, step, modes);
    triggers(game);
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

/// Ids of the objects in a zone whose card has the given name.
fn named(state: &Value, zone: &Value, name: &str) -> Vec<String> {
    zone.as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .filter(|id| state["objects"][id]["name"] == name)
        .collect()
}

fn ghosts_on_field(state: &Value) -> Vec<String> {
    named(state, &state["P1"]["field"], "ゴースト")
}

#[test]
fn bp01_t14_every_printing_storms_and_vanishes_at_its_own_end_phase() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"ghost","card":"BP01-T14"},{"id":"corrected","card":"PR-114"}
    ]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    for id in ["ghost", "corrected"] {
        assert!(
            before["objects"][id]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走")),
            "{id} has storm"
        );
    }
    end_phase(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!([]));
}

#[test]
fn bp01_114_summons_four_ghosts() {
    let mut game = game(&in_hand("BP01-114"));
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(ghosts_on_field(&state).len(), 4);
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
}

#[test]
fn bp02_080_puts_one_ghost_in_ex_or_three_with_necrocharge() {
    for (count, expected) in [(9_u8, 1_usize), (10, 3)] {
        let mut initial = in_hand("BP02-080");
        initial["players"]["P1"]["zones"]["cemetery"] = graves(count);
        let mut game = game(&initial);
        play(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(named(&state, &state["P1"]["ex"], "ゴースト").len(), expected);
    }
}

#[test]
fn sd05_011_summons_a_ghost_when_it_attacks() {
    let mut initial = on_field("SD05-010");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"SD05-011"}]);
    let mut game = game(&initial);
    evolve(&mut game, "source");
    attack(&mut game, "source", "P2.leader");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(ghosts_on_field(&state).len(), 1);
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}

#[test]
fn bp07_084_summons_two_ghosts_on_an_ally_evolution_then_gives_one_bane() {
    let mut initial = on_field("BP07-084");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"mummy","card":"SD05-010"}));
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"SD05-011"}]);
    let mut game = game(&initial);
    evolve(&mut game, "mummy");
    let evolved = game.projection(View::Referee).unwrap();
    let ghosts = ghosts_on_field(&evolved);
    assert_eq!(ghosts.len(), 2);
    activate_with(&mut game, "BP07-084", &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["acted"], true);
    let bane = ghosts
        .iter()
        .filter(|id| {
            state["objects"][id.as_str()]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("必殺"))
        })
        .count();
    assert_eq!(bane, 1, "exactly the chosen Ghost gains bane");
}

#[test]
fn etd02_005_evolution_summons_three_ghosts() {
    let mut initial = on_field("ETD02-004");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"ETD02-005"}]);
    let mut game = game(&initial);
    evolve(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(ghosts_on_field(&state).len(), 3);
}

#[test]
fn bp13_086_ghosts_placed_in_ex_cost_one_less_this_turn() {
    let mut game = game(&in_hand("BP13-086"));
    play(&mut game);
    let placed = game.projection(View::Referee).unwrap();
    let ghosts = named(&placed, &placed["P1"]["ex"], "ゴースト");
    assert_eq!(ghosts.len(), 2);
    assert_eq!(placed["P1"]["pp"]["current"], 9_u8);
    play_with(&mut game, &ghosts[0], &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!([ghosts[0]]));
    assert_eq!(state["P1"]["pp"]["current"], 9_u8, "the Ghost costs zero");
}

fn crest_position(card: &str, crests: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"power":10_u8,"hp":20_u8,"max_hp":20_u8}}]);
    let extras = ["BP20-T01", "BP20-T05", "BP20-P70", "BP20-T07"];
    let mut ex = vec![json!({"id":"source","card":card})];
    for (i, no) in extras.iter().take(crests.saturating_sub(1)).enumerate() {
        ex.push(json!({"id":format!("crest-{i}"),"card":no}));
    }
    initial["players"]["P1"]["zones"]["ex"] = json!(ex);
    initial
}

#[test]
fn bp20_p67_every_printing_weakens_by_crest_threshold_at_end_phase() {
    for card in ["BP20-P67", "BP20-T08"] {
        for (crests, power) in [(2_usize, 10_u8), (3, 8), (5, 6)] {
            let mut game = game(&crest_position(card, crests));
            end_phase(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["power"], power,
                "{card} with {crests} Crests"
            );
        }
    }
}

#[test]
fn bp20_094_places_its_crest_in_ex() {
    let mut game = game(&in_hand("BP20-094"));
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        named(&state, &state["P1"]["ex"], "クレスト 安息の継承者・ヒメカ").len(),
        1
    );
}

#[test]
fn bp20_082_first_mode_deals_three_to_the_chosen_follower() {
    let mut game = game(&in_hand("BP20-082"));
    play_with(&mut game, "source", &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
}

#[test]
fn bp20_118_fanfare_trades_two_health_for_two_power() {
    let mut game = game(&in_hand("BP20-118"));
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["power"], 7_u8);
    assert_eq!(state["objects"]["enemy"]["hp"], 3_u8);
}

#[test]
fn bp09_035_earth_rite_places_a_fleeting_elixir_in_ex() {
    let mut initial = in_hand("BP09-035");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["soil"]));
    assert_eq!(named(&state, &state["P1"]["ex"], "刹那の劇薬").len(), 1);
}
