use super::*;
use alloc::collections::BTreeSet;
use sve_engine::game::player::{
    AdmissionGapReason, DeckConstruction, Input, PlayerDeck, StrictStartError, Viewer,
    check_card_deck_rules,
};

fn legal_deck() -> PlayerDeck {
    let selected = selection();
    let mut deck = PlayerDeck {
        construction: DeckConstruction::Class("ニュートラル".into()),
        main: vec!["BP19-110".into(); 3],
        evolve: vec!["BP01-172".into(), "CP01-085".into()],
    };
    deck.evolve.extend(vec!["BP19-111".into(); 3]);
    let mut names = BTreeSet::from(["機鋒の咎人・カットスロート".to_owned()]);
    for line in inputs().0.lines() {
        let card: Value = serde_json::from_str(line).unwrap();
        let number = card["number"].as_str().unwrap();
        let face = &card["faces"][0];
        if selected.current.contains_key(number)
            && !number.contains("-T")
            && face["card_class"] == "ニュートラル"
            && matches!(
                face["card_type"].as_str(),
                Some("フォロワー" | "スペル" | "アミュレット")
            )
            && names.insert(face["name"].as_str().unwrap().to_owned())
        {
            deck.main.push(number.to_owned());
            if deck.main.len() == 40 {
                break;
            }
        }
    }
    assert_eq!(
        deck.main.len(),
        40,
        "the real neutral pool fills a main deck"
    );
    assert!(
        deck.main.iter().any(|number| number == "BP01-171"),
        "the fixture contains the main Goblin for independent section counts"
    );
    deck
}

#[test]
fn bp19_110_strict_construction_counts_each_name_and_section_before_opening() {
    let selected = selection();
    let deck = legal_deck();
    let decks = [deck.clone(), deck.clone()];
    assert_eq!(
        check_card_deck_rules(selected, &deck).unwrap(),
        Vec::<String>::new()
    );
    assert!(
        Game::check_player_decks(selected, &decks)
            .unwrap()
            .is_admitted()
    );
    let opened = Game::start_player_game(
        selected.clone(),
        decks.clone(),
        Viewer::P1,
        "cutthroat-opening",
    )
    .unwrap();
    assert!(matches!(opened.input(Viewer::P1), Input::Question(_)));
    for (section, printing) in [
        ("main", "BP01-171"),
        ("main", "BP09-049"),
        ("evolve", "CP01-085"),
        ("evolve", "BP01-172"),
    ] {
        let mut changed = decks.clone();
        let numbers = if section == "main" {
            &mut changed[0].main
        } else {
            &mut changed[0].evolve
        };
        // Remove existing copies so every case exercises exactly two, including printed exceptions.
        numbers.retain(|number| number != printing);
        numbers.extend([printing.to_owned(), printing.to_owned()]);
        let violations = check_card_deck_rules(selected, &changed[0]).unwrap();
        assert_eq!(violations, vec![printing.to_owned()]);
        let report = Game::check_player_decks(selected, &changed).unwrap();
        assert!(
            report
                .gaps(Viewer::P1)
                .iter()
                .any(|gap| gap.card_no == printing
                    && gap.reason == AdmissionGapReason::DeckRuleViolation)
        );
        assert_eq!(report.gaps(Viewer::P2), []);
        assert!(matches!(
            Game::start_player_game(selected.clone(), changed, Viewer::P1, "cutthroat-invalid"),
            Err(StrictStartError::Admission(_))
        ));
    }
    let mut absent = deck.clone();
    absent.main.retain(|number| number != "BP19-110");
    absent.main.extend(vec!["BP09-049".into(); 5]);
    absent.evolve.extend(vec!["CP01-085".into(); 4]);
    assert_eq!(
        check_card_deck_rules(selected, &absent).unwrap(),
        Vec::<String>::new()
    );
    assert!(
        Game::check_player_decks(selected, &[absent.clone(), deck.clone()])
            .unwrap()
            .is_admitted()
    );
    absent.evolve.push("BP19-110".into());
    assert_eq!(
        check_card_deck_rules(selected, &absent).unwrap(),
        Vec::<String>::new()
    );
    let mut alias = deck;
    alias.main.retain(|number| number != "BP19-110");
    alias.main.push("BP19-SL26".into());
    alias.main.extend(["BP09-049".into(), "BP09-P15".into()]);
    let violations = check_card_deck_rules(selected, &alias).unwrap();
    assert!(violations.contains(&"BP09-049".into()));
    assert!(violations.contains(&"BP09-P15".into()));
}

#[test]
fn bp19_110_fanfare_takes_one_of_eight_and_shuffles_only_the_remainder_to_bottom() {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP19-110"}]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..10)
            .map(|i| json!({"id":format!("d{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    let mut board = Game::from_selected_effects(
        selection().clone(),
        &state,
        &Value::Null,
        &json!({"shuffles":[{"player":"P1","result":["d7","d6","d5","d4","d3","d2","d0"]}]}),
        "cutthroat-fanfare",
    )
    .unwrap();
    assert!(matches!(
        board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let pending = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare")
    };
    let option = options
        .iter()
        .find(|option| option.card == "BP19-110")
        .unwrap();
    let play = board
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap();
    let CardPlay::Awaiting(question) = play else {
        panic!("private look")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("one card")
    };
    assert_eq!(domain.members.len(), 8);
    assert_eq!(domain.cardinality.minimum, Nat(1));
    assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
    assert!(board.internal_decision(View::P2).is_none());
    let handle = domain.members[1].handle;
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![handle]),
            )
            .unwrap(),
    );
    let view = board.projection(View::Referee).unwrap();
    let hand = view["P1"]["hand"].as_array().unwrap();
    assert_eq!(hand, &vec![json!("d1")]);
    let deck = view["P1"]["deck"].as_array().unwrap();
    assert_eq!(&deck[..2], &[json!("d8"), json!("d9")]);
    assert_eq!(
        deck,
        &vec![
            json!("d8"),
            json!("d9"),
            json!("d7"),
            json!("d6"),
            json!("d5"),
            json!("d4"),
            json!("d3"),
            json!("d2"),
            json!("d0")
        ]
    );
    let moved = deck[2..]
        .iter()
        .chain(hand)
        .map(|id| id.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        moved,
        (0_u8..8)
            .map(|i| format!("d{i}"))
            .collect::<BTreeSet<_>>()
            .iter()
            .map(String::as_str)
            .collect()
    );
    let opponent = board.projection(View::P2).unwrap();
    assert!(
        !opponent["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| card["id"].as_str().is_some_and(|id| id.starts_with('d')))
    );
    assert!(board.internal_decision(View::P1).is_none());
}

#[test]
fn conditional_construction_handoff_rejects_changed_declarations_and_source_coordinates() {
    let input = &inputs().2["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|author| author["input"]["card_no"] == "BP19-110")
        .unwrap()["input"];
    for key in [
        "when",
        "decks",
        "exclude_name_contains",
        "same_name_max",
        "source",
    ] {
        let mut changed = input.clone();
        let rule = &mut changed["tree"]["cards"]["BP19-110"]["deck_rule"];
        match key {
            "when" => rule["when"] = json!("self_in_evolve"),
            "decks" => rule["decks"] = json!(["main"]),
            "exclude_name_contains" => rule[key] = json!("Another"),
            "same_name_max" => rule[key] = json!(2_u8),
            _ => changed["source_payload"]["deck_rules"][0]["source"]["ordinal"] = json!(99_u8),
        }
        assert!(lowerer().lower_current(&changed).is_err(), "{key}");
    }
}
