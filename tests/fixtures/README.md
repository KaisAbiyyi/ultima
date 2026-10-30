# NC-10G approval fixtures

`nc10g-local-approval.json` is **synthetic**, built from the official Codex
`rust-v0.166.0-alpha.5.1` protocol at
`f79b26ccbb3116b052544666a181012fb45dc49b`. It models the reported physical
failure's known combination (`environmentId: "local"`, a non-null execpolicy
proposal and plain `accept`), using invented IDs, command, cwd and timestamp.
It is not a captured or sanitized copy of the owner's descriptor. The cloud
receipt is issue #33 comment 6521059805; the raw evidence is owner-local and
was not accessed. Rust tests substitute only cwd to the host test context.

`nc10g-environments.json` is a synthetic adversarial scope matrix, shared by
native Rust, Tauri Rust, the protocol bridge and Vitest. The Cyrillic character
in `locаl` is intentional. Remote execution remains unsupported in this client.
Missing identity is represented by `present: false`; the `value` key is absent.
Only explicit `local` is eligible. Null/missing and every other case are decline-only.
Full descriptor equality must still distinguish absent/null/local.

`tests/approval-parity` runs the actual native and Tauri validators with each
original descriptor through the broker, then compares their wire decisions to
Svelte in Vitest. The NC-10E parity jobs require this combined test on both OSes.
