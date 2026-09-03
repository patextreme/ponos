# Factory Components are source-mounted, not resolved by a package manager

Factory Components (the shared workflow library under `factory-components/`)
is distributed as plain source: the consumer mounts the tree at a path of
their choosing (nix flake input + symlink, git submodule, vendored copy) and
their shim `require`s it relatively. We deliberately built no ptah registry,
no runtime dependency resolution, no fetch-latest, and no lockfile format:
ptah's sandbox rejects non-relative requires, so a global-install model would
require new CLI surface and new sandbox policy for zero benefit over mounted
source. Pinning is whatever the mount mechanism pins (`flake.lock`, submodule
ref); every library update lands as a reviewable diff in the consumer repo;
and `ptah check` against the component's strict-typed `Config` is the
compatibility gate when a consumer bumps. This was a real tradeoff against a
ptah-native package manager — rejected because updates to executable workflow
code (it can call `ptah.exec`) must never reach a consumer repo without human
review, and because the existing require/check machinery already makes
mounted source work end to end.
