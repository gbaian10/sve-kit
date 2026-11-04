#![expect(
    clippy::panic,
    reason = "The complete printed Advance scenarios require their declared private and token choices."
)]
use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::DecisionView;

const PRINTINGS: [(&str, &str); 3] = [
    ("BP14-018", "BP14-019"),
    ("BP14-SL04", "BP14-SL05"),
    ("BP22-R03", "BP22-R04"),
];

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-advance-entry",
    )
    .unwrap()
}

fn ability(card: &str, index: usize) -> AbilityId {
    selection_with_rules().current[card]
        .program
        .card()
        .abilities[index]
        .node
        .id
}

fn position(base: &str, form: &str, feast: u8, royal: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":3_u8,"max":4_u8});
    initial["players"]["P1"]["ep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":base},{"id":"second","card":base}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"incoming","card":form}]);
    let mut grave = (0..feast)
        .map(|n| json!({"id":format!("feast-{n}"),"card":"BP14-025"}))
        .collect::<Vec<_>>();
    grave.extend((0..royal).map(
        |n| json!({"id":format!("royal-{n}"),"card":if n == 0 {"CSD03a-007"} else {"CP03-037"}}),
    ));
    initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"CP03-013","state":{"hp":20_u8,"max_hp":20_u8,"power":0_u8}}
    ]);
    initial
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed decision");
    };
    q
}

fn checks(board: &mut Game) {
    if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn trigger(board: &mut Game, card: &str, line: u64) -> CardPlay {
    checks(board);
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending trigger");
    };
    let handle = options
        .iter()
        .find(|o| o.card == card && o.line == line)
        .unwrap()
        .handle;
    board
        .answer_internal_card(q.actor, q.token, Answer::Pending(handle))
        .unwrap()
}

fn ward_choices(board: &mut Game, mut play: CardPlay) {
    let mut wards = 0_usize;
    while let CardPlay::Awaiting(q) = play {
        assert!(
            matches!(q.domain, InputDomain::WardEntry { .. }),
            "each token asks for Ward entry only"
        );
        wards = wards.checked_add(1).unwrap();
        play = board
            .answer_internal_card(q.actor, q.token, Answer::WardEntry(true))
            .unwrap();
    }
    resolved(play);
    assert_eq!(
        wards, 2,
        "each real token's printed Ward has its own entry decision"
    );
    checks(board);
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "all immediate printed triggers have resolved"
    );
}

fn advance(board: &mut Game, base: &str, form: &str, accept: bool) {
    let q = awaiting(
        board
            .activate_internal_card("source", ability(base, 0), PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::ResourceEntryPayment {
        materials,
        ep_allowed,
    } = &q.domain
    else {
        panic!("Advance payment");
    };
    assert_eq!(
        materials.members.len(),
        0,
        "Advance cannot consume resource materials"
    );
    assert!(
        *ep_allowed,
        "the available EP may replace one of the four PP"
    );
    let q = awaiting(
        board
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::ResourceEntryPayment {
                    materials: vec![],
                    use_ep: true,
                },
            )
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("private named Advance");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the printed eligible object is offered"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some(form),
        "the selected resource is the printed named Advance form"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "putting the named Advance onto the field is optional"
    );
    assert!(
        board.internal_decision(View::P2).is_none(),
        "the unselected evolve deck remains private"
    );
    let handles = if accept {
        vec![domain.members[0].handle]
    } else {
        vec![]
    };
    resolved(
        board
            .answer_internal_card(q.actor, q.token, Answer::Targets(handles))
            .unwrap(),
    );
}

#[test]
fn every_taketsumi_printing_draws_discards_and_creates_the_real_gold() {
    for (base, _) in PRINTINGS {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"] = json!({"current":2_u8,"max":2_u8});
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":base}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"drawn","card":"CP03-037"},{"id":"tail","card":"CP03-037"}
        ]);
        let mut board = game(&initial);
        assert!(
            matches!(
                board
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the real two-PP base enters before its fanfare"
        );
        let q = awaiting(trigger(&mut board, base, 2));
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("the drawn card must be discarded");
        };
        assert_eq!(
            domain.members.len(),
            1,
            "only the printed eligible object is offered"
        );
        assert_eq!(
            domain.members[0].card.as_deref(),
            Some("CP03-037"),
            "the just-drawn follower is the only hand card"
        );
        resolved(
            board
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
        );
        checks(&mut board);
        assert!(
            board.internal_pending_choice().unwrap().is_none(),
            "all immediate printed triggers have resolved"
        );
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"], 0_u8,
            "the printed PP payment is fully consumed"
        );
        assert_eq!(
            view["P1"]["hand"],
            json!([]),
            "the fanfare discards the drawn card"
        );
        assert_eq!(
            view["P1"]["cemetery"],
            json!(["drawn"]),
            "the drawn follower reaches the cemetery"
        );
        assert_eq!(
            view["P1"]["deck"],
            json!(["tail"]),
            "the fanfare draws exactly one card"
        );
        let ex = view["P1"]["ex"].as_array().unwrap();
        assert_eq!(ex.len(), 1, "the fanfare creates exactly one EX token");
        let token = &view["objects"][ex[0].as_str().unwrap()];
        assert_eq!(
            token["name"], "輝く金貨",
            "the full real gold template is used"
        );
    }
}

#[test]
fn every_advance_form_enters_with_two_real_ward_tokens_and_uses_its_printed_damage() {
    for (base, form) in PRINTINGS {
        let mut board = game(&position(base, form, 5, 0));
        advance(&mut board, base, form, true);
        let play = trigger(&mut board, form, 1);
        ward_choices(&mut board, play);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"], 0_u8,
            "the printed PP payment is fully consumed"
        );
        assert_eq!(view["P1"]["ep"], 0_u8, "one EP replaces one printed PP");
        assert_eq!(
            view["P1"]["banish"],
            json!(["source"]),
            "the base pays its printed self-banish"
        );
        assert_eq!(
            view["objects"]["incoming"]["generation"], 1_u8,
            "the actual Advance entry creates one new generation"
        );
        let tokens = view["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|id| {
                let object = &view["objects"][id.as_str().unwrap()];
                (object["name"] == "旅籠煤").then_some(object)
            })
            .collect::<Vec<_>>();
        assert_eq!(tokens.len(), 2, "the true form creates two real inn tokens");
        for token in tokens {
            assert_eq!(
                token["acted"], true,
                "accepting Ward enters that token acted"
            );
            assert!(
                token["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("守護")),
                "the real token retains its printed Ward"
            );
        }
        let q = awaiting(
            board
                .activate_internal_card("incoming", ability(form, 1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("opposing follower target");
        };
        assert_eq!(
            domain.members.len(),
            1,
            "only the printed eligible object is offered"
        );
        resolved(
            board
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
        );
        checks(&mut board);
        let damaged_view = board.projection(View::Referee).unwrap();
        assert_eq!(
            damaged_view["objects"]["incoming"]["acted"], true,
            "the printed activation acts the true form"
        );
        assert_eq!(
            damaged_view["objects"]["enemy"]["hp"], 15_u8,
            "the formal follower takes exactly five damage"
        );
        assert_eq!(
            damaged_view["P2"]["leader"]["life"], 17_u8,
            "that followers leader takes exactly three damage"
        );
    }
}

#[test]
fn cemetery_card_thresholds_and_declining_entry_keep_the_printed_payment_and_quota() {
    for (feast, royal, legal) in [(4, 0, false), (0, 9, false), (5, 0, true), (0, 10, true)] {
        let base = "BP14-018";
        let mut initial = position(base, "BP14-019", feast, royal);
        initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
        let mut board = game(&initial);
        if !legal {
            let before = board.projection(View::Referee).unwrap();
            let error = board
                .activate_internal_card("source", ability(base, 0), PlayerId { seat: 1 })
                .unwrap_err();
            assert!(
                matches!(error, EngineFailure::IllegalPlay(_)),
                "the cemetery card prerequisite fails before payment"
            );
            assert_eq!(
                board.projection(View::Referee).unwrap(),
                before,
                "a failed prerequisite pays nothing"
            );
            assert!(
                board.internal_decision(View::P1).is_none(),
                "a failed prerequisite asks no payment question"
            );
            continue;
        }
        advance(&mut board, base, "BP14-019", false);
        checks(&mut board);
        assert!(
            board.internal_pending_choice().unwrap().is_none(),
            "all immediate printed triggers have resolved"
        );
        let before = board.projection(View::Referee).unwrap();
        assert_eq!(
            before["P1"]["pp"]["current"], 4_u8,
            "declining entry does not refund the three paid PP"
        );
        assert_eq!(
            before["P1"]["ep"], 0_u8,
            "declining entry does not refund the paid EP"
        );
        assert_eq!(
            before["P1"]["banish"],
            json!(["source"]),
            "declining entry does not undo self-banish"
        );
        assert_eq!(
            before["objects"]["incoming"]["generation"], 0_u8,
            "an unchosen resource never enters the field"
        );
        let error = board
            .activate_internal_card("second", ability(base, 0), PlayerId { seat: 1 })
            .unwrap_err();
        assert!(
            matches!(error, EngineFailure::IllegalPlay(_)),
            "a second Advance is forbidden despite sufficient remaining PP"
        );
        assert_eq!(
            board.projection(View::Referee).unwrap(),
            before,
            "the remaining four PP suffice, but the shared evolution quota is spent, Q1878"
        );
    }
}

#[test]
fn an_opposing_live_aura_suppresses_the_advance_fanfare_without_refunding_entry_costs() {
    for (base, form) in PRINTINGS {
        let mut initial = position(base, form, 5, 0);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP22-R18","state":{"hp":20_u8,"max_hp":20_u8,"power":0_u8}}
        ]);
        let mut board = game(&initial);
        advance(&mut board, base, form, true);
        checks(&mut board);
        assert!(
            board.internal_pending_choice().unwrap().is_none(),
            "the live opposing aura prevents the Advance form's fanfare occurrence"
        );
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["field"],
            json!(["second", "incoming"]),
            "the Advance enters normally but its prohibited fanfare creates no tokens"
        );
        assert_eq!(
            view["P1"]["pp"]["current"], 0_u8,
            "suppressing a later trigger does not refund entry PP"
        );
        assert_eq!(
            view["P1"]["ep"], 0_u8,
            "suppressing a later trigger does not refund EP"
        );
        assert_eq!(
            view["P1"]["banish"],
            json!(["source"]),
            "the base still pays its printed self-banish"
        );
        assert_eq!(
            view["objects"]["incoming"]["generation"], 1_u8,
            "a prohibited fanfare does not undo the actual field entry"
        );
    }
}
