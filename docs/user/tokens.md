# Machines without a browser

Signing the CLI in with a personal token, where there is no browser to sign in with or no keychain to keep the
sign-in.

A cloud session, a container or CI has no browser to sign in with and often no keychain. Make a token for it on the
web app's Account page, under Command line: **Make a token**, pick how long it lasts, and copy it, since it is shown
once. Then give the machine the server and the token:

```sh
UPHEAP_SERVER=https://upheap.example.com UPHEAP_TOKEN=uph_... upheap item add "Rotate the staging certs"
```

A token can do anything you can, until it expires or you revoke it on the same page. Changing or resetting your
password revokes all of them. When the server refuses the token, the command exits 6 and says whether it was revoked,
has expired or isn't one the server knows.
