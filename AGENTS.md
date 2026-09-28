# AGENTS.md

Repo-specific guidance for coding agents. Human contribution rules live in `CONTRIBUTING.md`;
this file records local workflow that is easy to get wrong.

## Build, test, verify

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm tauri dev        # hot-reload desktop run
corepack pnpm test             # vitest
corepack pnpm verify           # versions + contracts + frontend + rust
```

Rust-only loop: `cargo test --manifest-path src-tauri/Cargo.toml --all-targets`.

## macOS menu bar needs a bundled `.dev` identity

Quota01 draws one menu-bar item per enabled provider: `src-tauri/src/menubar.rs` reconciles
`tauri-plugin-multiline-menubar` instances. That plugin namespaces each status item by
`NSBundle.mainBundle.bundleIdentifier`, falling back to the process name when the process has no
bundle.

`tauri dev` runs the bare `target/debug/quota01` binary, so macOS registers an anonymous process
(`anon<quota01>`, no bundle identifier). Status items then live in the process-name namespace
(`quota01.*`), where macOS reuses whatever state it recorded earlier — items the user ⌘-dragged out,
items switched off in the menu bar settings, positions saved under the old name. Symptom: startup
succeeds, the settings window opens, the log shows no `[menubar]` warnings, and no menu-bar item ever
appears.

Run menu-bar work under its own identity instead. `src-tauri/tauri.dev.conf.json` overrides only the
identity:

```json
{
  "identifier": "com.lingyi.quota01.dev",
  "productName": "Quota01 Dev"
}
```

```sh
# hot reload with an isolated data directory (menu-bar namespace is still the bare process name)
corepack pnpm exec tauri dev --config src-tauri/tauri.dev.conf.json

# real bundle: the way to exercise the menu bar
corepack pnpm exec tauri build --debug --bundles app --config src-tauri/tauri.dev.conf.json
open "src-tauri/target/debug/bundle/macos/Quota01 Dev.app"
```

|                       | default build                                      | `.dev` config              |
| --------------------- | -------------------------------------------------- | -------------------------- |
| menu-bar namespace    | `com.lingyi.quota01` or `quota01`                  | `com.lingyi.quota01.dev`   |
| app data              | `~/Library/Application Support/com.lingyi.quota01` | `…/com.lingyi.quota01.dev` |
| single-instance scope | shared with the installed app                      | independent                |

The `.dev` data directory starts empty, so API keys and provider enablement are entered again there;
the installed Quota01.app may keep running alongside. Both identities log to
`~/Library/Logs/Quota01/Quota01.log`.

### Checking the menu bar without looking at it

A visible status item owns a menu-bar-sized window, so the item count is observable:

```sh
cat > /tmp/menubar-check.swift <<'EOF'
import CoreGraphics
import Foundation
let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
for w in list {
  let owner = w["kCGWindowOwnerName"] as? String ?? ""
  let b = w["kCGWindowBounds"] as? [String: Any] ?? [:]
  let h = b["Height"] as? Int ?? 0, y = b["Y"] as? Int ?? -1
  if owner.contains("Quota01") && h <= 40 && y == 0 { print("\(owner): \(b)") }
}
EOF
swift /tmp/menubar-check.swift
```

One line per menu-bar item; zero lines means the items never rendered. In that case look for
`menu bar instance … was created but is not visible` in the log, and note that reconciliation
reporting no instances opens the settings window on purpose (`apply_runtime_entry`).

### Also expected on macOS

`desktop integration detected (tray=false)` is normal — macOS and Windows report no tray until
provider instances are reconciled, and the menu bar comes from those instances rather than from the
Tauri tray. A settings window that opens by itself at startup means reconciliation found no visible
menu-bar item.
