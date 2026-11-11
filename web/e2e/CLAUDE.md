# End-to-end tests

CI runs these without retries, on slower machines, at three widths (desktop, tablet, phone). A check that depends on
timing fails someone's pull request later, and phone and tablet show it first. Keep `retries: 0` in the config, and
don't add retries to hide a flake.

## Wait for what proves the save landed

After an action that saves, assert on something that exists only after the server answered. Many controls show the new
value while saving: switches, optimistic rows, a button that stays until the close lands. Asserting that value, or
reading the API right after it, races the request. A row that left, a toast, or a value only the response has is safe
(the app writes its cache from the response, not before). When the UI has nothing that only the response produces,
start `page.waitForResponse` before the action, match the request's method and a part of its URL, and await it after.
Search `The switch shows the new value while it saves` in `turns.spec.ts` for an example.

## Never check a value that is there before and after

Clicking Done and then expecting "Done" passes before the click lands, because the Done button is still there. Check
what only the new state shows, such as the Reopen button.

## Assertions

- Use web-first assertions (`expect(locator).toHaveText(...)`), which retry. Don't read a value once
  (`await locator.textContent()`, then `expect(value)`).
- Never use `waitForTimeout`.
- Round measured sizes before comparing them with a minimum: `Math.round(box?.height ?? 0)`. Layouts produce
  fractional pixels (43.99997). Keep the `?? 0`, so a missing box fails the check instead of throwing.
- `toBeFocused()` is fine and retries. If one fails now and then, the app is probably moving focus on a timer or a
  guessed number of frames, racing React Aria's own focus restore after an overlay closes. Fix the app, not the test:
  move focus after the render (`useFocusAfterRender` in `web/src/components/useFocusAfterRender.ts`), not on a timer.
- Each test makes its own account and data, so tests run in parallel and share nothing
  ([engineering-principles.md](../../design/engineering-principles.md), TypeScript and React, Tests).

## Reproducing a suspected race

Slow the CPU in the test with
`(await context.newCDPSession(page)).send("Emulation.setCPUThrottlingRate", { rate: 12 })`, then run
`npx playwright test e2e/<file>.spec.ts --repeat-each 10 --project phone`.
