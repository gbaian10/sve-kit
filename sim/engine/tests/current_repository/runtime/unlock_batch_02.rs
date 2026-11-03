#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use alloc::collections::BTreeSet;
use sve_engine::game::internal::{SearchAnswer, SearchDomain};

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "unlock-batch-02",
    )
    .unwrap()
}

fn titled(mut initial: Value) -> Value {
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial
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

fn evolving(base: &str, form: &str) -> Value {
    let mut initial = on_field(base);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn take(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |most| usize::try_from(most.0).unwrap())
        .min(available)
}

/// Takes the first legal cards, one per printed name, group by group.
fn search(domain: &SearchDomain) -> SearchAnswer {
    let mut names = BTreeSet::new();
    let mut pick = |handles: &[u64], cardinality: &Cardinality| -> Vec<u64> {
        let limit = take(cardinality, handles.len());
        handles
            .iter()
            .copied()
            .filter(|handle| {
                let name = &domain.cards[usize::try_from(*handle).unwrap()].name;
                names.insert(name.clone())
            })
            .take(limit)
            .collect()
    };
    match &domain.groups {
        None => SearchAnswer::Ungrouped(pick(
            &domain.cards.iter().map(|card| card.handle).collect::<Vec<_>>(),
            &domain.cardinality,
        )),
        Some(groups) => SearchAnswer::Grouped(
            groups
                .iter()
                .map(|group| pick(&group.handles, &group.cardinality))
                .collect(),
        ),
    }
}

fn finish_with(game: &mut Game, mut play: CardPlay, modes: &[u64]) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(modes.iter().copied().map(ModeId).collect()),
            InputDomain::Search(domain) => Answer::Search(search(domain)),
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
    play
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

fn definition(card: &str, kind: AbilityKind, nth: usize) -> AbilityId {
    selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .filter(|ability| ability.node.kind == kind)
        .nth(nth)
        .unwrap()
        .node
        .id
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

fn activate(game: &mut Game, card: &str) {
    let step = game
        .activate_internal_card("source", definition(card, AbilityKind::Activated, 0), actor())
        .unwrap();
    finish_with(game, step, &[1]);
    triggers_with(game, &[1]);
}

fn named(state: &Value, zone: &Value, name: &str) -> usize {
    zone.as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

fn ride(game: &mut Game, card: &str) {
    let id = selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            matches!(
                ability.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Ride,
                    ..
                }
            )
        })
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(question) = game.activate_internal_card("source", id, actor()).unwrap()
    else {
        panic!("Ride payment")
    };
    let step = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::ResourceEntryPayment {
                materials: vec![0],
                use_ep: false,
            },
        )
        .unwrap();
    finish_with(game, step, &[1]);
    triggers_with(game, &[1]);
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

/// The opposing ability-destroy spell is aimed at `source` controlled by P2.
fn hunted(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"hunt","card":"BP21-072"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

#[test]
fn cp04_097_damages_the_opposing_leader_for_its_own_and_another_ub() {
    let mut initial = titled(in_hand("CP04-097"));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ninon","card":"CP04-035"}]);
    let mut game = game(&initial);
    play(&mut game);
    assert_eq!(game.projection(View::Referee).unwrap()["P2"]["leader"]["life"], 19_u8);
    let step = game
        .activate_internal_card("ninon", definition("CP04-035", AbilityKind::Activated, 0), actor())
        .unwrap();
    finish_with(&mut game, step, &[1]);
    triggers_with(&mut game, &[1]);
    assert_eq!(game.projection(View::Referee).unwrap()["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn cp04_077_damage_counts_every_diabolos_follower_on_the_field() {
    let mut initial = titled(in_hand("CP04-077"));
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"yori","card":"CP04-081"},{"id":"goblin","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
        3_u8,
        "Yori and Shinobu herself count, the goblin does not"
    );
}

#[test]
fn cp04_078_evolution_summons_dokuro_oyaji() {
    let mut game = game(&titled(evolving("CP04-077", "CP04-078")));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "ドクロ親父"), 1);
}

#[test]
fn cp04_084_ub_activation_banishes_and_heals_two() {
    let mut initial = titled(on_field("CP04-084"));
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = game(&initial);
    activate(&mut game, "CP04-084");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["banish"], json!(["enemy"]));
    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
}

#[test]
fn cp03_034_drive_reveals_a_royal_paladin_card_and_pings_the_leader() {
    let mut initial = on_field("CP03-034");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"point","card":"CSD03a-017"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"CP03-023"},{"id":"tail","card":"BP01-171"}]);
    let mut game = game(&initial);
    ride(&mut game, "CP03-034");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["top"]));
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}

#[test]
fn bp19_057_survives_ability_destruction_and_goes_to_ex_after_combat() {
    let mut game = game(&hunted("BP19-057"));
    play_with(&mut game, "hunt", &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["source"]), "ability destruction is ignored");
    let mut initial = on_field("BP19-057");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut board = super::game(&initial);
    attack(&mut board, "source", "enemy");
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["ex"].as_array().unwrap().len(), 1, "the lastword moves it to EX");
    assert_eq!(named(&after, &after["P1"]["ex"], "禁牙の執行者・ドラズエル"), 1);
}

#[test]
fn bp02_089_survives_a_destroy_all_spell_that_takes_the_goblin() {
    let mut initial = on_field("BP02-089");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"goblin","card":"BP01-171"}));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"judgment","card":"BP01-131"}]);
    let mut game = game(&initial);
    play_with(&mut game, "judgment", &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source"]), "ability destruction is ignored");
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn bp01_107_pays_three_life_to_return_after_destruction() {
    let mut initial = on_field("BP01-107");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    attack(&mut game, "source", "enemy");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 17_u8);
    assert_eq!(named(&state, &state["P1"]["field"], "デュエリスト・モルディカイ"), 1);
}

#[test]
fn bp06_023_destroys_a_cheap_target_and_banishes_an_expensive_one() {
    for (enemy, zone) in [("BP01-037", "cemetery"), ("BP14-032", "banish")] {
        let mut initial = evolving("BP06-022", "BP06-023");
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":enemy}]);
        let mut game = game(&initial);
        evolve(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"][zone],
            json!(["enemy"]),
            "{enemy}"
        );
    }
}
