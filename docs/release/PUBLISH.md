# AdaTP v1.0.0 — Publish Procedure

## 0. Preconditions

- [`V1_VERIFY.md`](./V1_VERIFY.md) fully checked on this machine.
- `gh auth status` OK, push rights to the `Ada-Transfer-Protocol` org.

## 1. Sync the canonical docs into the Server repo

The workspace root is not itself a git repository; the Server repo is the
published home of the spec and docs:

```bash
bash tools/build/sync-server-repo.sh   # copies SPEC.md, docs/, tests/conformance, demos
```

## 2. Branch, commit, push (every repo)

For `server/` and each `sdks/<name>`:

```bash
git checkout -b release/v1.0.0        # if not already on it
git add -A                            # review with git status first!
git commit -m "AdaTP v1.0.0: WebSocket transport, real auth, spec pack, plugins, webhooks, Silo, portable builds"
git push -u origin release/v1.0.0
```

Staging rules: no `.env`, no `*.db`, no `target/`, `node_modules/`,
`dist/`, `.DS_Store`, no secrets of any kind. `server/vendor/**` IS
committed (offline builds depend on it).

## 3. Pull requests

Where `main` is protected:

```bash
gh pr create --title "AdaTP v1.0.0" --body-file docs/release/PR_BODY.md \
    --base main --head release/v1.0.0
```

(Compose PR bodies from the repo's CHANGELOG section.)

## 4. Tag — humans only

Tag only after V1_VERIFY passes on the release commit:

```bash
git tag -a v1.0.0 -m "AdaTP v1.0.0"
git push origin v1.0.0
```

If any verify step could not be executed on this machine (e.g. Docker
daemon unavailable), push the branch and leave the tag to a person who can
complete verification.

## 5. GitHub release + artifacts

```bash
bash tools/build/build-release.sh
gh release create v1.0.0 dist/adatp-server-1.0.0-*.tar.gz dist/SHA256SUMS \
    --repo Ada-Transfer-Protocol/Server \
    --title "AdaTP v1.0.0" --notes-file docs/release/RELEASE_NOTES.md
```

Attach one tarball per platform (CI artifacts cover linux/macos).

## 6. Package registries (deliberate, optional, NOT automated)

Publishing to npm (`adatp`, `adatp-client`), PyPI (`adatp`), or Packagist
is a separate decision with account ownership implications — v1.0.0 ships
via GitHub only. If/when publishing:

- npm: `npm publish` from `sdks/node` and `sdks/js` (scope/name check first)
- PyPI: `python -m build && twine upload` from `sdks/python`
- Packagist: submit the SDK-PHP repo URL

## 7. Post-publish

- Verify a clean-machine install from the published artifacts
  (`docs/production/install-binary.md`).
- Announce; update org profile README links.
- Open the v1.1 milestone with the known-gaps list
  (`docs/enterprise/README.md`).

## Auth failure recovery

If a push fails with an authentication error, do NOT fabricate tokens.
Recover with one of:

```bash
gh auth login                         # browser flow
gh auth status                        # inspect current identity
git remote -v                         # confirm the expected remote
git push -u origin release/v1.0.0     # retry after login
```

Or configure SSH: `git remote set-url origin git@github.com:Ada-Transfer-Protocol/<repo>.git`
after adding your key to GitHub.
