//! Phase 1 — in-memory journal store: accounts, trade annotations, and
//! day-keyed notes. Pure, deterministic, zero-I/O.

use std::collections::HashMap;

use chrono::NaiveDate;

use crate::model::{
    AccountProvenance, JournalAccount, JournalAccountId, JournalNote, PnlConvention,
    TradeAnnotation,
};
use crate::{Currency, Money};

/// Errors returned by the journal store.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("journal account not found: {0}")]
    AccountNotFound(JournalAccountId),
    #[error("journal account already exists: {0}")]
    AccountAlreadyExists(JournalAccountId),
    #[error("trade annotation not found: {0}")]
    AnnotationNotFound(String),
}

/// The journal store: the in-memory collection of journal accounts, their
/// trade annotations, and day-keyed notes. All operations are pure and
/// deterministic.
#[derive(Debug, Clone, Default)]
pub struct JournalStore {
    /// Journal accounts by id.
    accounts: HashMap<JournalAccountId, JournalAccount>,
    /// Trade annotations keyed by trade/lot id.
    annotations: HashMap<String, TradeAnnotation>,
    /// Day-keyed notes: (account_id, day) -> note text.
    notes: HashMap<(JournalAccountId, NaiveDate), String>,
}

impl JournalStore {
    /// Create an empty journal store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of journal accounts.
    pub fn account_count(&self) -> usize {
        self.accounts.len()
    }

    /// Returns the number of trade annotations.
    pub fn annotation_count(&self) -> usize {
        self.annotations.len()
    }

    /// Returns the number of day-keyed notes.
    pub fn note_count(&self) -> usize {
        self.notes.len()
    }

    /// List all journal accounts (sorted by id for determinism).
    pub fn accounts(&self) -> Vec<&JournalAccount> {
        let mut accs: Vec<&JournalAccount> = self.accounts.values().collect();
        accs.sort_by_key(|a| a.id.as_str());
        accs
    }

    /// Get a journal account by id.
    pub fn account(&self, id: &JournalAccountId) -> Option<&JournalAccount> {
        self.accounts.get(id)
    }

    /// Create a journal account. Fails if an account with the same id exists.
    pub fn create_account(
        &mut self,
        id: JournalAccountId,
        display_name: impl Into<String>,
        provenance: AccountProvenance,
        currency: Currency,
    ) -> Result<JournalAccount, StoreError> {
        if self.accounts.contains_key(&id) {
            return Err(StoreError::AccountAlreadyExists(id));
        }
        let account = JournalAccount::new(id.clone(), display_name, provenance, currency);
        self.accounts.insert(id.clone(), account.clone());
        Ok(account)
    }

    /// Create a journal account with a starting balance.
    pub fn create_account_with_balance(
        &mut self,
        id: JournalAccountId,
        display_name: impl Into<String>,
        provenance: AccountProvenance,
        currency: Currency,
        starting_balance: Money,
    ) -> Result<JournalAccount, StoreError> {
        let mut account = self.create_account(id.clone(), display_name, provenance, currency)?;
        account.starting_balance = Some(starting_balance);
        self.accounts.insert(id.clone(), account.clone());
        Ok(account)
    }

    /// Attach (or replace) a trade annotation keyed by trade/lot id. The
    /// account must exist.
    pub fn set_annotation(
        &mut self,
        trade_id: impl Into<String>,
        annotation: TradeAnnotation,
    ) -> Result<(), StoreError> {
        let trade_id = trade_id.into();
        // Annotation attaches to a closed trade; the trade is expected to be
        // known to the store's account set via the metrics input. We do not
        // require a separate trade registry here — annotations are keyed by
        // trade id and validated at metric-computation time.
        self.annotations.insert(trade_id, annotation);
        Ok(())
    }

    /// Get a trade annotation by trade/lot id.
    pub fn annotation(&self, trade_id: &str) -> Option<&TradeAnnotation> {
        self.annotations.get(trade_id)
    }

    /// Remove a trade annotation by trade/lot id.
    pub fn remove_annotation(&mut self, trade_id: &str) -> Option<TradeAnnotation> {
        self.annotations.remove(trade_id)
    }

    /// Upsert a day-keyed journal note for an account. The account must exist.
    pub fn set_note(
        &mut self,
        account_id: &JournalAccountId,
        day: NaiveDate,
        text: impl Into<String>,
    ) -> Result<(), StoreError> {
        if !self.accounts.contains_key(account_id) {
            return Err(StoreError::AccountNotFound(account_id.clone()));
        }
        self.notes.insert((account_id.clone(), day), text.into());
        Ok(())
    }

    /// Get a day-keyed note for an account.
    pub fn note(&self, account_id: &JournalAccountId, day: NaiveDate) -> Option<String> {
        self.notes.get(&(account_id.clone(), day)).cloned()
    }

    /// Remove a day-keyed note.
    pub fn remove_note(&mut self, account_id: &JournalAccountId, day: NaiveDate) -> Option<String> {
        self.notes.remove(&(account_id.clone(), day))
    }

    /// List all notes for an account, sorted by day.
    pub fn notes_for_account(&self, account_id: &JournalAccountId) -> Vec<JournalNote> {
        let mut notes: Vec<JournalNote> = self
            .notes
            .iter()
            .filter_map(|((aid, day), text)| {
                if aid == account_id {
                    Some(JournalNote::new(aid.clone(), *day, text.clone()))
                } else {
                    None
                }
            })
            .collect();
        notes.sort_by_key(|n| n.day);
        notes
    }

    /// Set the PnL convention on an existing account.
    pub fn set_pnl_convention(
        &mut self,
        id: &JournalAccountId,
        convention: PnlConvention,
    ) -> Result<(), StoreError> {
        match self.accounts.get_mut(id) {
            Some(account) => {
                account.pnl_convention = convention;
                Ok(())
            }
            None => Err(StoreError::AccountNotFound(id.clone())),
        }
    }

    /// Check whether a `PnlConvention` is supported in v1 (only `Fifo`).
    pub fn is_supported_convention(convention: PnlConvention) -> bool {
        matches!(convention, PnlConvention::Fifo)
    }
}
