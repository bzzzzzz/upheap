import { useMe, useUpdateMe } from "../api/queries";
import { LocalDataSection } from "../components/LocalDatabase";
import { NameForm } from "../components/NameForm";

/** The signed-in user's display name, which teammates see, and in the local build their data. */
export function AccountPage() {
  const { data: me } = useMe();
  const update = useUpdateMe();
  if (!me) return null;

  const { user } = me;
  return (
    <main>
      <h1 className="mt-6 mb-4 text-xl font-semibold tracking-tight">Account</h1>
      <dl className="flex max-w-md flex-col gap-4">
        <div>
          <dt className="mb-1 text-sm font-medium">Display name</dt>
          <dd className="flex">
            <NameForm
              label="Display name"
              value={user.display_name}
              version={user.version}
              pending={update.isPending}
              error={update.error}
              onSave={(display_name) => update.mutate({ user, display_name })}
            />
          </dd>
        </div>
        <div>
          <dt className="mb-1 text-sm font-medium">Email</dt>
          <dd className="text-sm text-stone-600 dark:text-stone-300">{user.email}</dd>
        </div>
      </dl>
      <LocalDataSection />
    </main>
  );
}
