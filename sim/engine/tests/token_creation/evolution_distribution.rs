#![expect(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "Unexpected public questions invalidate the evolution scenarios."
)]
use super::*;
use std::sync::OnceLock;
use sve_engine::game::internal::DamageShare;
use sve_engine::game::player::{
    Command, DecisionShape, EvolutionAnswer, EvolutionQuestion, Input, MainActionAnswer,
    MainIntentKind, Question, QuestionShape, SubmissionKind, Viewer,
};
use sve_engine::ir::Nat;

fn selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut base, base_author) = author(
            "base", "11111111111111111111111111111111", "フォロワー",
            vec![json!({"evolve":{"cost":{"pp":1_u8}}})],
        );
        let (mut form, mut form_author) = author(
            "form", "22222222222222222222222222222222", "フォロワー・エボルヴ",
            vec![
                json!({"evolve_time":{"do":[{"draw":{"n":1_u8}},
                    {"create":{"token":"token","to":"field","n":2_u8}}]}}),
                json!({"super_evolve_time":{
                    "targets":[{"bind":"enemies","from":{"who":"opp","zone":"field","kind":"follower"},"any":true}],
                    "do":[{"damage":{"to":"enemies","n":{"plus":[
                        {"count":{"who":"self","zone":"field"}},
                        {"count":{"who":"self","zone":"hand"}}
                    ]},"distribute":true}}]}}),
            ],
        );
        base["faces"][0]["name"] = json!("evolution source");
        form["faces"][0]["name"] = json!("evolution source");
        form["faces"][0]["cost"] = json!("-");
        form_author["input"]["source_payload"]["faces"][0]["cost"] = Value::Null;
        let (token, token_author) = author(
            "token", "33333333333333333333333333333333", "フォロワー・トークン", vec![],
        );
        let (mut enemy, enemy_author) = author(
            "enemy", "44444444444444444444444444444444", "フォロワー", vec![],
        );
        enemy["faces"][0]["hp"] = json!("20");
        let (filler, filler_author) = author(
            "filler", "55555555555555555555555555555555", "スペル", vec![],
        );
        let snapshot = [base, form, token, enemy, filler]
            .iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
        Catalog::from_current_selection(
            &snapshot,
            &json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string(),
            &json!({"documents":[],"authors":[base_author,form_author,token_author,enemy_author,filler_author]}),
        ).unwrap()
    })
}

fn game() -> Game {
    let mut position = initial(0);
    position["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
    position["players"]["P1"]["sep"] = json!(1_u8);
    position["players"]["P1"]["zones"] = json!({
        "field":[{"id":"source","card":"base"}],
        "evolve_deck":[{"id":"form","card":"form"}],
        "hand":[{"id":"hand-a","card":"filler"},{"id":"hand-b","card":"filler"}],
        "deck":[{"id":"drawn","card":"filler"}],
        "ex":[{"id":"extra","card":"filler"}]
    });
    position["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"enemy"}]);
    Game::from_selected_effects(
        selection().clone(),
        &position,
        &Value::Null,
        &Value::Null,
        "evolve-distribution",
    )
    .unwrap()
}

fn question(game: &Game) -> Question {
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("public question")
    };
    *question
}

fn submit(game: &mut Game, command: Command) {
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied,
        "the public command applies at its declared stage"
    );
}

fn evolve(game: &mut Game, super_evolution: bool) {
    game.advance_player_input();
    let main = question(game);
    let QuestionShape::ActionWindow(actions) = main.shape else {
        panic!("main actions")
    };
    let intent = actions
        .intents
        .iter()
        .find(|i| matches!(i.kind, MainIntentKind::Evolve { .. }))
        .unwrap();
    submit(
        game,
        Command::MainAction {
            token: main.token,
            intent: intent.id.clone(),
            answer: MainActionAnswer::None,
        },
    );
    let forms = question(game);
    let QuestionShape::Evolution(EvolutionQuestion::Form(form_domain)) = forms.shape else {
        panic!("form")
    };
    assert_eq!(form_domain.len(), 1, "one matching evolution resource");
    submit(
        game,
        Command::Evolution {
            token: forms.token,
            answer: EvolutionAnswer::Form(form_domain[0].id),
        },
    );
    let payment = question(game);
    let QuestionShape::Evolution(EvolutionQuestion::Payment(payment_domain)) = payment.shape else {
        panic!("payment")
    };
    assert_eq!(
        payment_domain.total_pp, 1,
        "the printed intrinsic cost is one PP"
    );
    assert!(
        payment_domain.super_allowed,
        "the setup allows super evolution"
    );
    submit(
        game,
        Command::Evolution {
            token: payment.token,
            answer: EvolutionAnswer::Payment {
                pp: payment_domain.total_pp,
                ep: false,
                super_evolution,
            },
        },
    );
}

fn answer(game: &mut Game, answer: Answer) {
    submit(
        game,
        Command::Answer {
            token: question(game).token,
            answer,
        },
    );
}

fn pending(game: &mut Game, line: u64, expected: usize) {
    let current = question(game);
    let QuestionShape::Decision(DecisionShape::Pending(options)) = current.shape else {
        panic!("pending evolution text")
    };
    assert_eq!(
        options.len(),
        expected,
        "only unresolved printed lines remain"
    );
    let selected = options.iter().find(|o| o.line == line).unwrap();
    answer(game, Answer::Pending(selected.handle));
}

fn distribution(game: &mut Game, total: u64, empty: bool) {
    let targets = question(game);
    let QuestionShape::Decision(DecisionShape::Targets(target_domain)) = targets.shape else {
        panic!("targets")
    };
    assert_eq!(
        target_domain.members.len(),
        1,
        "one opposing follower is available"
    );
    answer(
        game,
        Answer::Targets(if empty {
            vec![]
        } else {
            vec![target_domain.members[0].handle]
        }),
    );
    if !empty {
        let allocation = question(game);
        let QuestionShape::Decision(DecisionShape::Distribution(allocation_domain)) =
            allocation.shape
        else {
            panic!("distribution")
        };
        assert_eq!(
            allocation_domain.total,
            Nat(total),
            "the chosen pending order fixes the Hand plus Field total"
        );
        answer(
            game,
            Answer::Distribution(vec![DamageShare {
                handle: allocation_domain.members[0].handle,
                amount: Nat(total),
            }]),
        );
    }
}

fn assert_finished(game: &Game, damage: u64, super_evolution: bool) {
    assert!(
        matches!(question(game).shape, QuestionShape::ActionWindow(_)),
        "all pending lines finish and return priority"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["field"].as_array().unwrap().len(),
        3,
        "the source and two created tokens remain"
    );
    assert_eq!(
        state["P1"]["hand"],
        json!(["hand-a", "hand-b", "drawn"]),
        "the normal line draws exactly one card"
    );
    assert_eq!(
        state["P1"]["ex"],
        json!(["extra"]),
        "EX is unchanged and excluded from the total"
    );
    assert_eq!(
        state["objects"]["source"]["evolved_with"], "form",
        "the selected evolution resource is attached"
    );
    assert_eq!(
        state["objects"]["source"]["super_evolved"], super_evolution,
        "the chosen evolution kind is preserved"
    );
    assert_eq!(
        state["objects"]["enemy"]["damage"], damage,
        "only the chosen distribution total is dealt"
    );
}

#[test]
fn super_evolution_distribution_reads_hand_and_field_in_the_chosen_pending_order() {
    for normal_first in [false, true] {
        let mut game = game();
        evolve(&mut game, true);
        if normal_first {
            pending(&mut game, 1, 2);
        }
        pending(&mut game, 2, if normal_first { 1 } else { 2 });
        let total = if normal_first { 6 } else { 3 };
        distribution(&mut game, total, false);
        if !normal_first {
            pending(&mut game, 1, 1);
        }
        assert_finished(&game, total, true);
    }
}

#[test]
fn ordinary_evolution_only_resolves_draw_and_field_tokens() {
    let mut game = game();
    evolve(&mut game, false);
    pending(&mut game, 1, 1);
    assert_finished(&game, 0, false);
}

#[test]
fn zero_super_targets_skip_distribution_and_preserve_the_other_pending() {
    let mut game = game();
    evolve(&mut game, true);
    pending(&mut game, 2, 2);
    distribution(&mut game, 3, true);
    pending(&mut game, 1, 1);
    assert_finished(&game, 0, true);
}
