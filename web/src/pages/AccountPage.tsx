import { useNavigate } from "@tanstack/react-router";
import { useId } from "react";
import { ToggleButton, ToggleButtonGroup } from "react-aria-components";

import { useMe, useSignOut, useUpdateMe } from "../api/queries";
import { compactButtonClass } from "../components/buttons";
import { choiceButtonClass } from "../components/ChoiceButtons";
import { LocalDataSection } from "../components/LocalDatabase";
import { NameForm, type NameFormClasses } from "../components/NameForm";
import { ProblemText } from "../components/ProblemText";
import { useTheme, type Theme } from "../components/theme";

/** The display name form in the design tokens, its buttons the size of the Your data confirm's. */
const nameFormClasses: NameFormClasses = {
  input: "rounded-lg border border-line bg-surface px-2.5 py-2 text-sm text-ink pointer-coarse:min-h-11",
  save: compactButtonClass.primary,
  cancel: compactButtonClass.quiet,
  problem: "text-overdue-ink",
};

const themes: readonly { id: Theme; label: string }[] = [
  { id: "system", label: "System" },
  { id: "light", label: "Light" },
  { id: "dark", label: "Dark" },
];

/**
 * The signed-in user's display name, which teammates see, this browser's theme, and in the local build their data.
 * Sign out is here rather than in the navigation, at the bottom, where the server allows it.
 */
export function AccountPage() {
  const { data: me } = useMe();
  const update = useUpdateMe();
  const signOut = useSignOut();
  const navigate = useNavigate();
  if (!me) return null;

  const { user } = me;
  return (
    <main>
      <h1 className="mt-6 mb-4 text-[22px] font-semibold tracking-[-0.015em]">Account</h1>
      <div className="flex flex-col gap-6">
        <section aria-labelledby="profile" className="rounded-[14px] border border-line bg-surface p-5">
          <h2 id="profile" className="text-base font-semibold">
            Profile
          </h2>
          <dl className="mt-4 flex flex-col gap-4">
            <div>
              <dt className="mb-1.5 text-sm font-medium text-ink">Display name</dt>
              <dd className="flex">
                <NameForm
                  label="Display name"
                  value={user.display_name}
                  version={user.version}
                  pending={update.isPending}
                  error={update.error}
                  onSave={(display_name) => update.mutate({ user, display_name })}
                  classes={nameFormClasses}
                />
              </dd>
            </div>
            <div>
              <dt className="mb-1 text-sm font-medium text-ink">Email</dt>
              <dd className="text-sm break-words text-ink-soft">{user.email}</dd>
            </div>
          </dl>
        </section>
        <AppearanceSection />
        <LocalDataSection />
        {me.allowed_actions.includes("sign_out") && (
          <div className="flex flex-wrap items-center gap-3">
            <button
              type="button"
              disabled={signOut.isPending}
              onClick={() => signOut.mutate(undefined, { onSuccess: () => void navigate({ to: "/login" }) })}
              className={compactButtonClass.borderedDanger}
            >
              Sign out
            </button>
            <ProblemText error={signOut.error} className="text-overdue-ink" />
          </div>
        )}
      </div>
    </main>
  );
}

/**
 * The theme for this browser. Unlike ChoiceButtons, pressing the picked button again keeps it: a theme always has one
 * value, and System is the way back to none.
 */
function AppearanceSection() {
  const [theme, setTheme] = useTheme();
  const labelId = useId();
  const hintId = useId();
  return (
    <section aria-labelledby="appearance" className="rounded-[14px] border border-line bg-surface p-5">
      <h2 id="appearance" className="text-base font-semibold">
        Appearance
      </h2>
      <div className="mt-4 flex flex-col gap-1.5">
        <span id={labelId} className="text-sm font-medium text-ink">
          Theme
        </span>
        <ToggleButtonGroup
          aria-labelledby={labelId}
          selectionMode="single"
          disallowEmptySelection
          selectedKeys={[theme]}
          onSelectionChange={(keys) => {
            const [picked] = [...keys];
            if (picked !== undefined) setTheme(String(picked) as Theme);
          }}
          className="grid max-w-xs auto-cols-fr grid-flow-col gap-1"
        >
          {themes.map((option) => (
            <ToggleButton
              key={option.id}
              id={option.id}
              // On each button: the group passes no `aria-describedby` on.
              aria-describedby={hintId}
              className={`${choiceButtonClass} rounded-lg`}
            >
              {option.label}
            </ToggleButton>
          ))}
        </ToggleButtonGroup>
        <span id={hintId} className="text-xs text-muted">
          System follows your device. Kept in this browser only.
        </span>
      </div>
    </section>
  );
}
