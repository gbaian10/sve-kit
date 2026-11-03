#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed Ride migration scenarios."
)]
use super::*;
use sve_engine::game::internal::{DamageShare, SearchAnswer, SearchDomain};

#[derive(Clone, Copy)]
enum Case {
    Aqua,
    Hit(u8),
    Destroy(u8),
    BanishTop,
    Split,
    ShadowLook,
    SearchField,
    Heal,
    Burn,
    Kagero,
}

fn position(card: &str, case: Case) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"point","card":"CSD03a-017"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
    initial["players"]["P1"]["zones"]["deck"] = match case {
        Case::Aqua => {
            json!([{"id":"top","card":"CP03-011"},{"id":"duplicate","card":"CP03-011"},{"id":"tail","card":"CP03-013"}])
        }
        Case::ShadowLook => {
            json!([{"id":"top","card":"CP03-099"},{"id":"middle","card":"CP03-098"},{"id":"tail","card":"BP01-009"}])
        }
        Case::SearchField => json!([{"id":"top","card":"CP03-099"}]),
        Case::Kagero => json!([{"id":"top","card":"CP03-063"}]),
        _ => json!([{"id":"top","card":"BP01-009"},{"id":"tail","card":"BP01-037"}]),
    };
    if let Case::Destroy(count) = case {
        initial["players"]["P1"]["zones"]["banish"] = json!(
            (0_u8..count)
                .map(|i| json!({"id":format!("banished-{i}"),"card":"BP01-009"}))
                .collect::<Vec<_>>()
        );
    }
    initial
}

fn take(bounds: &Cardinality, available: usize) -> usize {
    bounds
        .maximum
        .map_or(available, |n| usize::try_from(n.0).unwrap())
        .min(available)
}

fn search(domain: &SearchDomain) -> SearchAnswer {
    let mut used = Vec::<String>::new();
    let mut pick = |handles: &[u64], count: usize| {
        handles
            .iter()
            .copied()
            .filter(|handle| {
                let name = &domain
                    .cards
                    .iter()
                    .find(|card| card.handle == *handle)
                    .unwrap()
                    .name;
                if used.contains(name) {
                    false
                } else {
                    used.push(name.clone());
                    true
                }
            })
            .take(count)
            .collect::<Vec<_>>()
    };
    if let Some(groups) = &domain.groups {
        SearchAnswer::Grouped(
            groups
                .iter()
                .map(|group| {
                    pick(
                        &group.handles,
                        take(&group.cardinality, group.handles.len()),
                    )
                })
                .collect(),
        )
    } else {
        let handles: Vec<_> = domain.cards.iter().map(|card| card.handle).collect();
        SearchAnswer::Ungrouped(pick(&handles, take(&domain.cardinality, handles.len())))
    }
}

fn answer(domain: &InputDomain) -> Answer {
    match domain {
        InputDomain::Targets(d) => Answer::Targets(
            d.members
                .iter()
                .take(take(&d.cardinality, d.members.len()))
                .map(|m| m.handle)
                .collect(),
        ),
        InputDomain::CostMaterials(d) => Answer::CostMaterials(
            d.members
                .iter()
                .take(take(&d.cardinality, d.members.len()))
                .map(|m| m.handle)
                .collect(),
        ),
        InputDomain::OptionalCost => Answer::OptionalCost(true),
        InputDomain::WardEntry { .. } => Answer::WardEntry(false),
        InputDomain::Search(d) => Answer::Search(search(d)),
        InputDomain::SearchPartition(d) => Answer::SearchPartition(
            d.cards
                .iter()
                .take(take(&d.cardinality, d.cards.len()))
                .map(|c| c.handle)
                .collect(),
        ),
        InputDomain::Order(cards) => Answer::Order(cards.iter().map(|c| c.handle).collect()),
        InputDomain::Distribution(d) => Answer::Distribution(
            d.members
                .iter()
                .enumerate()
                .map(|(i, m)| DamageShare {
                    handle: m.handle,
                    amount: if i == 0 { d.total } else { Nat(0) },
                })
                .collect(),
        ),
        _ => panic!("unexpected Ride migration question"),
    }
}

fn finish(board: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        play = board
            .answer_internal_card(q.actor, q.token, answer(&q.domain))
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Resolved(_) | CardPlay::Entered(_)),
        "the admitted printed operation completes"
    );
}

fn pending_all(board: &mut Game) {
    loop {
        if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let Some(q) = board.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending")
        };
        let play = board
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(board, play);
    }
}

fn check_effect(view: &Value, case: Case) {
    match case {
        Case::Aqua => {
            let hand = view["P1"]["hand"].as_array().unwrap();
            assert_eq!(hand.len(), 2, "the search adds two cards");
            assert!(
                hand.contains(&json!("tail")),
                "the second printed name is selected despite duplicate copies of the first"
            );
            assert!(
                hand.contains(&json!("top")) || hand.contains(&json!("duplicate")),
                "one copy of the first printed name is selected"
            );
        }
        Case::Hit(n) => assert_eq!(
            view["objects"]["enemy"]["hp"],
            20_u8.checked_sub(n).unwrap(),
            "the selected enemy receives the printed Drive-time damage"
        ),
        Case::Destroy(count) => {
            assert_eq!(
                view["P2"]["cemetery"],
                if count >= 5 {
                    json!(["enemy"])
                } else {
                    json!([])
                },
                "destruction requires five banished cards"
            );
            assert_eq!(
                view["P1"]["leader"]["life"], 12_u8,
                "the leader heals after the conditional destruction"
            );
        }
        Case::BanishTop => {
            assert_eq!(
                view["P1"]["banish"],
                json!(["top"]),
                "the top deck card is banished after damage and buff"
            );
            assert_eq!(
                view["objects"]["enemy"]["hp"], 18_u8,
                "the chosen enemy also takes two"
            );
        }
        Case::Split => assert_eq!(
            view["objects"]["enemy"]["hp"], 16_u8,
            "the four damage allocation is applied"
        ),
        Case::ShadowLook => {
            assert_eq!(
                view["P1"]["hand"],
                json!(["top"]),
                "one observed Shadow Paladin card enters hand"
            );
            assert_eq!(
                view["P1"]["cemetery"],
                json!(["middle", "tail"]),
                "the other observed cards enter cemetery"
            );
        }
        Case::SearchField => assert_eq!(
            view["P1"]["field"],
            json!(["source", "top"]),
            "the searched low-cost Shadow Paladin enters the field"
        ),
        Case::Heal => assert_eq!(
            view["P1"]["leader"]["life"], 11_u8,
            "the Drive-time effect heals one"
        ),
        Case::Burn => assert_eq!(
            view["P2"]["leader"]["life"], 17_u8,
            "all opposing leaders receive three"
        ),
        Case::Kagero => assert_eq!(
            view["P1"]["hand"],
            json!(["top"]),
            "the searched Kagero follower enters hand"
        ),
    }
}

fn run(card: &str, case: Case, cost: u8) {
    let mut board = Game::from_selected_effects(
        selection_with_rules().clone(),
        &position(card, case),
        &Value::Null,
        &Value::Null,
        "ride-batch1",
    )
    .unwrap();
    let before = board.projection(View::Referee).unwrap();
    let power = before["objects"]["source"]["power"].as_i64().unwrap();
    let hp = before["objects"]["source"]["hp"].as_i64().unwrap();
    let definition = selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            matches!(
                a.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Ride,
                    ..
                }
            )
        })
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(q) = board
        .activate_internal_card("source", definition, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Ride payment")
    };
    let play = board
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::ResourceEntryPayment {
                materials: vec![0],
                use_ep: false,
            },
        )
        .unwrap();
    finish(&mut board, play);
    pending_all(&mut board);
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["drive"],
        json!(["point"]),
        "Ride links exactly one DrivePoint"
    );
    assert_eq!(
        view["P1"]["pp"]["current"],
        10_u8.checked_sub(cost).unwrap(),
        "only the printed Ride cost is paid"
    );
    assert_eq!(
        view["objects"]["source"]["power"],
        power.checked_add(1).unwrap(),
        "the printed Drive-time buff raises attack once"
    );
    assert_eq!(
        view["objects"]["source"]["hp"],
        hp.checked_add(1).unwrap(),
        "the printed Drive-time buff raises life once"
    );
    check_effect(&view, case);
}

#[test]
fn cp03_008_all_printings_execute_the_printed_drive_time_effect() {
    for card in ["CP03-008", "CP03-P04"] {
        run(card, Case::Aqua, 1_u8);
    }
}

#[test]
fn cp03_024_all_printings_execute_the_printed_drive_time_effect() {
    for card in ["CP03-024", "CP03-SL07"] {
        run(card, Case::Hit(4), 1_u8);
    }
}

#[test]
fn cp03_050_all_printings_execute_the_printed_drive_time_effect() {
    for card in ["CP03-050", "CP03-P33"] {
        run(card, Case::Destroy(5), 1_u8);
        run(card, Case::Destroy(4), 1_u8);
    }
}

#[test]
fn cp03_051_all_printings_execute_the_printed_drive_time_effect() {
    for card in ["CP03-051", "CP03-P34"] {
        run(card, Case::BanishTop, 1_u8);
    }
}

#[test]
fn cp03_071_all_printings_execute_the_printed_drive_time_effect() {
    for card in ["CP03-071", "CP03-P48"] {
        run(card, Case::Split, 1_u8);
    }
}

#[test]
fn cp03_088_all_printings_execute_the_printed_drive_time_effect() {
    for card in ["CP03-088", "CP03-P58"] {
        run(card, Case::ShadowLook, 1_u8);
    }
}

#[test]
fn cp03_090_all_printings_execute_the_printed_drive_time_effect() {
    for card in ["CP03-090", "PR-282"] {
        run(card, Case::SearchField, 1_u8);
    }
}

#[test]
fn cp03_114_all_printings_execute_the_printed_drive_time_effect() {
    for card in ["CP03-114", "CP03-P78"] {
        run(card, Case::Heal, 1_u8);
    }
}

#[test]
fn cp03_122_all_printings_execute_the_printed_drive_time_effect() {
    for card in ["CP03-122", "CP03-P85"] {
        run(card, Case::Hit(2), 1_u8);
    }
}

#[test]
fn csd03a_008_all_printings_execute_the_printed_drive_time_effect() {
    for card in ["CSD03a-008", "CSD03a-P04"] {
        run(card, Case::Burn, 1_u8);
    }
}

#[test]
fn csd03a_011_all_printings_execute_the_printed_drive_time_effect() {
    for card in ["CSD03a-011", "CSD03a-P07"] {
        run(card, Case::Hit(2), 1_u8);
    }
}

#[test]
fn csd03b_006_all_printings_execute_the_printed_drive_time_effect() {
    for card in ["CSD03b-006", "CSD03b-P03"] {
        run(card, Case::Hit(3), 1_u8);
    }
}

#[test]
fn csd03b_011_all_printings_execute_the_printed_drive_time_effect() {
    for card in ["CSD03b-011", "CSD03b-P08"] {
        run(card, Case::Kagero, 2_u8);
    }
}
