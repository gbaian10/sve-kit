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

fn named(state: &Value, zone: &Value, name: &str) -> Vec<String> {
    zone.as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .filter(|id| state["objects"][id]["name"] == name)
        .collect()
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
fn bp13_068_discards_an_armed_card_to_search_an_armed_spell() {
    let mut initial = in_hand("BP13-068");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fuel","card":"BP03-064"}));
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"smash","card":"BP03-066"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["fuel"]));
    assert_eq!(state["P1"]["hand"], json!(["smash"]));
}

#[test]
fn bp15_016_lastword_returns_a_cheap_spell_to_hand() {
    let mut initial = on_field("BP15-016");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true,"power":9_u8}}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"big","card":"BP03-066"},{"id":"mana","card":"BP07-017"}
    ]);
    let mut game = game(&initial);
    attack(&mut game, "source", "enemy");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["mana"]));
}

#[test]
fn bp13_118_searches_a_cheap_neutral_spell() {
    let mut initial = in_hand("BP13-118");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"snipe","card":"BP01-179"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["snipe"])
    );
}

#[test]
fn bp13_057_cannot_attack_and_evolves_only_while_awakened() {
    let mut initial = on_field("BP13-057");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP13-058"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    initial["players"]["P1"]["pp"] = json!({"current":6_u8,"max":6_u8});
    let mut asleep = super::game(&initial);
    assert_eq!(
        asleep
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
        "cannot-evolve"
    );
}

#[test]
fn bp13_058_banishes_at_ten_max_pp_and_heals_three_at_end() {
    let mut initial = on_field("BP13-057");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP13-058"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    triggers_with(&mut game, &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["banish"], json!(["enemy"]));
    assert_eq!(state["P1"]["leader"]["life"], 23_u8);
}

#[test]
fn bp14_113_evolution_summons_an_ancient_artifact() {
    let mut initial = on_field("BP14-112");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP14-113"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        named(&state, &state["P1"]["field"], "エンシェントアーティファクト").len(),
        1
    );
}
