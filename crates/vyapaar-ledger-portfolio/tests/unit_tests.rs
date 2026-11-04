//! Portfolio unit tests.
//!
//! Ported from the `honba-kokoro-portfolio` inline `#[cfg(test)]` block (the
//! zero-`cfg(test)` gate requires every test to live in `tests/`, migration
//! guide Fix 3) and rewritten against the canonical [`vyapaar_core`]
//! vocabulary. Temporal determinism: the three settlement-engine tests that
//! used to stamp fills with `Utc::now()` now use a fixed timestamp.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use rust_decimal::Decimal;
use vyapaar_core::{Currency, Exchange, InstrumentId};
use vyapaar_ledger_portfolio::{
    AccountState, CapitalGainType, ClearingCalendar, CorporateAction, DematHoldings, Fill,
    HeldPosition, Marks, PortfolioState, PositionSide, Price, Product, Quantity, SettlementEngine,
    TaxSummary, Trade, adjust_for_bonus, adjust_for_split, apply_fill_to_account,
    compute_capital_gains, record_dividend, settle_trades, value,
};

fn reliance() -> InstrumentId {
    InstrumentId::new(Exchange::Nse, "RELIANCE")
}

/// Fixed stamp injected by the tests, standing in for the deleted
/// `chrono::Utc::now()` fill timestamps (temporal determinism).
fn fixed_timestamp() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 6, 1, 9, 15, 0).unwrap()
}

fn single_account_state(acct: &str, account: AccountState) -> PortfolioState {
    PortfolioState {
        base_currency: Currency::Inr,
        accounts: [(acct.to_string(), account)].into_iter().collect(),
    }
}

#[test]
fn test_fill_reduces_long() {
    let id = reliance();
    let mut acc = AccountState::new(Currency::Inr, vec![]);
    let pos = HeldPosition::new(
        id.clone(),
        PositionSide::Long,
        Quantity::new(Decimal::from(10)),
        Price::from_rupees("2000").unwrap(),
    );
    acc.positions.push(pos);
    let mut state = single_account_state("acct1", acc);

    // Fill 4 units at 2015 - should reduce position to 6 and realize PnL
    let pnl = apply_fill_to_account(
        state.accounts.get_mut("acct1").unwrap(),
        id,
        PositionSide::Long,
        4,
        Price::from_rupees("2015").unwrap(),
    );
    assert!(pnl.amount > Decimal::from(0)); // 4 * (2015 - 2000) = 60 INR
    // Position should now have 6 units
    let pos = &state.accounts["acct1"].positions[0];
    assert_eq!(pos.quantity.0, Decimal::from(6));
}

#[test]
fn test_fill_full_close_long() {
    let id = reliance();
    let mut acc = AccountState::new(Currency::Inr, vec![]);
    let pos = HeldPosition::new(
        id.clone(),
        PositionSide::Long,
        Quantity::new(Decimal::from(10)),
        Price::from_rupees("2000").unwrap(),
    );
    acc.positions.push(pos);
    let mut state = single_account_state("acct1", acc);

    // Fill 10 units at 2050 - should close position fully and realize PnL
    let pnl = apply_fill_to_account(
        state.accounts.get_mut("acct1").unwrap(),
        id,
        PositionSide::Long,
        10,
        Price::from_rupees("2050").unwrap(),
    );
    assert!(pnl.amount > Decimal::from(0)); // 10 * (2050 - 2000) = 500 INR
    // Position should now have 0 units
    let pos = &state.accounts["acct1"].positions[0];
    assert_eq!(pos.quantity.0, Decimal::from(0));
    // entry_lot_ids should be cleared
    assert!(pos.entry_lot_ids.is_empty());
}

#[test]
fn test_entry_date_tracking() {
    let id = reliance();
    let pos = HeldPosition::new(
        id,
        PositionSide::Long,
        Quantity::new(Decimal::from(10)),
        Price::from_rupees("2000").unwrap(),
    );
    // There is no clock in the library: `HeldPosition::new` stamps the
    // deterministic 2026-01-01 epoch date, which callers replace via
    // `with_entry_date` when the real date is known.
    assert!(pos.entry_date >= NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    assert!(pos.entry_date <= NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
}

#[test]
fn test_trade_dates() {
    let id = reliance();
    let trade = Trade {
        trade_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        settle_date: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
        instrument: id,
        side: PositionSide::Long,
        quantity: 10,
        price: Price::from_rupees("2000").unwrap(),
        cash_flow: 20_000,
    };

    // Current date is after settle_date
    let current_date = NaiveDate::from_ymd_opt(2026, 1, 3).unwrap();

    let mut state = single_account_state("acct1", AccountState::new(Currency::Inr, vec![]));

    let result = settle_trades(&mut state, &[trade], current_date);

    // Should have settled successfully
    assert!(result.successful);
    // At least one account should be affected
    assert!(!result.affected_accounts.is_empty());
}

#[test]
fn test_compute_capital_gains_short_term() {
    let id = reliance();
    // Position held for ~91 days at sale time (short term).
    let pos = HeldPosition::new(
        id,
        PositionSide::Long,
        Quantity::new(Decimal::from(10)),
        Price::from_rupees("1500").unwrap(),
    )
    .with_entry_date(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let sale_date = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
    let (pnl, gain_type) =
        compute_capital_gains(&pos, 5, Price::from_rupees("2000").unwrap(), sale_date);
    assert!(pnl.amount > Decimal::from(0));
    // With ~91 days holding, should be short term
    assert!(matches!(gain_type, CapitalGainType::ShortTerm));
}

#[test]
fn test_tax_summary_from_fills() {
    let id = reliance();

    // Create account with a position held for long term
    let mut acc = AccountState::new(Currency::Inr, vec![]);
    let pos = HeldPosition::new(
        id.clone(),
        PositionSide::Long,
        Quantity::new(Decimal::from(10)),
        Price::from_rupees("1500").unwrap(),
    );
    acc.positions.push(pos);

    let state = single_account_state("acct1", acc);

    // Simulate a long-term fill (sale after > 365 days holding):
    // entry_date is the 2026-01-01 epoch date, and fill_date is 2027-01-01.
    let fills: [(
        &AccountState,
        InstrumentId,
        PositionSide,
        u64,
        Price,
        NaiveDate,
    ); 1] = [(
        &state.accounts["acct1"],
        id,
        PositionSide::Long,
        5,
        Price::from_rupees("2000").unwrap(),
        NaiveDate::from_ymd_opt(2027, 1, 1).unwrap(),
    )];

    let tax_summary = TaxSummary::from_fills(
        &fills,
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2027, 12, 31).unwrap(),
        &[], // No corporate actions
    );

    // Should have some realized PnL
    assert!(tax_summary.total_realized_pnl.amount >= Decimal::from(0));
    assert!(tax_summary.period_end >= tax_summary.period_start);
}

// ————————————————————————————————————————
// Corporate Actions Tests
// ———————————————————————————————————

#[test]
fn test_adjust_for_split() {
    let id = reliance();
    let mut pos = HeldPosition::new(
        id,
        PositionSide::Long,
        Quantity::new(Decimal::from(10)),
        Price::from_rupees("2000").unwrap(),
    );

    // 1:2 split - 1 share becomes 2 shares, price halves
    adjust_for_split(&mut pos, 2, 1);

    // Quantity should double
    assert_eq!(pos.quantity.0, Decimal::from(20));
    // Entry price should halve
    assert_eq!(pos.entry_price.0, Decimal::from(1000));
}

#[test]
fn test_adjust_for_bonus() {
    let id = reliance();
    let mut pos = HeldPosition::new(
        id,
        PositionSide::Long,
        Quantity::new(Decimal::from(10)),
        Price::from_rupees("2000").unwrap(),
    );

    // 1:1 bonus - 1 free share for every 1 held (100% bonus)
    adjust_for_bonus(&mut pos, 1, 1);

    // Quantity should double (10 + 10 bonus = 20)
    assert_eq!(pos.quantity.0, Decimal::from(20));
    // Entry price should halve to keep total cost basis same
    assert_eq!(pos.entry_price.0, Decimal::from(1000));
}

#[test]
fn test_record_dividend() {
    let id = reliance();
    let pos = HeldPosition::new(
        id,
        PositionSide::Long,
        Quantity::new(Decimal::from(100)),
        Price::from_rupees("2000").unwrap(),
    );

    // Dividend of 5 INR per share (500 paise)
    let dividend = record_dividend(&pos, 500);

    // 100 shares * 5 INR = 500 INR
    assert_eq!(dividend.amount, Decimal::from(50000));
}

#[test]
fn test_corporate_action_split_before_fill() {
    let id = reliance();

    // Create account with position
    let mut acc = AccountState::new(Currency::Inr, vec![]);
    let pos = HeldPosition::new(
        id.clone(),
        PositionSide::Long,
        Quantity::new(Decimal::from(10)),
        Price::from_rupees("2000").unwrap(),
    );
    acc.positions.push(pos);

    let state = single_account_state("acct1", acc);

    // Corporate action: 1:2 split on 2026-06-01 (ex-date)
    let actions = vec![CorporateAction::Split {
        ratio_numerator: 2,
        ratio_denominator: 1,
        ex_date: NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
    }];

    // Fill on 2026-07-01 (after split)
    let fills: [(
        &AccountState,
        InstrumentId,
        PositionSide,
        u64,
        Price,
        NaiveDate,
    ); 1] = [(
        &state.accounts["acct1"],
        id,
        PositionSide::Long,
        5,
        Price::from_rupees("1100").unwrap(),
        NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
    )];

    let tax_summary = TaxSummary::from_fills(
        &fills,
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
        &actions,
    );

    // After 1:2 split: position qty = 20, entry_price = 1000
    // Selling 5 shares at 1100: PnL = 5 * (1100 - 1000) = 500 INR
    assert_eq!(tax_summary.total_sold_qty, 5);
    assert!(tax_summary.total_realized_pnl.amount > Decimal::from(0));
}

#[test]
fn test_corporate_action_bonus_before_fill() {
    let id = reliance();

    // Create account with position
    let mut acc = AccountState::new(Currency::Inr, vec![]);
    let pos = HeldPosition::new(
        id.clone(),
        PositionSide::Long,
        Quantity::new(Decimal::from(10)),
        Price::from_rupees("2000").unwrap(),
    );
    acc.positions.push(pos);

    let state = single_account_state("acct1", acc);

    // Corporate action: 1:1 bonus on 2026-06-01
    let actions = vec![CorporateAction::Bonus {
        ratio_numerator: 1,
        ratio_denominator: 1,
        ex_date: NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
    }];

    // Fill on 2026-07-01 (after bonus)
    let fills: [(
        &AccountState,
        InstrumentId,
        PositionSide,
        u64,
        Price,
        NaiveDate,
    ); 1] = [(
        &state.accounts["acct1"],
        id,
        PositionSide::Long,
        10,
        Price::from_rupees("1100").unwrap(),
        NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
    )];

    let tax_summary = TaxSummary::from_fills(
        &fills,
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
        &actions,
    );

    // After 1:1 bonus: position qty = 20, entry_price = 1000
    // Selling 10 shares at 1100: PnL = 10 * (1100 - 1000) = 1000 INR
    assert_eq!(tax_summary.total_sold_qty, 10);
    assert!(tax_summary.total_realized_pnl.amount > Decimal::from(0));
}

#[test]
fn test_corporate_action_dividend() {
    let id = reliance();

    // Create account with position
    let mut acc = AccountState::new(Currency::Inr, vec![]);
    let pos = HeldPosition::new(
        id.clone(),
        PositionSide::Long,
        Quantity::new(Decimal::from(100)),
        Price::from_rupees("2000").unwrap(),
    );
    acc.positions.push(pos);

    let state = single_account_state("acct1", acc);

    // Corporate action: dividend of 10 INR per share on 2026-06-01
    let actions = vec![CorporateAction::Dividend {
        amount_per_share: 1000, // 10 INR = 1000 paise
        ex_date: NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
        record_date: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
        payment_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
    }];

    // Fill on 2026-07-01 (after dividend ex-date)
    let fills: [(
        &AccountState,
        InstrumentId,
        PositionSide,
        u64,
        Price,
        NaiveDate,
    ); 1] = [(
        &state.accounts["acct1"],
        id,
        PositionSide::Long,
        50,
        Price::from_rupees("2100").unwrap(),
        NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
    )];

    let tax_summary = TaxSummary::from_fills(
        &fills,
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
        &actions,
    );

    // Dividend doesn't affect position quantity or cost basis
    // Position qty = 100, entry_price = 2000
    // Selling 50 shares at 2100: PnL = 50 * (2100 - 2000) = 5000 INR
    assert_eq!(tax_summary.total_sold_qty, 50);
    assert!(tax_summary.total_realized_pnl.amount > Decimal::from(0));
}

#[test]
fn test_corporate_action_after_fill_not_applied() {
    let id = reliance();

    // Create account with position
    let mut acc = AccountState::new(Currency::Inr, vec![]);
    let pos = HeldPosition::new(
        id.clone(),
        PositionSide::Long,
        Quantity::new(Decimal::from(10)),
        Price::from_rupees("2000").unwrap(),
    );
    acc.positions.push(pos);

    let state = single_account_state("acct1", acc);

    // Corporate action: 1:2 split on 2026-07-01 (AFTER the fill)
    let actions = vec![CorporateAction::Split {
        ratio_numerator: 2,
        ratio_denominator: 1,
        ex_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
    }];

    // Fill on 2026-06-01 (BEFORE split)
    let fills: [(
        &AccountState,
        InstrumentId,
        PositionSide,
        u64,
        Price,
        NaiveDate,
    ); 1] = [(
        &state.accounts["acct1"],
        id,
        PositionSide::Long,
        5,
        Price::from_rupees("2100").unwrap(),
        NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
    )];

    let tax_summary = TaxSummary::from_fills(
        &fills,
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
        &actions,
    );

    // Split NOT applied because ex_date > fill_date
    // Position qty = 10, entry_price = 2000
    // Selling 5 shares at 2100: PnL = 5 * (2100 - 2000) = 500 INR
    assert_eq!(tax_summary.total_sold_qty, 5);
    assert!(tax_summary.total_realized_pnl.amount > Decimal::from(0));
}

// ————————————————————————————————————————
// T+1 Settlement Engine Tests
// ———————————————————————————————————

#[test]
fn test_clearing_calendar_trading_day() {
    let cal = ClearingCalendar::new();

    // Regular weekday should be trading day
    let wednesday = NaiveDate::from_ymd_opt(2026, 1, 7).unwrap(); // Wednesday
    assert!(cal.is_trading_day(wednesday));

    // Weekend should not be trading day
    let saturday = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(); // Saturday
    assert!(!cal.is_trading_day(saturday));

    let sunday = NaiveDate::from_ymd_opt(2026, 1, 11).unwrap(); // Sunday
    assert!(!cal.is_trading_day(sunday));

    // Republic Day (holiday) should not be trading day
    let republic_day = NaiveDate::from_ymd_opt(2026, 1, 26).unwrap(); // Monday
    assert!(!cal.is_trading_day(republic_day));
}

#[test]
fn test_clearing_calendar_settlement_date() {
    let cal = ClearingCalendar::new();

    // Trade on Wednesday, settle on Thursday
    let wednesday = NaiveDate::from_ymd_opt(2026, 1, 7).unwrap();
    let thursday = cal.settlement_date(wednesday);
    assert_eq!(thursday, NaiveDate::from_ymd_opt(2026, 1, 8).unwrap());

    // Trade on Friday, settle on Monday (weekend skip)
    let friday = NaiveDate::from_ymd_opt(2026, 1, 9).unwrap();
    let monday = cal.settlement_date(friday);
    assert_eq!(monday, NaiveDate::from_ymd_opt(2026, 1, 12).unwrap());
}

#[test]
fn test_clearing_calendar_holiday_skip() {
    let cal = ClearingCalendar::new();

    // Trade on Thursday Jan 22, 2026 - settle on Friday Jan 23 (not a holiday)
    let thursday = NaiveDate::from_ymd_opt(2026, 1, 22).unwrap();
    let friday = cal.settlement_date(thursday);
    assert_eq!(friday, NaiveDate::from_ymd_opt(2026, 1, 23).unwrap());

    // Trade on Friday Jan 23 - T+1 is Monday Jan 26, but that is Republic
    // Day, so skip to Tuesday 27th
    let friday = NaiveDate::from_ymd_opt(2026, 1, 23).unwrap();
    let tuesday = cal.settlement_date(friday);
    assert_eq!(tuesday, NaiveDate::from_ymd_opt(2026, 1, 27).unwrap());
}

#[test]
fn test_demat_holdings_free_and_blocked() {
    let id = reliance();
    let mut holdings = DematHoldings::new();

    // Add free holdings
    holdings.add_free(id.clone(), Quantity::from_u64(100));
    assert_eq!(holdings.free_holdings(&id), Quantity::from_u64(100));
    assert_eq!(holdings.total_holdings(&id), Quantity::from_u64(100));

    // Block for settlement
    holdings.block_for_settlement(
        id.clone(),
        Quantity::from_u64(50),
        NaiveDate::from_ymd_opt(2026, 1, 8).unwrap(),
        "trade_001".to_string(),
    );

    // Free should remain 100, blocked should be 50
    assert_eq!(holdings.free_holdings(&id), Quantity::from_u64(100));
    assert_eq!(holdings.total_holdings(&id), Quantity::from_u64(150));

    // Remove free for sell
    holdings.remove_free(&id, Quantity::from_u64(30)).unwrap();
    assert_eq!(holdings.free_holdings(&id), Quantity::from_u64(70));

    // Settle receivable - blocked 50 moves to free
    holdings
        .settle_receivable(&id, Quantity::from_u64(50))
        .unwrap();
    assert_eq!(holdings.free_holdings(&id), Quantity::from_u64(120)); // 70 + 50
    assert_eq!(holdings.total_holdings(&id), Quantity::from_u64(120));
}

#[test]
fn test_demat_holdings_insufficient_free() {
    let id = reliance();
    let mut holdings = DematHoldings::new();

    // Try to remove more than available
    let result = holdings.remove_free(&id, Quantity::from_u64(10));
    assert!(result.is_err());

    // Add some and try again
    holdings.add_free(id.clone(), Quantity::from_u64(5));
    let result = holdings.remove_free(&id, Quantity::from_u64(10));
    assert!(result.is_err());

    // Remove exact amount
    let result = holdings.remove_free(&id, Quantity::from_u64(5));
    assert!(result.is_ok());
    assert_eq!(holdings.free_holdings(&id), Quantity::ZERO);
}

#[test]
fn test_settlement_engine_create_instruction() {
    let engine = SettlementEngine::default();

    let fill = Fill {
        trade_id: "trade_001".to_string(),
        instrument: reliance(),
        side: PositionSide::Long,
        quantity: Quantity::from_u64(10),
        price: Price::from_rupees("2000").unwrap(),
        product: Product::Delivery,
        timestamp: fixed_timestamp(),
        account_id: "acct1".to_string(),
    };

    let instr = engine.create_instruction(&fill, "acct1".to_string());

    assert_eq!(instr.trade_id, "trade_001");
    assert_eq!(instr.side, PositionSide::Long);
    assert_eq!(instr.quantity, Quantity::from_u64(10));
    assert_eq!(instr.product, Product::Delivery);
    assert!(instr.cash_flow.amount < Decimal::ZERO); // Buyer pays
}

#[test]
fn test_settlement_engine_intraday_no_settlement() {
    let engine = SettlementEngine::default();

    let fill = Fill {
        trade_id: "trade_002".to_string(),
        instrument: reliance(),
        side: PositionSide::Long,
        quantity: Quantity::from_u64(10),
        price: Price::from_rupees("2000").unwrap(),
        product: Product::Intraday,
        timestamp: fixed_timestamp(),
        account_id: "acct1".to_string(),
    };

    let instr = engine.create_instruction(&fill, "acct1".to_string());

    // Intraday settles same day
    assert_eq!(instr.trade_date, instr.settle_date);
}

#[test]
fn test_settlement_engine_process_settlements() {
    let engine = SettlementEngine::default();
    let mut holdings = DematHoldings::new();

    let id = reliance();

    // Create a delivery fill
    let fill = Fill {
        trade_id: "trade_003".to_string(),
        instrument: id.clone(),
        side: PositionSide::Long,
        quantity: Quantity::from_u64(10),
        price: Price::from_rupees("2000").unwrap(),
        product: Product::Delivery,
        timestamp: fixed_timestamp(),
        account_id: "acct1".to_string(),
    };

    let instr = engine.create_instruction(&fill, "acct1".to_string());

    // Block the holdings at trade time
    holdings.block_for_settlement(
        id.clone(),
        Quantity::from_u64(10),
        instr.settle_date,
        instr.trade_id.clone(),
    );

    // Process settlement on settle date
    let result = engine.process_settlements(
        &mut holdings,
        std::slice::from_ref(&instr),
        instr.settle_date,
    );

    assert!(result.is_successful());
    assert_eq!(result.settled_count, 1);
    assert_eq!(result.failed_count, 0);
    assert_eq!(holdings.free_holdings(&id), Quantity::from_u64(10));
}

#[test]
fn test_settlement_engine_before_settle_date() {
    let engine = SettlementEngine::default();
    let mut holdings = DematHoldings::new();

    let id = reliance();

    let fill = Fill {
        trade_id: "trade_004".to_string(),
        instrument: id.clone(),
        side: PositionSide::Long,
        quantity: Quantity::from_u64(10),
        price: Price::from_rupees("2000").unwrap(),
        product: Product::Delivery,
        timestamp: fixed_timestamp(),
        account_id: "acct1".to_string(),
    };

    let instr = engine.create_instruction(&fill, "acct1".to_string());
    holdings.block_for_settlement(
        id.clone(),
        Quantity::from_u64(10),
        instr.settle_date,
        instr.trade_id.clone(),
    );

    // Process settlement BEFORE settle date - should not settle
    let before_settle = instr.settle_date - chrono::Duration::days(1);
    let result =
        engine.process_settlements(&mut holdings, std::slice::from_ref(&instr), before_settle);

    assert_eq!(result.settled_count, 0);
    assert_eq!(result.failed_count, 0);
    assert_eq!(holdings.free_holdings(&id), Quantity::ZERO);
}

// ————————————————————————————————————————
// Valuation Tests (new: `value` is a real deterministic implementation here)
// ———————————————————————————————————

#[test]
fn test_value_snapshot_is_deterministic_and_real() {
    let mut acc = AccountState::new(Currency::Inr, vec![]);
    acc.positions.push(HeldPosition::new(
        reliance(),
        PositionSide::Long,
        Quantity::from_u64(10),
        Price::from_rupees("2000").unwrap(),
    ));
    let state = single_account_state("acct1", acc);

    let marks = Marks::new().with(reliance(), Price::from_rupees("2015").unwrap());

    let snap = value(&state, &marks);
    // Market value = 10 * 2015 = 20150; unrealized = 10 * (2015 - 2000) = 150.
    assert_eq!(snap.totals.amount, Decimal::from(20150));
    assert_eq!(snap.unrealized_pnl.amount, Decimal::from(150));
    assert_eq!(snap.realized_pnl.amount, Decimal::ZERO);
    assert_eq!(
        snap.breakdown.get("RELIANCE").map(|m| m.amount),
        Some(Decimal::from(20150))
    );

    // Same inputs, byte-identical snapshot — no clock, no I/O.
    let again = value(&state, &marks);
    assert_eq!(again.totals.amount, snap.totals.amount);
    assert_eq!(again.unrealized_pnl.amount, snap.unrealized_pnl.amount);
}
