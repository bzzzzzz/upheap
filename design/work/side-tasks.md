# Side tasks

Small pieces of work that can run beside the track in flight, [Work with agents](work-with-agents.md), without
touching what its next pieces change. Each is one pull request, and they don't depend on each other, so they can go
in any order and at the same time.

## Progress

- [ ] 1. **Backups.** Production runs on Neon's free tier, with a short restore window
   ([operations.md](../operations.md), Cautions). Check the window on the plan in use, decide whether a paid plan
   is needed, and write down how to restore. Invite links now let real teammates in, and their data with them.
   Done when: operations.md gives the restore window and the steps to restore, and the plan is settled.
- [x] 2. **Old OAuth rows.** Used authorization codes and expired or revoked token families stay in their tables.
   Remove them on each code exchange, as ended sign-in windows are removed on each sign-in attempt.
   Done when: an exchange removes the rows that can no longer be used, a test shows it, and the suites pass on
   SQLite, MySQL and Postgres.
- [x] 3. **Confirmations.** Two actions break ux.md, Rules §1. Archiving a heap acts at once with no Undo toast,
   and giving up your own admin role acts at once without asking first.
   Done when: archiving offers Undo in a toast, demoting yourself asks first in a dialog on a wide screen and a
   sheet on a phone, and tests cover both.
- [x] 4. **Phone width for Activity,** in the design tokens.
   Done when: Activity, the heap's and the team's, fits a phone without horizontal scroll, checked at the
   Playwright phone width.
- [x] 5. **What holds an item back.** An item's contributors say why it's where it is, not why it isn't higher
   (Product Priority §9). Name what's missing or low too: no due time, unset sizes, a low need. Work with agents
   changes the ranking only in stage 2, so this doesn't collide with stage 1.
   Done when: an item missing a due time, or with an unset size or a low need, says so beside its contributors in
   the web app, with domain tests for each case.

Once all five are done, this file goes.
