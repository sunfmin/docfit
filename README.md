# docfit

Keeps a repo's documents each in its one home, within its budget: a CLI for AI coding agents, beside slimfit (code size and tests) and truefit (ties to the real system).

The manual is `docfit --help`, and it is the only copy.

Build and check: `./gate.sh`.

Install: `brew install sunfmin/tap/docfit`.
A release is cut by factorium (`[cut]` in factorium.toml runs `packaging/release.sh cut`).
`docfit --version` says `docfit X.Y.Z` for a release and `X.Y.Z+<commits>.g<commit>[.dirty]` for any other build.

Its own documents are registered in `docfit.toml`, and the gate runs `docfit check` on them.
