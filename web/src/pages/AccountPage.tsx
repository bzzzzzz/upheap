import { useMe, useUpdateMe } from "../api/queries";
import { compactButtonClass } from "../components/buttons";
import { LocalDataSection } from "../components/LocalDatabase";
import { NameForm, type NameFormClasses } from "../components/NameForm";

/** The display name form in the design tokens, its buttons the size of the Your data confirm's. */
const nameFormClasses: NameFormClasses = {
  input:
    "rounded-lg border border-line bg-surface px-2.5 py-2 text-sm text-ink placeholder:text-muted disabled:opacity-60 pointer-coarse:min-h-11",
  save: compactButtonClass.primary,
  cancel: compactButtonClass.quiet,
  problem: "text-overdue-ink",
};

/** The signed-in user's display name, which teammates see, and in the local build their data. */
export function AccountPage() {
  const { data: me } = useMe();
  const update = useUpdateMe();
  if (!me) return null;

  const { user } = me;
  return (
    <main>
      <h1 className="mt-6 mb-4 text-[22px] font-semibold tracking-[-0.015em]">Account</h1>
      {/* The mockup's main column, 760 px at most. */}
      <div className="flex max-w-[760px] flex-col gap-6">
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
        <LocalDataSection />
      </div>
    </main>
  );
}
