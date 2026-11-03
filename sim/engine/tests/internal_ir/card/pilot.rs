use super::super::pilot::{board, pilot};
use super::*;

fn loaded(bodies: Vec<Vec<Located<Node>>>) -> Game {
    let mut game = board("アミュレット・トークン", "3", "3");
    let abilities = bodies
        .into_iter()
        .zip(1_u64..)
        .map(|(steps, line)| {
            let mut located = ability(line, steps);
            located.source = source("unit-spell");
            located.source.fragment = None;
            located.source.line = u32::try_from(line).unwrap();
            for (step, ordinal) in located
                .node
                .body
                .as_mut()
                .unwrap()
                .steps
                .iter_mut()
                .zip(1_u64..)
            {
                if let Node::Instruction { instruction } = &mut step.node {
                    let id = line.saturating_mul(10).saturating_add(ordinal);
                    instruction.id = InstructionId(id);
                    instruction.receipt = ReceiptId(id);
                }
            }
            let origin = located.node.source.clone();
            locate_steps(
                &mut located.node.body.as_mut().unwrap().steps,
                &located.source,
                &origin,
            );
            located
        })
        .collect();
    game.load_internal_card("unit-spell", &card(abilities))
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game
}

fn activate(game: &mut Game, line: u64) {
    resolved(
        game.activate_internal_card("s", AbilityId(line), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

fn end(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(game);
}

#[test]
fn pilot_preserves_granted_aura_through_expiry_and_the_next_pilot() {
    let mut game = loaded(vec![
        vec![pilot(1), grant_keyword(1, KeywordId::Aura)],
        vec![pilot(1)],
    ]);
    activate(&mut game, 1);
    let granted = game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"].clone();
    assert_eq!(granted, json!(["オーラ"]));
    end(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
        granted
    );
    end(&mut game);
    activate(&mut game, 2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
        granted
    );
}

#[test]
fn piloted_token_leaving_field_is_still_erased_as_a_token() {
    let mut game = loaded(vec![vec![
        pilot(1),
        movement::put(
            2,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Cemetery,
        ),
    ]]);
    activate(&mut game, 1);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["objects"]["s"].is_null(),
        "the token special kind survives pilot"
    );
    assert_eq!(state["P1"]["cemetery"], json!([]));
}

#[test]
fn pilot_lowering_rejects_other_refs_explicit_stats_and_durations() {
    for (payload, accepted) in [
        (json!({"ref":"this"}), true),
        (json!({"ref":"other"}), false),
        (json!({"ref":"this","on":"pilot"}), false),
        (json!({"ref":"this","attack":3,"life":3}), false),
        (json!({"ref":"this","until":"game"}), false),
    ] {
        let mut tree = fanfare::expanded::tree().clone();
        let mut seed = tree["cards"]["BP01-041"].clone();
        seed["abilities"][0]["activate"] = json!({"cost":{"pp":0},"do":[{"pilot":payload}]});
        tree["cards"] = json!({"unit-amulet":seed});
        let result = lowerer().lower(&tree);
        assert_eq!(result.is_ok(), accepted, "lowering result: {result:?}");
    }
}

fn grant_keyword(ability: u64, keyword: KeywordId) -> Located<Node> {
    instruction(
        2,
        Action::Grant {
            subjects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            abilities: vec![GrantedAbility::Keyword { keyword }],
            duration: Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            provider: TextProvider {
                provider: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                face: FaceId("0".into()),
                ability: AbilityId(ability),
            },
        },
    )
}

#[test]
fn pilot_preserves_entry_sickness_until_storm_is_granted() {
    let mut game = loaded(vec![
        vec![pilot(1)],
        vec![grant_keyword(2, KeywordId::Storm)],
    ]);
    activate(&mut game, 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["entered_this_turn"],
        true
    );
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "pilot does not clear entry sickness"
    );
    activate(&mut game, 2);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}
