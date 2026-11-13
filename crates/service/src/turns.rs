//! Turn use cases: sending an item to an agent, and taking it back.
//!
//! A turn write never changes the item's version, and an item write never changes a turn's, so sending an item
//! doesn't conflict with someone editing it (architecture.md, Data model). Both use cases return the whole item, so
//! a client re-renders from one response. Starting or closing an item, and archiving its heap, take its turn back
//! too, through [`Services::take_back_open_turn`]; a turn whose item is at its goal ends as reached through
//! [`Services::end_met_turn`].

use domain::audit::AuditAction;
use domain::heap::Heap;
use domain::item::{Item, ItemError, Kind, Stage, StateKind};
use domain::permission::Permission;
use domain::run::{Run, RunEnd};
use domain::turn::{Reach, RedoError, TakeBackWhy, Turn, TurnError, TurnLine, TurnSource, check_redo, redo_stage};
use domain::{ItemId, Timestamp, TurnId};
use store::{ConnectionTrait, StoreError, TransactionTrait};

use crate::access::{authorize_heap_write, find_item_locking_in};
use crate::audit::{
    GoalMetHow, describe_goal_met, describe_new_turn, describe_reached, describe_redone, describe_retry,
    describe_take_back, describe_withdrawal,
};
use crate::items::{ItemChildren, ItemView, build_item_view};
use crate::{Principal, ServiceError, Services};

impl Services {
    /// Sends an item to an agent: opens a turn of `reach` with an optional `line` from the person, going to the
    /// next stable state. Any member may.
    ///
    /// Fails with [`ItemError::Closed`] on a closed item, [`TurnError::NeedsNoteOrTask`] on a reminder, to come or
    /// not (it must become a task first), [`TurnError::Blocked`] on a blocked item (it must be unblocked first),
    /// [`ServiceError::HeapArchived`] in an archived heap, and [`TurnError::AlreadyOpen`] when an agent already has
    /// the item. An item with an open question may be sent: the turn opens, and the item waits as before.
    pub async fn send_to_agent(
        &self,
        principal: Principal,
        item_id: ItemId,
        reach: Reach,
        line: Option<TurnLine>,
    ) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let found = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems).await?;
        // Read again under the heap lock, with locking reads: on MySQL a plain read would see the snapshot fixed by
        // the first read above, not what committed while this waited for the lock.
        let item = find_item_locking_in(&txn, item_id, access.heap.id, found.version).await?;
        if item.is_closed() {
            return Err(ItemError::Closed { item_id: item.id }.into());
        }
        if item.kind == Kind::Reminder {
            return Err(TurnError::NeedsNoteOrTask { item_id: item.id }.into());
        }
        if item.state.kind() == StateKind::Blocked {
            return Err(TurnError::Blocked { item_id: item.id }.into());
        }
        if store::turns::find_open_turn_locking(&txn, item.id).await?.is_some() {
            return Err(TurnError::AlreadyOpen { item_id: item.id }.into());
        }

        let now = self.now();
        let turn = Turn::open(
            TurnId::from_uuid(self.new_id(now)),
            &item,
            reach,
            line,
            TurnSource::GoAhead,
            principal.user_id,
            now,
        );
        store::turns::insert_turn(&txn, &turn).await?;
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::TurnOpened,
            &item,
            describe_new_turn(&turn),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let children = ItemChildren::read_locking(&txn, item.id).await?;
        let view = build_item_view(&txn, &access.heap, access.membership.role, item, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            reach = turn.reach.code(),
            goal = turn.goal.code(),
            user_id = %principal.user_id,
            "turn opened"
        );
        Ok(view)
    }

    /// Takes an item back from the agent that has it. An item with no open turn is returned unchanged, without an
    /// audit entry, so two clients taking back at once both succeed. Fails with [`ServiceError::HeapArchived`] in an
    /// archived heap.
    pub async fn take_back(&self, principal: Principal, item_id: ItemId) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let found = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems).await?;
        let item = find_item_locking_in(&txn, item_id, access.heap.id, found.version).await?;

        let now = self.now();
        let taken = self
            .take_back_open_turn(&txn, &access.heap, principal, item.id, TakeBackWhy::TakeBack, now)
            .await?;
        let children = ItemChildren::read_locking(&txn, item.id).await?;
        let view = build_item_view(&txn, &access.heap, access.membership.role, item, children, now).await?;
        if !taken {
            // Nothing was written.
            return Ok(view);
        }
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            user_id = %principal.user_id,
            "turn taken back"
        );
        Ok(view)
    }

    /// Lets the agents try again at an item whose turn is stuck: the count of failed runs goes back to 0, so the item is
    /// in the queue again, and `turn_retried` is recorded. Any member may. Fails with [`TurnError::NotStuck`] when the
    /// item has no open turn or its turn is not stuck, and with [`ServiceError::HeapArchived`] in an archived heap.
    pub async fn retry_turn(&self, principal: Principal, item_id: ItemId) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let found = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems).await?;
        let item = find_item_locking_in(&txn, item_id, access.heap.id, found.version).await?;
        let now = self.now();
        let open = store::turns::find_open_turn_locking(&txn, item.id)
            .await?
            .ok_or(TurnError::NotStuck { item_id: item.id })?;
        let lapsed: Vec<Run> = store::runs::list_unended_runs_for_turn_locking(&txn, open.id)
            .await?
            .into_iter()
            .filter(|run| run.is_lapsed(now))
            .collect();
        let turn = open.counting_lapse(!lapsed.is_empty());
        if !turn.is_stuck() {
            return Err(TurnError::NotStuck { item_id: item.id }.into());
        }
        // A lapsed run counts as failed until it is written, so write it: left unended it would count again after
        // the reset.
        for run in &lapsed {
            self.end_lapsed_run(&txn, run, now).await?;
        }
        if !store::turns::set_failed_runs(&txn, turn.id, 0).await? {
            return Err(TurnError::NotStuck { item_id: item.id }.into());
        }
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::TurnRetried,
            &item,
            describe_retry(&turn),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let children = ItemChildren::read_locking(&txn, item.id).await?;
        let view = build_item_view(&txn, &access.heap, access.membership.role, item, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            user_id = %principal.user_id,
            failed_runs = turn.failed_runs,
            "turn retried"
        );
        Ok(view)
    }

    /// Ends the open turn of `item_id`, if it has one, with its `turn_taken_back` entry, and says whether there was
    /// one. The caller holds the heap lock, which keeps two writes from ending or opening a turn at once.
    pub(crate) async fn take_back_open_turn(
        &self,
        txn: &impl ConnectionTrait,
        heap: &Heap,
        principal: Principal,
        item_id: ItemId,
        why: TakeBackWhy,
        now: Timestamp,
    ) -> Result<bool, ServiceError> {
        let Some(turn) = store::turns::find_open_turn_locking(txn, item_id).await? else {
            return Ok(false);
        };
        self.take_back_turn(txn, heap, principal, &turn, why, now).await
    }

    /// Asks for changes to an item whose latest turn reached a stable state short of done: that turn is marked redone,
    /// the item's stage goes back to where the turn's first run found it, and a new turn opens to the same reach and
    /// goal with the person's `line`. The kind isn't changed back, and what the agent wrote (context, steps) stays.
    /// `turn_redone` and `turn_opened` are recorded. Any member who may edit items may.
    ///
    /// Fails with [`ItemError::Closed`] on a closed item, [`TurnError::Blocked`] on a blocked one,
    /// [`ServiceError::HeapArchived`] in an archived heap, and [`TurnError::NothingToRedo`] when the item is a
    /// reminder, has an open turn, or its latest turn wasn't reached or is redone already.
    pub async fn redo_turn(
        &self,
        principal: Principal,
        item_id: ItemId,
        line: TurnLine,
    ) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let found = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems).await?;
        let item = find_item_locking_in(&txn, item_id, access.heap.id, found.version).await?;
        let latest = store::turns::find_latest_turn(&txn, item.id, true).await?;
        let reached = check_redo(&item, latest.as_ref()).map_err(|error| match error {
            RedoError::Item(error) => ServiceError::Item(error),
            RedoError::Turn(error) => ServiceError::Turn(error),
        })?;

        let now = self.now();
        let earliest = store::runs::find_earliest_run_for_turn_locking(&txn, reached.id).await?;
        let before = item.stage;
        let after = redo_stage(&item, earliest.as_ref());
        let redone = reached.redone_by(principal.user_id, now);
        if !store::turns::mark_turn_redone(&txn, &redone).await? {
            return Err(TurnError::NothingToRedo { item_id: item.id }.into());
        }
        let item = if after == before {
            item
        } else {
            let edited = Item {
                stage: after,
                ..item.clone()
            };
            store::items::update_item(&txn, &edited, item.version, now)
                .await?
                .ok_or(ServiceError::VersionConflict {
                    expected: item.version,
                    current: item.version.next(),
                })?
        };
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::TurnRedone,
            &item,
            describe_redone(reached, before, after, line.as_str()),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let next = reached.redo(TurnId::from_uuid(self.new_id(now)), line, principal.user_id, now);
        store::turns::insert_turn(&txn, &next).await?;
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::TurnOpened,
            &item,
            describe_new_turn(&next),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let children = ItemChildren::read_locking(&txn, item.id).await?;
        let view = build_item_view(&txn, &access.heap, access.membership.role, item, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            goal = next.goal.code(),
            user_id = %principal.user_id,
            "turn redone"
        );
        Ok(view)
    }

    /// Ends the open turn of `item`, if it has one, as reached by the person: the person agreed to a done proposal.
    /// It writes a `turn_reached` entry (the stage doesn't change) and ends the turn's runs that haven't ended as
    /// asked, since the proposal was asked in one. Says whether there was an open turn. The caller holds the heap
    /// lock.
    pub(crate) async fn reach_open_turn(
        &self,
        txn: &impl ConnectionTrait,
        heap: &Heap,
        principal: Principal,
        item: &Item,
        now: Timestamp,
    ) -> Result<bool, ServiceError> {
        let Some(turn) = store::turns::find_open_turn_locking(txn, item.id).await? else {
            return Ok(false);
        };
        if !store::turns::end_turn(txn, &turn.reached(principal.user_id, now)).await? {
            return Ok(false);
        }
        let entry = self.item_audit_entry(
            heap,
            principal,
            AuditAction::TurnReached,
            item,
            describe_reached(turn.goal, item.stage, item.stage),
            now,
        );
        store::audit::insert_audit_entry(txn, &entry).await?;
        for run in store::runs::list_unended_runs_for_turn_locking(txn, turn.id).await? {
            store::runs::end_run(txn, &run.ended_as(RunEnd::Asked, now, None, Vec::new(), None)).await?;
        }
        Ok(true)
    }

    /// Ends `turn`, which the caller read under the heap lock, with its `turn_taken_back` entry. Says whether it
    /// was still open.
    ///
    /// The turn's runs that haven't ended, live or lapsed, end as taken back, which is not a failure. The open
    /// questions asked in a run of the turn are withdrawn, each with its `question_withdrawn` entry; the person's own
    /// stay open.
    pub(crate) async fn take_back_turn(
        &self,
        txn: &impl ConnectionTrait,
        heap: &Heap,
        principal: Principal,
        turn: &Turn,
        why: TakeBackWhy,
        now: Timestamp,
    ) -> Result<bool, ServiceError> {
        let ended = turn.take_back(principal.user_id, now);
        if !store::turns::end_turn(txn, &ended).await? {
            return Ok(false);
        }
        let entry = self.item_audit_entry_for(
            heap,
            principal,
            AuditAction::TurnTakenBack,
            turn.item_id,
            describe_take_back(turn, why),
            now,
        );
        store::audit::insert_audit_entry(txn, &entry).await?;
        self.release_turn_work(txn, heap, principal, turn, why, now).await?;
        Ok(true)
    }

    /// Ends `turn`, which the caller read under the heap lock, as reached because its item is at the goal: `by` the
    /// person (`principal`, outside a run), with a `turn_reached` entry saying how. `item_after` is the item as it
    /// stands, and `before` its stage before the edit that moved it (the same stage, for a claim that found it there).
    /// Says nothing was done, with `None`, when the turn had already ended.
    ///
    /// Like [`Self::take_back_turn`], the turn's unended runs end as taken back and the open questions asked in them
    /// are withdrawn, with `goal_met`. A turn sent all the way opens the next one, and it is returned. The caller holds
    /// the heap lock and writes the item's own entry first, so the history reads in the order things happened.
    #[allow(clippy::too_many_arguments)] // Every one differs between the two callers.
    pub(crate) async fn end_met_turn(
        &self,
        txn: &impl ConnectionTrait,
        heap: &Heap,
        principal: Principal,
        turn: &Turn,
        item_after: &Item,
        before: Stage,
        how: GoalMetHow,
        now: Timestamp,
    ) -> Result<Option<Turn>, ServiceError> {
        let ended = turn.reached(principal.user_id, now);
        if !store::turns::end_turn(txn, &ended).await? {
            return Ok(None);
        }
        let entry = self.item_audit_entry(
            heap,
            principal,
            AuditAction::TurnReached,
            item_after,
            describe_goal_met(turn.goal, before, item_after.stage, how),
            now,
        );
        store::audit::insert_audit_entry(txn, &entry).await?;
        self.release_turn_work(txn, heap, principal, turn, TakeBackWhy::GoalMet, now)
            .await?;
        let next = turn.follow_on(TurnId::from_uuid(self.new_id(now)), item_after, now);
        if let Some(next) = &next {
            store::turns::insert_turn(txn, next).await?;
            let entry = self.item_audit_entry(
                heap,
                principal,
                AuditAction::TurnOpened,
                item_after,
                describe_new_turn(next),
                now,
            );
            store::audit::insert_audit_entry(txn, &entry).await?;
        }

        tracing::info!(
            heap_id = %heap.id,
            item_id = %item_after.id,
            goal = turn.goal.code(),
            how = how.code(),
            next_goal = next.as_ref().map_or("none", |next| next.goal.code()),
            user_id = %principal.user_id,
            "turn ended at its goal"
        );
        Ok(next)
    }

    /// Ends the unended runs of `turn`, which has just ended, as taken back (not a failure, live or lapsed), and
    /// withdraws the open questions asked in them, each with its `question_withdrawn` entry and `why`; the person's
    /// own questions stay open.
    pub(crate) async fn release_turn_work(
        &self,
        txn: &impl ConnectionTrait,
        heap: &Heap,
        principal: Principal,
        turn: &Turn,
        why: TakeBackWhy,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        for run in store::runs::list_unended_runs_for_turn_locking(txn, turn.id).await? {
            let ended = run.ended_as(RunEnd::TakenBack, now, None, Vec::new(), None);
            store::runs::end_run(txn, &ended).await?;
        }
        let questions = store::questions::list_open_questions_for_runs_of_turn_locking(txn, turn.id).await?;
        for question in &questions {
            let withdrawn = question.withdraw(principal.user_id, now);
            store::questions::update_question(txn, &withdrawn, question.version)
                .await?
                .ok_or(ServiceError::VersionConflict {
                    expected: question.version,
                    current: question.version.next(),
                })?;
            let entry = self.item_audit_entry_for(
                heap,
                principal,
                AuditAction::QuestionWithdrawn,
                turn.item_id,
                describe_withdrawal(question, why),
                now,
            );
            store::audit::insert_audit_entry(txn, &entry).await?;
        }
        Ok(())
    }
}
