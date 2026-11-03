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

fn triggers(game: &mut Game) {
    triggers_with(game, &[1]);
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

fn activate_with(game: &mut Game, card: &str, index: usize, modes: &[u64]) {
    let step = game
        .activate_internal_card("source", activation(card, index), actor())
        .unwrap();
    finish_with(game, step, modes);
    triggers_with(game, modes);
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

/// A stack amulet with one counter, ready to pay an Earth Rite.
fn soil() -> Value {
    json!({"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}})
}

/// An acted enemy that can be attacked and survives the exchange.
fn acted_enemy() -> Value {
    json!({"id":"enemy","card":"BP01-037","state":{"acted":true,"hp":20_u8,"max_hp":20_u8}})
}

#[test]
fn bp01_018_deals_one_to_an_enemy_when_another_ally_enters() {
    let mut initial = on_field("BP01-018");
    push(&mut initial, "P1", "hand", json!({"id":"ally","card":"BP01-171"}));
    let mut game = game(&initial);
    play(&mut game, "ally");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
}

#[test]
fn bp01_019_deals_one_to_two_enemies_when_another_ally_enters() {
    let mut initial = on_field("BP01-018");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-019"}]);
    push(&mut initial, "P1", "hand", json!({"id":"ally","card":"BP01-171"}));
    push(&mut initial, "P2", "field", json!({"id":"other","card":"BP01-037"}));
    let mut game = game(&initial);
    evolve(&mut game, 1);
    play(&mut game, "ally");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(state["objects"]["other"]["damage"], 1_u8);
}

#[test]
fn bp01_030_evolution_destroys_only_an_acted_enemy() {
    let mut initial = on_field("BP01-029");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-030"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"standing","card":"BP01-037"},
        {"id":"enemy","card":"BP01-037","state":{"acted":true}}
    ]);
    let mut game = game(&initial);
    evolve(&mut game, 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["standing"]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn bp01_033_buffs_royal_followers_on_fanfare_and_on_entry() {
    let mut initial = in_hand("BP01-033");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"royal","card":"BP01-037"},{"id":"goblin","card":"BP01-171"}
    ]);
    push(&mut initial, "P1", "hand", json!({"id":"newcomer","card":"BP01-037"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    let fanfare = game.projection(View::Referee).unwrap();
    assert_eq!(fanfare["objects"]["royal"]["power"], 6_u8);
    assert_eq!(fanfare["objects"]["goblin"]["power"], 2_u8, "a neutral follower is not buffed");
    play(&mut game, "newcomer");
    let entry = game.projection(View::Referee).unwrap();
    assert_eq!(entry["objects"]["newcomer"]["power"], 6_u8);
    assert_eq!(entry["objects"]["newcomer"]["hp"], 6_u8);
}

#[test]
fn bp01_055_golems_cost_one_less_and_strike_on_entry() {
    let mut initial = on_field("BP01-054");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-055"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"golem","card":"BP01-T09"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = game(&initial);
    evolve(&mut game, 3);
    play(&mut game, "golem");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 6_u8, "the Golem costs 2 - 1");
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}

#[test]
fn bp01_058_fanfare_and_first_activation_make_golem_and_earth_sigil() {
    let mut game = game(&in_hand("BP01-058"));
    play(&mut game, "source");
    activate_with(&mut game, "BP01-058", 0, &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "防御型ゴーレム").len(), 1);
    assert_eq!(named(&state, &state["P1"]["field"], "大地の魔片").len(), 1);
    assert_eq!(state["objects"]["source"]["acted"], true);
}

#[test]
fn bp01_058_earth_rite_activation_makes_the_chosen_golem() {
    let mut initial = on_field("BP01-058");
    push(&mut initial, "P1", "field", soil());
    let mut game = game(&initial);
    activate_with(&mut game, "BP01-058", 1, &[2]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "攻撃型ゴーレム").len(), 1);
    assert_eq!(state["P1"]["cemetery"], json!(["soil"]));
}

#[test]
fn bp01_067_first_mode_pays_earth_rite_for_plus_one_plus_two() {
    let mut initial = in_hand("BP01-067");
    initial["players"]["P1"]["zones"]["field"] = json!([soil()]);
    let mut game = game(&initial);
    play_with(&mut game, "source", &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert_eq!(state["objects"]["source"]["hp"], 6_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["soil"]));
}

#[test]
fn bp01_067_second_mode_makes_an_earth_sigil() {
    let mut initial = in_hand("BP01-067");
    initial["players"]["P1"]["zones"]["field"] = json!([soil()]);
    let mut sigil = game(&initial);
    play_with(&mut sigil, "source", &[2]);
    let state = sigil.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 3_u8, "the first mode is not applied");
    assert!(
        state["P1"]["field"].as_array().unwrap().contains(&json!("soil")),
        "the second mode does not pay an Earth Rite"
    );
    assert_eq!(named(&state, &state["P1"]["field"], "大地の魔片").len(), 1);
}

#[test]
fn bp01_068_lastword_makes_an_earth_sigil() {
    let mut initial = on_field("BP01-068");
    initial["players"]["P2"]["zones"]["field"] = json!([acted_enemy()]);
    let mut game = game(&initial);
    attack(&mut game, "source", "enemy");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(named(&state, &state["P1"]["field"], "大地の魔片").len(), 1);
}

#[test]
fn bp01_069_lastword_makes_an_earth_sigil_and_adds_a_stack() {
    let mut initial = on_field("BP01-068");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-069"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([acted_enemy()]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    attack(&mut game, "source", "enemy");
    let state = game.projection(View::Referee).unwrap();
    let sigils = named(&state, &state["P1"]["field"], "大地の魔片");
    assert_eq!(sigils.len(), 1);
    assert_eq!(
        state["objects"][sigils[0].as_str()]["counters"]["スタックカウンター"],
        2_u8
    );
}

#[test]
fn bp01_137_deals_four_more_damage_only_during_the_opponent_turn() {
    let mut initial = on_field("BP01-137");
    initial["players"]["P2"]["zones"]["field"] = json!([acted_enemy()]);
    let mut own_turn = game(&initial);
    attack(&mut own_turn, "source", "enemy");
    let own = own_turn.projection(View::Referee).unwrap();
    assert_eq!(own["objects"]["enemy"]["damage"], 1_u8);

    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"acted":true});
    initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(false);
    let mut their_turn = game(&initial);
    attack(&mut their_turn, "enemy", "source");
    let theirs = their_turn.projection(View::Referee).unwrap();
    assert_eq!(theirs["objects"]["enemy"]["damage"], 5_u8);
}
