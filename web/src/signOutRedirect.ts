// Signing out through the identity provider leaves the app, and the provider sends the person to the sign-in page
// with no `redirect`. The page they signed out on is kept here, in the tab, for the sign-in page to pick up.
const KEY = "upheap.signOutRedirect";

/** How long a kept page waits for the provider to send the person back. A later one is someone else's. */
const MAX_AGE_MS = 10 * 60 * 1000;

/** Keeps `href`, an app path with its search and hash, for `takeKeptPage`. */
export function keepPage(href: string): void {
  sessionStorage.setItem(KEY, JSON.stringify({ href, at: Date.now() }));
}

/**
 * The page `keepPage` kept, removed as it is read; nothing when none was kept, it is older than ten minutes or it
 * can't be read. The caller checks it like any `redirect` before using it.
 */
export function takeKeptPage(): string | undefined {
  const kept = sessionStorage.getItem(KEY);
  sessionStorage.removeItem(KEY);
  if (kept === null) return undefined;
  try {
    const { href, at } = JSON.parse(kept) as { href?: unknown; at?: unknown };
    if (typeof href === "string" && typeof at === "number" && Date.now() - at <= MAX_AGE_MS) return href;
  } catch {
    // Not what `keepPage` wrote.
  }
  return undefined;
}

/**
 * What to do once a sign-out has succeeded. With the provider's logout URL (`signedOut`), keeps `page` (nothing for
 * a page that has no use for one) and goes to the provider; otherwise runs `otherwise`, the plain sign-out's way on.
 */
export function leaveAfterSignOut(
  signedOut: { provider_logout_url: string } | undefined,
  page: string | undefined,
  otherwise: () => void,
): void {
  if (!signedOut) return otherwise();
  if (page === undefined) sessionStorage.removeItem(KEY);
  else keepPage(page);
  window.location.assign(signedOut.provider_logout_url);
}
