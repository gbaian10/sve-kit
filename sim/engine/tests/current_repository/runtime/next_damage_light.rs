#![expect(clippy::panic, reason = "Unexpected questions invalidate the printed next-damage scenarios.")]
use super::*;
use super::event_cemetery::finish_checks;

const fn actor() -> PlayerId { PlayerId {seat: 1} }

fn pass_quick(board: &mut Game) {
    for _ in 0_u8..4 {
        let Some(question) = board.internal_quick_choice().unwrap() else { return; };
        board.answer_internal_card(question.actor, question.token, Answer::Quick(None)).unwrap();
        finish_checks(board);
    }
    panic!("combat must finish its Quick window");
}

fn attack(board: &mut Game, attacker: &str) {
    board.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":"P1.leader"})).unwrap();
    finish_checks(board);
}

fn resolve_one(board: &mut Game, printing: &str) {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else { panic!("printed occurrence"); };
    assert_eq!(options.len(), 1, "exactly one printed event must be pending");
    assert_eq!(options[0].card, printing, "the effective printing supplies the ability");
    resolved(board.answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle)).unwrap());
    finish_checks(board);
}

#[test]
fn both_seal_printings_pay_one_pp_as_quick_and_prevent_only_the_next_attack() {
    for printing in ["BP05-017", "BP05-P08"] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"seal","card":printing}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}
        ]);
        let mut board = game(&initial);
        attack(&mut board, "first");
        let question = board.internal_quick_choice().unwrap().unwrap();
        let InputDomain::Quick {options, ..} = &question.domain else { panic!("defender Quick"); };
        let option = options.iter().find(|option| option.card == printing).unwrap();
        resolved(board.answer_internal_card(question.actor, question.token, Answer::Quick(Some(option.handle))).unwrap());
        finish_checks(&mut board);
        pass_quick(&mut board);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 20_u8);
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert_eq!(state["P1"]["cemetery"], json!(["seal"]));
        attack(&mut board, "second");
        pass_quick(&mut board);
        assert_eq!(board.projection(View::Referee).unwrap()["P1"]["leader"]["life"], 15_u8);
    }
}

#[test]
fn armor_goblin_retains_ward_and_prevents_one_hit_after_its_real_evolution() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"armor","card":"BP13-116"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP13-117"}]);
    let action = Action::Damage {
        recipients: RecipientSet::Objects {objects: damage_prevent::field_name("アーマーゴブリン")},
        amount: DamageAmount::Uniform {amount: Expr::Literal(Int(2))},
        flags: DamageFlags::Ability, source: ObjectView::SourceInformation {}, batch: BatchPolicy::Instruction {},
    };
    let mut board = with_producer(&initial, action);
    board.internal_rule_action(&json!({"do":"evolve","source":"armor","evolve_card":"form","pay":{"pp":1_u8}})).unwrap();
    resolve_one(&mut board, "BP13-117");
    activate(&mut board, "P1");
    finish_checks(&mut board);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["armor"]["damage"], 0_u8);
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert!(state["objects"]["armor"]["keywords"].as_array().unwrap().iter().any(|keyword| keyword == "守護"));
    activate(&mut board, "P1");
    finish_checks(&mut board);
    assert_eq!(board.projection(View::Referee).unwrap()["objects"]["armor"]["damage"], 2_u8);
}

fn pending(board: &mut Game, printing: &str) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else { panic!("printed pending choice"); };
    assert_eq!(options.len(), 1, "the intended printed ability is the only occurrence");
    assert_eq!(options[0].card, printing, "the real printing provides this ability");
    board.answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle)).unwrap()
}

#[expect(clippy::wildcard_enum_match_arm, reason = "These printed effects have only scoped selection, search and order questions.")]
fn finish_scoped(board: &mut Game, mut play: CardPlay, wanted: &str) {
    use sve_engine::game::internal::SearchAnswer;
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                assert_eq!(domain.members.len(), 1, "only the fully matching looked card is eligible");
                assert_eq!(domain.members[0].card.as_deref(), Some(wanted), "both printed traits are required");
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::Search(domain) => {
                assert_eq!(domain.cards.len(), 1, "the named real amulet is the only search result");
                assert_eq!(domain.cards[0].card, wanted, "the fanfare searches the exact printed name");
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
            }
            InputDomain::Order(cards) => Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
            _ => panic!("unexpected scoped printed choice"),
        };
        play = board.answer_internal_card(question.actor, question.token, answer).unwrap();
    }
    assert!(matches!(play, CardPlay::Resolved(_) | CardPlay::Entered(_)), "the printed effect must resolve");
    finish_checks(board);
}

#[test]
fn both_temples_look_at_three_take_only_a_card_with_both_traits_and_order_the_rest() {
    for printing in ["BP20-103", "PR-560"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"temple","card":printing}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"wanted","card":"BP20-103"}, {"id":"almost","card":"BP20-012"},
            {"id":"other","card":"BP01-139"}, {"id":"spare","card":"BP01-037"}
        ]);
        let mut board = game(&initial);
        assert!(matches!(board.play_internal_card("temple", actor()).unwrap(), CardPlay::Entered(_)));
        let play = pending(&mut board, printing);
        finish_scoped(&mut board, play, "BP20-103");
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert_eq!(state["P1"]["hand"], json!(["wanted"]));
        assert_eq!(state["P1"]["deck"], json!(["spare","other","almost"]));
    }
}

#[test]
fn both_temples_need_three_own_ex_crests_then_pay_a_quick_departure_and_reduce_one_attack() {
    for printing in ["BP20-103", "PR-560"] {
        let definition = selection().current[printing].program.card().abilities.iter()
            .find(|ability| ability.node.kind == AbilityKind::Activated).unwrap().node.id;
        for count in [2_usize, 3] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"temple","card":printing}]);
            let mut crests = ["BP20-T07", "BP20-T09", "BP20-T10"].iter().take(count).enumerate()
                .map(|(i, card)| json!({"id":format!("crest-{i}"),"card":card})).collect::<Vec<_>>();
            crests.push(json!({"id":"ordinary-ex","card":"BP01-037"}));
            initial["players"]["P1"]["zones"]["ex"] = json!(crests);
            initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"foreign","card":"BP20-T07"}]);
            initial["players"]["P2"]["zones"]["field"] = json!([
                {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}
            ]);
            if count == 3 { initial["turn"]["active"] = json!("P2"); }
            let mut board = game(&initial);
            if count == 2 {
                let before = board.projection(View::Referee).unwrap();
                let error = board.activate_internal_card("temple", definition, actor()).unwrap_err();
                assert!(error.to_string().contains("card-text use prerequisite is false"), "the own-EX crest threshold is the reason this activation is rejected");
                assert_eq!(board.projection(View::Referee).unwrap(), before);
            } else {
                attack(&mut board, "first");
                let question = board.internal_quick_choice().unwrap().unwrap();
                let InputDomain::Quick {options, ..} = &question.domain else { panic!("temple defensive Quick"); };
                let option = options.iter().find(|option| option.card == printing).unwrap();
                resolved(board.answer_internal_card(question.actor, question.token, Answer::Quick(Some(option.handle))).unwrap());
                finish_checks(&mut board);
                pass_quick(&mut board);
                let state = board.projection(View::Referee).unwrap();
                assert_eq!(state["P1"]["pp"]["current"], 9_u8);
                assert_eq!(state["P1"]["cemetery"], json!(["temple"]));
                assert_eq!(state["P1"]["leader"]["life"], 17_u8);
                attack(&mut board, "second");
                pass_quick(&mut board);
                assert_eq!(board.projection(View::Referee).unwrap()["P1"]["leader"]["life"], 12_u8);
            }
        }
    }
}

fn accept_optional_cost(board: &mut Game, play: CardPlay) -> CardPlay {
    let CardPlay::Awaiting(question) = play else { return play; };
    if matches!(question.domain, InputDomain::OptionalCost) {
        board.answer_internal_card(question.actor, question.token, Answer::OptionalCost(true)).unwrap()
    } else {
        CardPlay::Awaiting(question)
    }
}

#[test]
fn both_mountains_search_a_real_hot_spring_pay_its_counters_and_draw_before_preventing_battle_damage() {
    for printing in ["BP14-055", "BP14-SL13"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"mountain","card":printing}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"pool","card":"BP14-059"},{"id":"drawn","card":"BP01-037"},{"id":"spare","card":"BP01-037"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037"},
            {"id":"foreign-pool","card":"BP14-059","state":{"counters":{"神湯カウンター":99_u8}}}
        ]);
        let action = Action::Damage {
            recipients: RecipientSet::Objects {objects: damage_prevent::field_name("秘湯の山脈竜")},
            amount: DamageAmount::Uniform {amount: Expr::Literal(Int(1))},
            flags: DamageFlags::Ability, source: ObjectView::SourceInformation {}, batch: BatchPolicy::Instruction {},
        };
        let mut board = with_producer(&initial, action);
        assert!(matches!(board.play_internal_card("mountain", actor()).unwrap(), CardPlay::Entered(_)));
        let fanfare_play = pending(&mut board, printing);
        finish_scoped(&mut board, fanfare_play, "BP14-059");
        resolve_one(&mut board, "BP14-059");
        assert_eq!(board.projection(View::Referee).unwrap()["objects"]["pool"]["counters"]["神湯カウンター"], 10_u8);
        board.internal_rule_action(&json!({"do":"attack","attacker":"mountain","target":"enemy"})).unwrap();
        let attack_play = pending(&mut board, printing);
        let CardPlay::Awaiting(question) = accept_optional_cost(&mut board, attack_play) else { panic!("named counter payment"); };
        let InputDomain::CostMaterials(domain) = &question.domain else { panic!("counter holder choice"); };
        assert_eq!(domain.members.len(), 1);
        assert!(domain.members.iter().all(|member| member.player == actor()), "Q1910: only the own named pool can pay");
        resolved(board.answer_internal_card(question.actor, question.token, Answer::CostMaterials(vec![domain.members[0].handle])).unwrap());
        finish_checks(&mut board);
        pass_quick(&mut board);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 5_u8);
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
        assert_eq!(state["objects"]["pool"]["counters"]["神湯カウンター"], 8_u8);
        assert_eq!(state["objects"]["foreign-pool"]["counters"]["神湯カウンター"], 99_u8);
        assert_eq!(state["objects"]["mountain"]["damage"], 0_u8);
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
        activate(&mut board, "P1");
        finish_checks(&mut board);
        assert_eq!(board.projection(View::Referee).unwrap()["objects"]["mountain"]["damage"], 1_u8);
    }
}
