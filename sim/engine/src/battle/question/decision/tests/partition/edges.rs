use super::*;

#[test]
fn native_empty_segments_skip_and_singletons_still_require_the_empty_remainder_bucket() {
    for actor in [Viewer::P1, Viewer::P2] {
        let empty = make(actor, &[], "unit-spell");
        let next = question(&empty, actor).unwrap();
        assert!(
            !matches!(domain(&next), Some(DecisionShape::SearchPartition(_))),
            "empty native segments do not invent a question"
        );
        assert_eq!(
            empty.projection(View::Referee).unwrap()[seats(actor).0]["leader"]["life"],
            21_u32
        );
        let mut game = make(actor, &["unit-follower"], "unit-spell");
        let (native, items, buckets, binding) = projected(&game, actor, "singleton");
        assert_eq!(items.len(), 1);
        assert_eq!(
            buckets.last().unwrap().bounds,
            wire::Bounds {
                min: WireNat::ZERO,
                max: Some(WireNat::ZERO)
            }
        );
        let complete = answer(&items, &[0]);
        let mut missing = complete.clone();
        let Answer::Partition {
            buckets: incomplete_buckets,
        } = &mut missing
        else {
            panic!("partition");
        };
        incomplete_buckets.remove(0);
        assert_eq!(binding.decode(&missing), Err(AnswerError));
        submit_exact(&mut game, &native, &binding, &complete);
        assert_eq!(
            game.projection(View::Referee).unwrap()[seats(actor).0]["hand"],
            json!(["deck-0"])
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[seats(actor).0]["leader"]["life"],
            21_u32
        );
    }
}

#[test]
fn search_capacity_stops_capture_the_actual_field_or_ex_and_leave_unselected_cards_in_deck() {
    for actor in [Viewer::P1, Viewer::P2] {
        for (zone, wire_zone, count, available) in [
            ("ex", Zone::Ex, 6_u32, 5_usize),
            ("field", Zone::Field, 5_u32, 4_usize),
        ] {
            let mut procedure = json!({"sel":{"who":"self","zone":"deck"},"n":count,"to":zone});
            if zone == "field" {
                procedure["sel"]["kind"] = json!("follower");
            }
            let cards = vec!["unit-follower"; usize::try_from(count).unwrap()];
            let mut game = begin(actor, &procedure, &cards, "unit-spell");
            let (native, items, buckets, binding) = projected(&game, actor, "capacity");
            let selected = buckets.first().unwrap();
            assert_eq!(
                selected.destination,
                Some(wire::Destination {
                    player: seat(actor),
                    zone: wire_zone,
                    position: None
                })
            );
            assert_eq!(selected.bounds.min, WireNat::try_from(available).unwrap());
            assert_eq!(selected.bounds.max, Some(selected.bounds.min));
            assert_eq!(buckets.last().unwrap().destination, None);
            assert_eq!(buckets.last().unwrap().bounds.min, WireNat::from(1_u32));
            let chosen = (0..available).collect::<Vec<_>>();
            submit_exact(&mut game, &native, &binding, &answer(&items, &chosen));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[seats(actor).0]["deck"].as_array().unwrap().len(), 1);
            assert_eq!(state[seats(actor).0][zone].as_array().unwrap().len(), 5);
            assert_eq!(state[seats(actor).0]["leader"]["life"], 21_u32);
        }
    }
}
