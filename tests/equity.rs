//! Integration and exact all-in equity tests (SPEC_v3.7 §7, M16).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
mod support;

use std::fs;
use std::path::Path;
use std::str::FromStr;

use pretty_assertions::assert_eq;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tempfile::NamedTempFile;

use common::{
    EV_CASHOUT, EV_CASHOUT_CALLER, EV_CASHOUT_FLOP, EV_CASHOUT_RECEIVED, FOUR_WAY_ALL_IN,
    SAMPLE_SESSION, THREE_WAY_ALL_IN_SIDE_POT, gg,
};
use poker_tracker::domain::{ActionType, Card, CommunityCards, Street};
use poker_tracker::parser::HandParser;
use poker_tracker::report::{ReportKind, ReportOptions, write_report};
use poker_tracker::stats::{derive_hand_stats, exact_equity};
use support::{make_action, make_canonical_hand, make_player};

fn card(s: &str) -> Card {
    Card::from_str(s).unwrap()
}

// ============================================================================
// §7.1 Evaluator unit tests
// ============================================================================

#[test]
fn eval_unit_tests() {
    use poker_tracker::stats::eval::evaluate;

    // 1. Categories beat one another
    let hc = evaluate(&[card("Ac"), card("Kd"), card("Jh"), card("8s"), card("4c")]);
    let pair = evaluate(&[card("Ac"), card("Ad"), card("Jh"), card("8s"), card("4c")]);
    let two_pair = evaluate(&[card("Ac"), card("Ad"), card("Kh"), card("Kd"), card("4c")]);
    let trips = evaluate(&[card("Ac"), card("Ad"), card("Ah"), card("Kd"), card("4c")]);
    let straight = evaluate(&[card("Ac"), card("Kd"), card("Qs"), card("Jh"), card("Tc")]);
    let flush = evaluate(&[card("Ac"), card("Kc"), card("Jc"), card("8c"), card("4c")]);
    let full_house = evaluate(&[card("Ac"), card("Ad"), card("Ah"), card("Kd"), card("Kc")]);
    let quads = evaluate(&[card("Ac"), card("Ad"), card("Ah"), card("As"), card("4c")]);
    let straight_flush = evaluate(&[card("Ac"), card("Kc"), card("Qc"), card("Jc"), card("Tc")]);

    assert!(hc < pair, "pair beats high card");
    assert!(pair < two_pair, "two pair beats pair");
    assert!(two_pair < trips, "trips beats two pair");
    assert!(trips < straight, "straight beats trips");
    assert!(straight < flush, "flush beats straight");
    assert!(flush < full_house, "full house beats flush");
    assert!(full_house < quads, "quads beats full house");
    assert!(quads < straight_flush, "straight flush beats quads");

    // 2. Wheel straight (A-2-3-4-5) is the lowest straight (5-high < 6-high)
    let wheel = evaluate(&[card("Ac"), card("2d"), card("3h"), card("4s"), card("5c")]);
    let six_straight = evaluate(&[card("2d"), card("3h"), card("4s"), card("5c"), card("6s")]);
    assert!(wheel < six_straight, "wheel is lower than 6-high straight");

    // 3. Steel wheel (A-2-3-4-5 suited) is the lowest straight flush
    let steel_wheel = evaluate(&[card("Ac"), card("2c"), card("3c"), card("4c"), card("5c")]);
    let six_sf = evaluate(&[card("2c"), card("3c"), card("4c"), card("5c"), card("6c")]);
    assert!(
        steel_wheel < six_sf,
        "steel wheel is lower than 6-high straight flush"
    );

    // 4. Split pot: board plays for both players
    let p1 = evaluate(&[
        card("2c"),
        card("3d"),
        card("Jh"),
        card("Th"),
        card("9s"),
        card("8c"),
        card("7d"),
    ]);
    let p2 = evaluate(&[
        card("4c"),
        card("5d"),
        card("Jh"),
        card("Th"),
        card("9s"),
        card("8c"),
        card("7d"),
    ]);
    assert_eq!(p1, p2, "board straight plays for both players (split)");

    // 5. 5-, 6-, and 7-card inputs agree when extra cards do not improve the best 5-card hand
    let h5 = evaluate(&[card("Ac"), card("Kd"), card("Jh"), card("8s"), card("4c")]);
    let h6 = evaluate(&[
        card("Ac"),
        card("Kd"),
        card("Jh"),
        card("8s"),
        card("4c"),
        card("2d"),
    ]);
    let h7 = evaluate(&[
        card("Ac"),
        card("Kd"),
        card("Jh"),
        card("8s"),
        card("4c"),
        card("2d"),
        card("3h"),
    ]);
    assert_eq!(h5, h6, "5-card and 6-card hand agree");
    assert_eq!(h6, h7, "6-card and 7-card hand agree");
}

// ============================================================================
// §7.2 Exact equities from eval7 oracle
// ============================================================================

#[test]
fn exact_equities_and_none_cases() {
    // 1. Ah As vs Kc Kd (empty board): 29603/36432, 6829/36432
    let eq1 = exact_equity(&[[card("Ah"), card("As")], [card("Kc"), card("Kd")]], &[]).unwrap();
    assert_eq!(
        eq1,
        vec![(29603, 36432), (6829, 36432)],
        "Ah As vs Kc Kd preflop equity"
    );

    // 2. Ac Kd vs Ah Kc (empty board): 862441/1712304, 849863/1712304
    let eq2 = exact_equity(&[[card("Ac"), card("Kd")], [card("Ah"), card("Kc")]], &[]).unwrap();
    assert_eq!(
        eq2,
        vec![(862441, 1712304), (849863, 1712304)],
        "Ac Kd vs Ah Kc preflop equity"
    );

    // 3. As Ks vs Qh Qd vs Jc Tc (empty board): 3 players
    let eq3 = exact_equity(
        &[
            [card("As"), card("Ks")],
            [card("Qh"), card("Qd")],
            [card("Jc"), card("Tc")],
        ],
        &[],
    )
    .unwrap();
    assert_eq!(
        eq3,
        vec![(61241, 152306), (281378, 685377), (256829, 1370754)],
        "As Ks vs Qh Qd vs Jc Tc 3-way preflop equity"
    );

    // 4. Ah Ad vs Kh Kd on Ks 7c 2d (flop): 17/198, 181/198 (PT4 documented example)
    let eq4 = exact_equity(
        &[[card("Ah"), card("Ad")], [card("Kh"), card("Kd")]],
        &[card("Ks"), card("7c"), card("2d")],
    )
    .unwrap();
    assert_eq!(
        eq4,
        vec![(17, 198), (181, 198)],
        "Ah Ad vs Kh Kd on Ks 7c 2d flop equity"
    );

    // None cases:
    // Fewer than 2 players
    assert_eq!(exact_equity(&[[card("Ah"), card("As")]], &[]), None);
    // More than 5 board cards
    assert_eq!(
        exact_equity(
            &[[card("Ah"), card("As")], [card("Kc"), card("Kd")]],
            &[
                card("2c"),
                card("3c"),
                card("4c"),
                card("5c"),
                card("6c"),
                card("7c")
            ]
        ),
        None
    );
    // Repeated card
    assert_eq!(
        exact_equity(&[[card("Ah"), card("As")], [card("Ah"), card("Kd")]], &[]),
        None
    );
    assert_eq!(
        exact_equity(
            &[[card("Ah"), card("As")], [card("Kc"), card("Kd")]],
            &[card("Ah")]
        ),
        None
    );
}

// ============================================================================
// §7.3 The four real EV Cashout hands
// ============================================================================

#[test]
fn four_real_ev_cashout_hands() {
    let parser = gg();

    // 1. HD140653161 (EV_CASHOUT_CALLER): turn cashout 19/22, pot after 4.46, cashout 3.8, ratio 0.9865
    let h1 = parser.parse_hand(EV_CASHOUT_CALLER).unwrap();
    let s1 = derive_hand_stats(&h1);
    // Cashing player is c7bdd1e4
    let cashing1 = s1.iter().find(|s| s.player_name == "c7bdd1e4").unwrap();
    assert_eq!(cashing1.ev_qualified, 1);
    assert_eq!(
        (cashing1.ev_equity_num, cashing1.ev_equity_den),
        (19, 22),
        "turn all-in equity 19/22"
    );
    // Verify cashout ratio on turn
    let pot_after1 = dec!(4.46);
    let cashout1 = dec!(3.8);
    let ratio1 = cashout1 / (pot_after1 * Decimal::from(19) / Decimal::from(22));
    assert!(
        ratio1 >= dec!(0.98) && ratio1 <= dec!(1.00),
        "ratio1 in [0.98, 1.00]: {ratio1}"
    );

    // 2. HD140647649 (EV_CASHOUT_FLOP): flop cashout 857/990, pot after 3.78, cashout 3.23, ratio 0.9871
    let h2 = parser.parse_hand(EV_CASHOUT_FLOP).unwrap();
    let s2 = derive_hand_stats(&h2);
    // Cashing player is 62688fe9
    let cashing2 = s2.iter().find(|s| s.player_name == "62688fe9").unwrap();
    assert_eq!(cashing2.ev_qualified, 1);
    // All-in was preflop: 1223075 / 1712304
    assert_eq!(
        (cashing2.ev_equity_num, cashing2.ev_equity_den),
        (1223075, 1712304),
        "preflop all-in equity"
    );
    // Cashout was on the flop: check flop equity directly with exact_equity
    let flop_board2 = h2.boards[0].flop.unwrap();
    let p_cashing2 = h2.players.iter().find(|p| p.name == "62688fe9").unwrap();
    let p_opp2 = h2.players.iter().find(|p| p.name == "dc7d0eb4").unwrap();
    let flop_eq2 = exact_equity(
        &[
            [
                p_cashing2.hole_cards[0].unwrap(),
                p_cashing2.hole_cards[1].unwrap(),
            ],
            [p_opp2.hole_cards[0].unwrap(), p_opp2.hole_cards[1].unwrap()],
        ],
        &flop_board2,
    )
    .unwrap();
    assert_eq!(
        flop_eq2[0],
        (857, 990),
        "cashing player flop equity 857/990"
    );
    let pot_after2 = dec!(3.78);
    let cashout2 = dec!(3.23);
    let ratio2 = cashout2 / (pot_after2 * Decimal::from(857) / Decimal::from(990));
    assert!(
        ratio2 >= dec!(0.98) && ratio2 <= dec!(1.00),
        "ratio2 in [0.98, 1.00]: {ratio2}"
    );

    // 3. HD140642980 (EV_CASHOUT): turn cashout 37/44, pot after 2.81, cashout 2.33, ratio 0.9861
    let h3 = parser.parse_hand(EV_CASHOUT).unwrap();
    let s3 = derive_hand_stats(&h3);
    // Cashing player is 1020a9fa
    let cashing3 = s3.iter().find(|s| s.player_name == "1020a9fa").unwrap();
    assert_eq!(cashing3.ev_qualified, 1);
    // All-in was flop: 541 / 990
    assert_eq!(
        (cashing3.ev_equity_num, cashing3.ev_equity_den),
        (541, 990),
        "flop all-in equity"
    );
    // Cashout was on turn: turn equity directly with exact_equity
    let flop3 = h3.boards[0].flop.unwrap();
    let turn3 = h3.boards[0].turn.unwrap();
    let board3 = [flop3[0], flop3[1], flop3[2], turn3];
    let p_cashing3 = h3.players.iter().find(|p| p.name == "1020a9fa").unwrap();
    let p_opp3 = h3.players.iter().find(|p| p.name == "79b6d419").unwrap();
    let turn_eq3 = exact_equity(
        &[
            [
                p_cashing3.hole_cards[0].unwrap(),
                p_cashing3.hole_cards[1].unwrap(),
            ],
            [p_opp3.hole_cards[0].unwrap(), p_opp3.hole_cards[1].unwrap()],
        ],
        &board3,
    )
    .unwrap();
    assert_eq!(turn_eq3[0], (37, 44), "cashing player turn equity 37/44");
    let pot_after3 = dec!(2.81);
    let cashout3 = dec!(2.33); // 2.81 payout - 0.48 risk paid = 2.33
    let ratio3 = cashout3 / (pot_after3 * Decimal::from(37) / Decimal::from(44));
    assert!(
        ratio3 >= dec!(0.98) && ratio3 <= dec!(1.00),
        "ratio3 in [0.98, 1.00]: {ratio3}"
    );

    // 4. HD145319803 (EV_CASHOUT_RECEIVED): flop cashout 346/495, pot after 9.93, cashout 6.87, ratio 0.9898
    let h4 = parser.parse_hand(EV_CASHOUT_RECEIVED).unwrap();
    let s4 = derive_hand_stats(&h4);
    let cashing4 = s4.iter().find(|s| s.player_name == "81ae8569").unwrap();
    let opp4 = s4.iter().find(|s| s.player_name == "a2939c73").unwrap();
    assert_eq!(cashing4.ev_qualified, 1);
    assert_eq!(opp4.ev_qualified, 1);
    assert_eq!(
        (cashing4.ev_equity_num, cashing4.ev_equity_den),
        (346, 495),
        "cashing player equity 346/495"
    );
    assert_eq!(
        (opp4.ev_equity_num, opp4.ev_equity_den),
        (149, 495),
        "opponent equity 149/495"
    );
    assert_eq!(opp4.ev_net_micros, -2_010_970, "opponent ev_net_micros");
    assert_eq!(
        cashing4.ev_net_micros, 1_870_000,
        "cashing player keeps actual net_micros"
    );

    let pot_after4 = dec!(9.93);
    let cashout4 = dec!(6.87);
    let ratio4 = cashout4 / (pot_after4 * Decimal::from(346) / Decimal::from(495));
    assert!(
        ratio4 >= dec!(0.98) && ratio4 <= dec!(1.00),
        "ratio4 in [0.98, 1.00]: {ratio4}"
    );
}

// ============================================================================
// §7.4 Qualification
// ============================================================================

#[test]
fn qualification_and_hero_twelve_hands() {
    let parser = gg();

    // 1. Four side-pot exclusions: HD145383901, HD140643875, HD140650460, HD140652493
    for raw in [
        FOUR_WAY_ALL_IN,
        THREE_WAY_ALL_IN_SIDE_POT,
        SAMPLE_SESSION.split("\n\n").nth(1).unwrap(),
    ] {
        let h = parser.parse_hand(raw).unwrap();
        let s = derive_hand_stats(&h);
        for p in s {
            assert_eq!(
                p.ev_qualified, 0,
                "side-pot hand player {} must not qualify",
                p.player_name
            );
        }
    }

    // 2. Count qualifying hands in corpus: 28 in v3.6, 13 in v3.5 -> 41 total
    let v36_dir = Path::new("tests/fixtures/ggpoker/v3_6");
    let mut v36_paths: Vec<_> = fs::read_dir(v36_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    v36_paths.sort();

    let mut v36_qual = 0;
    let mut v36_hero_in = 0;
    let mut all_hero_hands = Vec::new();
    let mut hd140652493_checked = false;

    for path in &v36_paths {
        let content = fs::read_to_string(path).unwrap();
        for (_, res) in parser.parse_session(&content) {
            let h = res.unwrap();
            let stats = derive_hand_stats(&h);
            if h.header.site_hand_id == "HD140652493" {
                for p in &stats {
                    assert_eq!(
                        p.ev_qualified, 0,
                        "HD140652493 player {} must not qualify",
                        p.player_name
                    );
                }
                hd140652493_checked = true;
            }
            let hand_qualified = stats.iter().any(|s| s.ev_qualified == 1);
            if hand_qualified {
                v36_qual += 1;
            }
            if let Some(hero) = stats.iter().find(|s| {
                h.players
                    .iter()
                    .find(|p| p.is_hero)
                    .map(|p| p.name.as_str())
                    == Some(s.player_name.as_str())
            }) && hero.ev_qualified == 1
            {
                v36_hero_in += 1;
                all_hero_hands.push((h.header.site_hand_id.clone(), hero.clone()));
            }
        }
    }
    assert!(
        hd140652493_checked,
        "HD140652493 checked for side-pot exclusion"
    );
    assert_eq!(v36_qual, 28, "v3.6 folder has exactly 28 qualifying hands");
    assert_eq!(v36_hero_in, 9, "hero is in 9 qualifying hands in v3.6");

    let v35_path = Path::new("tests/fixtures/ggpoker/NLHRed15_2026-08-21.txt");
    let v35_content = fs::read_to_string(v35_path).unwrap();
    let mut v35_qual = 0;
    let mut v35_hero_in = 0;
    for (_, res) in parser.parse_session(&v35_content) {
        let h = res.unwrap();
        let stats = derive_hand_stats(&h);
        if stats.iter().any(|s| s.ev_qualified == 1) {
            v35_qual += 1;
        }
        if let Some(hero) = stats.iter().find(|s| {
            h.players
                .iter()
                .find(|p| p.is_hero)
                .map(|p| p.name.as_str())
                == Some(s.player_name.as_str())
        }) && hero.ev_qualified == 1
        {
            v35_hero_in += 1;
            all_hero_hands.push((h.header.site_hand_id.clone(), hero.clone()));
        }
    }
    assert_eq!(v35_qual, 13, "v3.5 file has exactly 13 qualifying hands");
    assert_eq!(v35_hero_in, 3, "hero is in 3 qualifying hands in v3.5");
    assert_eq!(
        v36_qual + v35_qual,
        41,
        "corpus has 41 qualifying hands total"
    );
    assert_eq!(
        all_hero_hands.len(),
        12,
        "hero has 12 qualifying hands total"
    );

    // 3. Assert Hero's 12 hands exactly as given in §7.4
    let expected_hero: &[(&str, (u64, u64), i64)] = &[
        ("HD140645133", (41, 1980), -3_940_556),
        ("HD140643576", (1, 1), 9_430_000),
        ("HD140652930", (812323, 1141536), 1_514_565),
        ("HD140654227", (49342, 107019), -191_780),
        ("HD140655678", (1, 1), 10_500_000),
        ("HD140654333", (253495, 311328), 1_265_093),
        ("HD140646342", (5, 22), -5_690_909),
        ("HD145383838", (21, 22), 5_523_182),
        ("HD145411928", (19, 36), 53_611),
        ("HD140647703", (32, 33), 1_212_727),
        ("HD140647294", (1, 2), -70_000),
        ("HD140643125", (881, 990), 1_048_323),
    ];

    for (id, (exp_num, exp_den), exp_ev_net) in expected_hero {
        let found = all_hero_hands
            .iter()
            .find(|(hid, _)| hid == id)
            .unwrap_or_else(|| panic!("hero qualifying hand {id} missing"));
        assert_eq!(
            (found.1.ev_equity_num as u64, found.1.ev_equity_den as u64),
            (*exp_num, *exp_den),
            "hand {id} equity"
        );
        assert_eq!(
            found.1.ev_net_micros, *exp_ev_net,
            "hand {id} ev_net_micros"
        );
    }
}

// ============================================================================
// §7.5 Equity-adjusted results
// ============================================================================

#[test]
fn equity_adjusted_results_per_file_and_reports() {
    let parser = gg();
    let v36_dir = Path::new("tests/fixtures/ggpoker/v3_6");
    let mut v36_paths: Vec<_> = fs::read_dir(v36_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    v36_paths.sort();

    // Table from §7.5:
    let expected_files = [
        ("GG20260821-2054 - NLHRed14", 0, 0, dec!(7.75), dec!(7.75)),
        (
            "GG20260821-2054 - NLHRed18",
            5,
            2,
            dec!(-17.71),
            dec!(-17.550556),
        ),
        (
            "GG20260821-2106 - NLHRed4",
            4,
            1,
            dec!(-3.03),
            dec!(2.584565),
        ),
        (
            "GG20260821-2207 - NLHRed10",
            8,
            1,
            dec!(3.78),
            dec!(0.91822),
        ),
        (
            "GG20260821-2247 - NLHRed14",
            4,
            2,
            dec!(-1),
            dec!(-1.744907),
        ),
        ("GG20260821-2252 - NLHRed19", 1, 0, dec!(9.41), dec!(9.41)),
        (
            "GG20260821-2254 - NLHRed7",
            2,
            1,
            dec!(-17.48),
            dec!(-13.170909),
        ),
        (
            "GG20260925-0500 - NLHRed21",
            1,
            1,
            dec!(-3.25),
            dec!(-3.806818),
        ),
        (
            "GG20260925-0539 - NLHRed3",
            2,
            1,
            dec!(3.45),
            dec!(3.923611),
        ),
        ("GG20260925-0618 - NLHRed15", 1, 0, dec!(-7.71), dec!(-7.71)),
    ];

    let mut all_v36_hands = Vec::new();

    for (path, (prefix, exp_qual, exp_hin, exp_net, exp_ev)) in v36_paths.iter().zip(expected_files)
    {
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(prefix)
        );
        let content = fs::read_to_string(path).unwrap();
        let mut qual = 0;
        let mut hero_in = 0;
        let mut hero_net_micros = 0i64;
        let mut hero_ev_micros = 0i64;

        for (_, res) in parser.parse_session(&content) {
            let h = res.unwrap();
            let stats = derive_hand_stats(&h);
            if stats.iter().any(|s| s.ev_qualified == 1) {
                qual += 1;
            }
            if let Some(hero_s) = stats.iter().find(|s| {
                h.players
                    .iter()
                    .find(|p| p.is_hero)
                    .map(|p| p.name.as_str())
                    == Some(s.player_name.as_str())
            }) {
                let actual_net = h
                    .checked_player_results()
                    .unwrap()
                    .into_iter()
                    .find(|r| r.player_name == hero_s.player_name)
                    .unwrap()
                    .net;
                hero_net_micros +=
                    poker_tracker::domain::decimal_to_micro_units(actual_net).unwrap();
                hero_ev_micros += hero_s.ev_net_micros;
                if hero_s.ev_qualified == 1 {
                    hero_in += 1;
                }
            }
            all_v36_hands.push(h);
        }

        assert_eq!(qual, exp_qual, "qualifying hands in {prefix}");
        assert_eq!(hero_in, exp_hin, "hero in qualifying in {prefix}");
        assert_eq!(
            Decimal::new(hero_net_micros, 6).normalize(),
            exp_net.normalize(),
            "hero net in {prefix}"
        );
        assert_eq!(
            Decimal::new(hero_ev_micros, 6).normalize(),
            exp_ev.normalize(),
            "hero ev net in {prefix}"
        );
    }

    // Reports on v3.6 folder
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = poker_tracker::storage::SqliteRepository::open(temp_db.path()).unwrap();
    repo.insert_hands_batch(&all_v36_hands).unwrap();

    let mut res_buf = Vec::new();
    write_report(
        &repo,
        ReportKind::Results,
        &ReportOptions::default(),
        &mut res_buf,
    )
    .unwrap();
    let res_str = String::from_utf8(res_buf).unwrap();
    let res_line = res_str.lines().nth(1).unwrap();
    assert_eq!(
        res_line,
        "CAD,0.05/0.1,989,-25.79,-2.61,-257.90,-26.08,14.23115,2.500423,29.25,0,9,-19.396794,-19.61"
    );

    // Check sessions report 10 ev_net values
    let mut sess_buf = Vec::new();
    write_report(
        &repo,
        ReportKind::Sessions,
        &ReportOptions::default(),
        &mut sess_buf,
    )
    .unwrap();
    let sess_str = String::from_utf8(sess_buf).unwrap();
    let sess_lines: Vec<&str> = sess_str.lines().skip(1).collect();
    let expected_ev_nets = [
        "7.75",
        "-17.550556",
        "2.584565",
        "0.91822",
        "-1.744907",
        "9.41",
        "-13.170909",
        "-3.806818",
        "3.923611",
        "-7.71",
    ];
    assert_eq!(sess_lines.len(), 10);
    for (line, &exp_ev) in sess_lines.iter().zip(&expected_ev_nets) {
        let last_col = line.split(',').next_back().unwrap();
        assert_eq!(last_col, exp_ev, "session ev_net in line {line}");
    }
}

// ============================================================================
// Canonical near-miss tests (no site text)
// ============================================================================

#[test]
fn canonical_all_in_on_river_does_not_qualify() {
    let mut p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(10), false),
    ];
    p[0].hole_cards = [Some(card("Ah")), Some(card("Kd"))];
    p[1].hole_cards = [Some(card("Qc")), Some(card("Qd"))];

    let a = vec![
        make_action(
            0,
            Street::Preflop,
            "Hero",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "Villain",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Call,
            dec!(0.05),
            dec!(0.10),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "Villain",
            ActionType::Check,
            dec!(0),
            dec!(0.10),
            false,
        ),
        make_action(
            4,
            Street::Flop,
            "Hero",
            ActionType::Check,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            5,
            Street::Flop,
            "Villain",
            ActionType::Check,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            6,
            Street::Turn,
            "Hero",
            ActionType::Check,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            7,
            Street::Turn,
            "Villain",
            ActionType::Check,
            dec!(0),
            dec!(0),
            false,
        ),
        // River all-in
        make_action(
            8,
            Street::River,
            "Hero",
            ActionType::Bet,
            dec!(9.90),
            dec!(9.90),
            true,
        ),
        make_action(
            9,
            Street::River,
            "Villain",
            ActionType::Call,
            dec!(9.90),
            dec!(9.90),
            true,
        ),
    ];

    let board = vec![CommunityCards {
        run_index: 0,
        flop: Some([card("2s"), card("3s"), card("4c")]),
        turn: Some(card("9h")),
        river: Some(card("Jd")),
    }];

    let mut h = make_canonical_hand(1, p, a, board, vec![("Hero", dec!(20.00))]);
    h.outcome.total_pot = dec!(20.00);

    let stats = derive_hand_stats(&h);
    assert_eq!(stats[0].ev_qualified, 0, "river all in must not qualify");
    assert_eq!(stats[1].ev_qualified, 0, "river all in must not qualify");
}

#[test]
fn canonical_live_player_with_unknown_hole_cards_does_not_qualify() {
    let mut p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(10), false),
    ];
    p[0].hole_cards = [Some(card("Ah")), Some(card("Kd"))];
    p[1].hole_cards = [None, None]; // Villain unknown

    let a = vec![
        make_action(
            0,
            Street::Preflop,
            "Hero",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "Villain",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Raise,
            dec!(9.95),
            dec!(10.00),
            true,
        ),
        make_action(
            3,
            Street::Preflop,
            "Villain",
            ActionType::Call,
            dec!(9.90),
            dec!(10.00),
            true,
        ),
    ];

    let mut h = make_canonical_hand(1, p, a, vec![], vec![("Hero", dec!(20.00))]);
    h.outcome.total_pot = dec!(20.00);

    let stats = derive_hand_stats(&h);
    assert_eq!(
        stats[0].ev_qualified, 0,
        "unknown live hole cards must not qualify"
    );
    assert_eq!(
        stats[1].ev_qualified, 0,
        "unknown live hole cards must not qualify"
    );
}

#[test]
fn canonical_unequal_live_contributions_do_not_qualify() {
    let mut p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Short", dec!(5), false),
    ];
    p[0].hole_cards = [Some(card("Ah")), Some(card("Kd"))];
    p[1].hole_cards = [Some(card("Qc")), Some(card("Qd"))];

    // Hero raised to 10, Short called 5 and is all in; no uncalled bet returned (unequal investment: Hero invested 10, Short 5)
    let a_unequal = vec![
        make_action(
            0,
            Street::Preflop,
            "Hero",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "Short",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Raise,
            dec!(9.95),
            dec!(10.00),
            true,
        ),
        make_action(
            3,
            Street::Preflop,
            "Short",
            ActionType::Call,
            dec!(4.90),
            dec!(5.00),
            true,
        ),
    ];

    let mut h = make_canonical_hand(1, p, a_unequal, vec![], vec![("Hero", dec!(15.00))]);
    h.outcome.total_pot = dec!(15.00);

    let stats = derive_hand_stats(&h);
    assert_eq!(
        stats[0].ev_qualified, 0,
        "unequal live contributions must not qualify"
    );
    assert_eq!(
        stats[1].ev_qualified, 0,
        "unequal live contributions must not qualify"
    );
}

#[test]
fn canonical_dead_blind_excluded_from_live_contribution() {
    let mut p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(10), false),
    ];
    p[0].hole_cards = [Some(card("Ah")), Some(card("Kd"))];
    p[1].hole_cards = [Some(card("Qc")), Some(card("Qd"))];

    // Villain posts dead blind of 0.10, so invested is 10.10. Hero invested 10.00.
    // Live contributions: Hero 10.00, Villain 10.10 - 0.10 = 10.00. Equal -> qualifies!
    let a = vec![
        make_action(
            0,
            Street::Preflop,
            "Hero",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "Villain",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Villain",
            ActionType::PostDeadBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "Hero",
            ActionType::Raise,
            dec!(9.95),
            dec!(10.00),
            true,
        ),
        make_action(
            4,
            Street::Preflop,
            "Villain",
            ActionType::Call,
            dec!(9.90),
            dec!(10.00),
            true,
        ),
    ];

    let mut h = make_canonical_hand(1, p, a, vec![], vec![("Hero", dec!(20.10))]);
    h.outcome.total_pot = dec!(20.10);

    let stats = derive_hand_stats(&h);
    assert_eq!(
        stats[0].ev_qualified, 1,
        "dead blind excluded makes live contribution equal"
    );
    assert_eq!(
        stats[1].ev_qualified, 1,
        "dead blind excluded makes live contribution equal"
    );
}

#[test]
fn canonical_folded_player_known_cards_does_not_change_equity() {
    // 3 players: Hero, Villain live; Folder folds preflop
    let mut p_unknown = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(10), false),
        make_player(3, "Folder", dec!(10), false),
    ];
    p_unknown[0].hole_cards = [Some(card("Ah")), Some(card("As"))];
    p_unknown[1].hole_cards = [Some(card("Kc")), Some(card("Kd"))];
    p_unknown[2].hole_cards = [None, None];

    let a = vec![
        make_action(
            0,
            Street::Preflop,
            "Folder",
            ActionType::Fold,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "Hero",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Villain",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "Hero",
            ActionType::Raise,
            dec!(9.95),
            dec!(10.00),
            true,
        ),
        make_action(
            4,
            Street::Preflop,
            "Villain",
            ActionType::Call,
            dec!(9.90),
            dec!(10.00),
            true,
        ),
    ];

    let mut h1 = make_canonical_hand(1, p_unknown, a.clone(), vec![], vec![("Hero", dec!(20.00))]);
    h1.outcome.total_pot = dec!(20.00);
    let s1 = derive_hand_stats(&h1);

    // Now give Folder known cards (e.g. Kh Ks, which would affect equity if removed from deck)
    let mut p_known = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(10), false),
        make_player(3, "Folder", dec!(10), false),
    ];
    p_known[0].hole_cards = [Some(card("Ah")), Some(card("As"))];
    p_known[1].hole_cards = [Some(card("Kc")), Some(card("Kd"))];
    p_known[2].hole_cards = [Some(card("Qh")), Some(card("Qs"))];

    let mut h2 = make_canonical_hand(1, p_known, a, vec![], vec![("Hero", dec!(20.00))]);
    h2.outcome.total_pot = dec!(20.00);
    let s2 = derive_hand_stats(&h2);

    assert_eq!(
        (s1[0].ev_equity_num, s1[0].ev_equity_den),
        (s2[0].ev_equity_num, s2[0].ev_equity_den),
        "folded player cards must not affect live player equity"
    );
    assert_eq!(s1[0].ev_net_micros, s2[0].ev_net_micros);
}

#[test]
fn canonical_multi_run_hand_uses_first_run_board() {
    let mut p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(10), false),
    ];
    p[0].hole_cards = [Some(card("Ah")), Some(card("Kd"))];
    p[1].hole_cards = [Some(card("8c")), Some(card("8d"))];

    let a = vec![
        make_action(
            0,
            Street::Preflop,
            "Hero",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "Villain",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Flop,
            "Hero",
            ActionType::Bet,
            dec!(9.95),
            dec!(10.00),
            true,
        ),
        make_action(
            3,
            Street::Flop,
            "Villain",
            ActionType::Call,
            dec!(9.90),
            dec!(10.00),
            true,
        ),
    ];

    // Multi-run board: run 1 has flop 2c 3c 4c; run 2 has flop Ts Js Qs
    let boards = vec![
        CommunityCards {
            run_index: 1,
            flop: Some([card("2c"), card("3c"), card("4c")]),
            turn: Some(card("5h")),
            river: Some(card("6d")),
        },
        CommunityCards {
            run_index: 2,
            flop: Some([card("Ts"), card("Js"), card("Qs")]),
            turn: Some(card("Ks")),
            river: Some(card("As")),
        },
    ];

    let mut h = make_canonical_hand(1, p.clone(), a.clone(), boards, vec![("Hero", dec!(20.00))]);
    h.outcome.total_pot = dec!(20.00);

    let stats = derive_hand_stats(&h);
    assert_eq!(stats[0].ev_qualified, 1);

    // Compute equity with run 1's flop: 2c 3c 4c
    let exp_eq = exact_equity(
        &[[card("Ah"), card("Kd")], [card("8c"), card("8d")]],
        &[card("2c"), card("3c"), card("4c")],
    )
    .unwrap();

    assert_eq!(
        (stats[0].ev_equity_num as u64, stats[0].ev_equity_den as u64),
        exp_eq[0],
        "multi-run hand uses first run board"
    );
}

#[test]
fn canonical_three_way_all_in_qualifies_with_exact_equity() {
    let mut p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain1", dec!(10), false),
        make_player(3, "Villain2", dec!(10), false),
    ];
    // Hole cards matching §7.2 exact equities example
    p[0].hole_cards = [Some(card("As")), Some(card("Ks"))];
    p[1].hole_cards = [Some(card("Qh")), Some(card("Qd"))];
    p[2].hole_cards = [Some(card("Jc")), Some(card("Tc"))];

    let a = vec![
        make_action(
            0,
            Street::Preflop,
            "Hero",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "Villain1",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Villain2",
            ActionType::Raise,
            dec!(10.00),
            dec!(10.00),
            true,
        ),
        make_action(
            3,
            Street::Preflop,
            "Hero",
            ActionType::Call,
            dec!(9.95),
            dec!(10.00),
            true,
        ),
        make_action(
            4,
            Street::Preflop,
            "Villain1",
            ActionType::Call,
            dec!(9.90),
            dec!(10.00),
            true,
        ),
    ];

    let mut h = make_canonical_hand(1, p, a, vec![], vec![("Hero", dec!(30.00))]);
    h.outcome.total_pot = dec!(30.00);

    let stats = derive_hand_stats(&h);

    // All 3 live players must qualify with exact equities from §7.2
    assert_eq!(stats[0].ev_qualified, 1);
    assert_eq!(stats[1].ev_qualified, 1);
    assert_eq!(stats[2].ev_qualified, 1);

    assert_eq!(
        (stats[0].ev_equity_num, stats[0].ev_equity_den),
        (61241, 152306)
    );
    assert_eq!(
        (stats[1].ev_equity_num, stats[1].ev_equity_den),
        (281378, 685377)
    );
    assert_eq!(
        (stats[2].ev_equity_num, stats[2].ev_equity_den),
        (256829, 1370754)
    );

    // One half-even rounding of pot after deductions (30.00) minus invested (10.00)
    assert_eq!(stats[0].ev_net_micros, 2_062_755);
    assert_eq!(stats[1].ev_net_micros, 2_316_346);
    assert_eq!(stats[2].ev_net_micros, -4_379_101);

    // Conserves total EV net = 0
    assert_eq!(
        stats[0].ev_net_micros + stats[1].ev_net_micros + stats[2].ev_net_micros,
        0
    );
}

#[test]
fn exact_equity_four_way_preflop() {
    let hole_cards = [
        [card("Ah"), card("As")],
        [card("Kh"), card("Ks")],
        [card("Qh"), card("Qd")],
        [card("Jh"), card("Jd")],
    ];
    let equities = exact_equity(&hole_cards, &[]).unwrap();
    assert_eq!(equities.len(), 4);

    // Sum of equity fractions must equal exactly 1
    let sum_equity: Decimal = equities
        .iter()
        .map(|&(num, den)| Decimal::from(num) / Decimal::from(den))
        .sum();
    assert_eq!(sum_equity, Decimal::ONE, "4-way equities sum to 1");

    // Aces > Kings > Queens > Jacks
    let p0 = Decimal::from(equities[0].0) / Decimal::from(equities[0].1);
    let p1 = Decimal::from(equities[1].0) / Decimal::from(equities[1].1);
    let p2 = Decimal::from(equities[2].0) / Decimal::from(equities[2].1);
    let p3 = Decimal::from(equities[3].0) / Decimal::from(equities[3].1);
    assert!(p0 > p1, "Aces beat Kings in equity");
    assert!(p1 > p2, "Kings beat Queens in equity");
    assert!(p2 > p3, "Queens beat Jacks in equity");
}
