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

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn on_field(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn evolved_position(base: &str, form: &str) -> Value {
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

fn finish_with(game: &mut Game, mut play: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::Search(domain) => Answer::Search(domain.groups.as_ref().map_or_else(
                || {
                    SearchAnswer::Ungrouped(
                        domain
                            .cards
                            .iter()
                            .take(take(&domain.cardinality, domain.cards.len()))
                            .map(|card| card.handle)
                            .collect(),
                    )
                },
                |groups| {
                    // Each printed group greedily takes cards no earlier group used.
                    let mut used = Vec::new();
                    SearchAnswer::Grouped(
                        groups
                            .iter()
                            .map(|group| {
                                let picked: Vec<u64> = group
                                    .handles
                                    .iter()
                                    .copied()
                                    .filter(|handle| !used.contains(handle))
                                    .take(take(&group.cardinality, group.handles.len()))
                                    .collect();
                                used.extend(picked.iter().copied());
                                picked
                            })
                            .collect(),
                    )
                },
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
            InputDomain::Integer(bounds) => {
                Answer::Integer(bounds.maximum.unwrap_or(bounds.minimum))
            }
            other => panic!("unexpected decision: {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}


fn finish(game: &mut Game, play: CardPlay) {
    finish_with(game, play, 1);
}

fn triggers_with(game: &mut Game, mode: u64) {
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
        finish_with(game, step, mode);
    }
}

fn triggers(game: &mut Game) {
    triggers_with(game, 1);
}

fn play_with(game: &mut Game, id: &str, mode: u64) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish_with(game, step, mode);
    triggers_with(game, mode);
}

fn play(game: &mut Game, id: &str) {
    play_with(game, id, 1);
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base uses its printed evolution cost"
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

fn activate(game: &mut Game, card: &str) -> bool {
    let Ok(step) = game.activate_internal_card("source", printed_activation(card), actor()) else {
        return false;
    };
    finish(game, step);
    triggers(game);
    true
}

fn names(state: &Value, zone: &Value) -> Vec<String> {
    zone.as_array()
        .unwrap()
        .iter()
        .map(|id| {
            state["objects"][id.as_str().unwrap()]["name"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

#[test]
fn bp17_p31_union_magic_damages_two_and_adds_ultimate_magic() {
    let mut initial = position("BP17-P31");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP17-P31"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037"},{"id":"small","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["small"]));
    assert_eq!(names(&state, &state["P1"]["ex"]), ["アルティメット・マジック"]);
}

#[test]
fn bp17_038_evolution_puts_union_magic_in_ex() {
    let mut game = game(&evolved_position("BP17-037", "BP17-038"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(names(&state, &state["P1"]["ex"]), ["ユニオン・マジック"]);
}

#[test]
fn bp17_044_extra_cost_with_three_ex_machines_adds_a_second_tentacle() {
    let mut initial = position("BP17-044");
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..3_u8)
            .map(|i| json!({"id":format!("repair{i}"),"card":"BP07-T02"}))
            .collect::<Vec<_>>()
    );
    // Every Belphomet follower is still a prototype, so the search finds no match here.
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["deck"], json!(["wrong"]));
    assert_eq!(
        names(&state, &state["P1"]["field"]),
        ["遮断の触手", "蹂躙の触手"]
    );
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
}

#[test]
fn bp17_113_fanfare_makes_a_machine_and_a_repair_mode() {
    let mut game = game(&position("BP17-113"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        names(&state, &state["P1"]["field"]),
        ["フロートボードマーセナリー", "プロダクトマシーン"]
    );
    assert_eq!(names(&state, &state["P1"]["ex"]), ["リペアモード"]);
}

#[test]
fn bp17_114_evolution_makes_a_machine_and_a_repair_mode() {
    let mut game = game(&evolved_position("BP17-113", "BP17-114"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        names(&state, &state["P1"]["field"]),
        ["フロートボードマーセナリー", "プロダクトマシーン"]
    );
    assert_eq!(names(&state, &state["P1"]["ex"]), ["リペアモード"]);
}

#[test]
fn bp17_036_buffs_bayleon_and_may_place_a_natera_tree() {
    let mut initial = position("BP17-036");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"bayleon","card":"BP07-018"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["bayleon"]["power"], 4_u8);
    assert_eq!(state["objects"]["bayleon"]["hp"], 4_u8);
    assert_eq!(
        names(&state, &state["P1"]["field"]),
        ["王たる光・ベイリオン", "ナテラの大樹"]
    );
}

#[test]
fn bp17_107_played_from_hand_only_adds_repair_mode() {
    let mut game = game(&position("BP17-107"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(names(&state, &state["P1"]["ex"]), ["リペアモード"]);
    assert_eq!(
        state["objects"]["enemy"]["damage"],
        0_u8,
        "not put onto the field by an ability"
    );
}

#[test]
fn bp17_012_first_mode_buffs_a_puppet_token() {
    let mut initial = position("BP17-012");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"puppet","card":"BP08-T09"}]);
    let mut game = game(&initial);
    play_with(&mut game, "source", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["puppet"]["power"], 3_u8);
    assert_eq!(state["objects"]["puppet"]["hp"], 3_u8);
    assert_eq!(state["P1"]["ex"], json!([]));
}

#[test]
fn bp17_104_storm_fanfare_brings_two_product_machines() {
    let mut game = game(&position("BP17-104"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        names(&state, &state["P1"]["field"]),
        ["鉄鋼の翼人", "プロダクトマシーン", "プロダクトマシーン"]
    );
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
}

#[test]
fn bp17_105_evolution_brings_two_product_machines() {
    let mut game = game(&evolved_position("BP17-104", "BP17-105"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        names(&state, &state["P1"]["field"]),
        ["鉄鋼の翼人", "プロダクトマシーン", "プロダクトマシーン"]
    );
}

#[test]
fn bp17_097_evolution_brings_an_escamari_patroller() {
    let mut game = game(&evolved_position("BP17-096", "BP17-097"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        names(&state, &state["P1"]["field"]),
        ["平穏の制定者・マーロン", "エスカマリの巡回者"]
    );
}

#[test]
fn bp17_111_ten_cemetery_followers_give_storm_and_eight_damage() {
    let mut initial = evolved_position("BP17-110", "BP17-111");
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..10_u8)
            .map(|i| json!({"id":format!("grave{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&initial);
    evolve(&mut game);
    let evolved = game.projection(View::Referee).unwrap();
    assert_eq!(evolved["P1"]["hand"], json!(["first"]));
    assert!(
        evolved["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
    // Two whole sets of five followers: 4 x 2 damage destroys the 5/5.
    assert!(activate(&mut game, "BP17-111"));
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
}
