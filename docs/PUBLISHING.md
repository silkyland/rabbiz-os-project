# Publishing a clean copy of the history

`scripts/release/public_history.sh` builds a **new** repository directory from one branch with
identities unified and private strings redacted. It never pushes and never modifies this repo.

```bash
brew install git-filter-repo gitleaks          # gitleaks is optional but strongly recommended
printf '<real-mac-or-private-string>==><placeholder>\n' > scripts/release/redact.local.txt  # git-ignored
PUBLIC_EMAIL='<id>+<login>@users.noreply.github.com' \
OLD_EMAILS='old1@example.com,old2@example.com' \
scripts/release/public_history.sh ../rabbiz-os-project-public
```

What it does: clones only `BRANCH` (default `main`), maps every old email to `PUBLIC_EMAIL`,
replaces `192.168.x.y` with `<lan-ip>` and `/Users/<name>/` with `/Users/<user>/`, applies your
`redact.local.txt`, then verifies (same commit count, same file list, no old email in metadata or
file contents, no pattern left, no remote) and runs `gitleaks` when installed.

Keep the private rules in `redact.local.txt` (git-ignored): putting real MACs or addresses in the
script would publish them. Set `PUBLIC_NAME` only if you also want to change the author name.
Push the result to the private repository first and review it before making it public.
