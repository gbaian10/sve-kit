#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed quota scenarios."
)]
use super::*;

fn selected() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, kind, power, hp, traits) in [
            ("quota-machine", "スペル", "-", "-", json!(["機械"])),
            ("quota-amulet", "アミュレット", "-", "-", json!([])),
            ("quota-enemy", "フォロワー", "0", "99", json!([])),
        ] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_type":kind,"card_class":"ニュートラル","cost":"1","power":power,"hp":hp,"traits":traits,"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn machine_spell() -> WellFormedCardProgram {
    let mut card = producer_steps(vec![]).card().clone();
    let located = card.abilities.first_mut().unwrap();
    located.source.card_id = "quota-machine".into();
    located.source.card_no = "quota-machine".into();
    let ability = &mut located.node;
    ability.kind = AbilityKind::Spell;
    ability.active_zones = vec![ZoneKind::Resolution];
    ability.play.as_mut().unwrap().costs = CostSpec::Ordered {
        steps: vec![CostStep {
            keyword: None,
            materials: None,
            instruction: Box::new(Instruction {
                id: InstructionId(1),
                receipt: ReceiptId(1),
                actor: PlayerRef::Controller {},
                source: ability.source.clone(),
                precondition: None,
                context: InstructionContext::Cost,
                action: Action::Resource {
                    player: PlayerRef::Controller {},
                    resource: Resource::PpCurrent,
                    operation: ResourceOperation::Pay,
                    amount: Expr::Literal(Int(1)),
                },
            }),
        }],
    };
    WellFormedCardProgram::check(card).unwrap()
}

fn board(initial: &Value) -> Game {
    let mut board = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-quota",
    )
    .unwrap();
    board
        .load_internal_card("quota-machine", &machine_spell())
        .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["quota-amulet", "quota-enemy"] {
        board.load_internal_card(number, &empty).unwrap();
    }
    board
}

fn finish(board: &mut Game) {
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn pending(board: &mut Game, number: &str, expected: bool) {
    finish(board);
    let question = board.internal_pending_choice().unwrap();
    if !expected {
        assert!(question.is_none(), "{number} has no quota left");
        return;
    }
    let question = question.unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending");
    };
    assert_eq!(options.len(), 1, "one admitted occurrence per play");
    assert_eq!(
        options[0].card, number,
        "the seed card is the trigger source"
    );
    let mut play = board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(declaration) = play {
        let InputDomain::Targets(domain) = &declaration.domain else {
            panic!("formal target");
        };
        assert_eq!(
            domain.members.len(),
            1,
            "the only enemy follower is the target"
        );
        play = board
            .answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
    }
    resolved(play);
    finish(board);
}

#[test]
fn bp07_036_evolved_tetra_damages_after_four_machine_plays_and_ignores_the_fifth() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP07-035","state":{"evolved":true,"evolved_with":"form","power":4_u8,"hp":4_u8,"max_hp":4_u8}}]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"BP07-036"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0_u8..5)
            .map(|n| json!({"id":format!("machine{n}"),"card":"quota-machine"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"quota-enemy"}]);
    let mut board = board(&initial);
    for n in 0_u8..5 {
        resolved(
            board
                .play_internal_card(&format!("machine{n}"), PlayerId { seat: 1 })
                .unwrap(),
        );
        pending(&mut board, "BP07-036", n < 4);
    }
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 95_u8);
    assert_eq!(
        state["P1"]["pp"]["current"], 10_u8,
        "the evolved static discount applies to each EX machine spell"
    );
}

#[test]
fn bp11_001_restores_pp_for_three_amulet_plays_then_stops() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP11-001"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!(
        (0_u8..4)
            .map(|n| json!({"id":format!("amulet{n}"),"card":"quota-amulet"}))
            .collect::<Vec<_>>()
    );
    let mut board = board(&initial);
    for n in 0_u8..4 {
        assert!(matches!(
            board
                .play_internal_card(&format!("amulet{n}"), PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        pending(&mut board, "BP11-001", n < 3);
    }
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        9_u8
    );
}

#[test]
fn bp22_019_stands_after_ten_combat_hits_and_stays_acted_after_the_eleventh() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP22-019","state":{"entered_this_turn":false}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"quota-enemy"}]);
    let mut board = board(&initial);
    for n in 0_u8..11 {
        board
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap();
        pending(&mut board, "BP22-019", true);
        let question = board.internal_quick_choice().unwrap().unwrap();
        assert!(matches!(
            board
                .answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ));
        pending(&mut board, "BP22-019", n < 10);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
            n >= 10
        );
    }
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 55_u8);
    assert_eq!(state["objects"]["source"]["hp"], 19_u8);
}
