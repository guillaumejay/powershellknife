# powershellknife

A keyboard-first TUI to maintain your PowerShell setup on Windows, without
writing a single line of PowerShell yourself. Ships as one binary, `psknife.exe`.

Two things it does, end of story:

1. **History cleaner** — detects duplicates and typos in
   `ConsoleHost_history.txt`, lets you fix them interactively, backs up
   before writing.
2. **Profile editor** — exposes common PSReadLine options, auto-imported
   modules and persistent aliases as a form. Writes to a *managed block*
   (`# >>> powershellknife … # <<<`) so your custom profile code stays
   intact byte-for-byte.

Every write creates a timestamped backup under
`~/.powershellknife/backups/`. `psknife restore` rolls back.

## Status

MVP. Windows + PowerShell 7 + `CurrentUser / CurrentHost` profile only.
PS 5.1, other profiles, Linux/macOS: not supported yet — on purpose.

## Install

No distribution yet — build from source:

```powershell
git clone https://github.com/<you>/powershellknife.git
cd powershellknife
cargo build --release
# binary is at target/release/psknife.exe
```

Requires Rust stable (edition 2024) and PowerShell 7 on `PATH` for the
optional cmdlet-inventory refresh.

## Usage

Run the TUI:

```powershell
psknife
```

Roll back the last apply:

```powershell
psknife restore
# or pick a specific backup timestamp:
psknife restore --timestamp 2026-04-19_143022
```

## Keybindings

### Global

| Key           | Action                             |
| ------------- | ---------------------------------- |
| `F3`          | next tab                           |
| `Shift+Tab`   | previous tab                       |
| `q` / `F10`   | quit (prompts if unsaved changes)  |

### History tab

| Key           | Action                                     |
| ------------- | ------------------------------------------ |
| `↑ ↓` / `j k` | move selection                             |
| `Tab`         | filter: All / Typos / Duplicates           |
| `d`           | delete selected entry (or whole dup group) |
| `r`           | replace typo with suggestion               |
| `K`           | keep (clear pending action)                |
| `c`           | collapse dup group (keep latest)           |
| `A`           | bulk auto-fix all high-confidence typos    |
| `X`           | bulk collapse all duplicate groups         |
| `p`           | preview diff of pending changes            |
| `F2`          | apply (confirm with `y`)                   |
| `o`           | open history file in default editor        |
| `F5`          | reload from disk                           |

### Profile tab

| Key           | Action                                            |
| ------------- | ------------------------------------------------- |
| `Tab`         | cycle section (PSReadLine / Modules / Aliases / Custom) |
| `↑ ↓` / `j k` | move selection within section                     |
| `Space`       | toggle bool option                                |
| `← →`         | cycle enum value (PredictionSource, EditMode, BellStyle) |
| `?`           | open docs for the selected PSReadLine option      |
| `Enter`       | add module / add alias (on the `[+]` row)         |
| `e`           | edit alias value                                  |
| `x` / `Del`   | remove selected module or alias                   |
| `F2`          | apply (backup + atomic write)                     |
| `o`           | open profile in default editor                    |
| `F5`          | reload from disk                                  |

## What gets written to the profile

Only lines between these markers — your custom code is never touched:

```powershell
# >>> managed by powershellknife — do not edit manually
Set-PSReadLineOption -HistoryNoDuplicates:$true
Set-PSReadLineOption -PredictionSource History
Import-Module posh-git
Set-Alias ll 'Get-ChildItem -Force'
# <<< managed by powershellknife
```

If psknife detects the block is corrupted (duplicate markers, mis-nested),
it refuses to write and asks you to fix the file manually (press `o` to
open it in your editor, `F5` to re-check).

## Safety model

- **Backups**: every apply copies the target file to
  `~/.powershellknife/backups/<timestamp>/` first.
- **Atomic writes**: write to `<target>.psknife-tmp`, then `rename` —
  no truncated file if psknife crashes.
- **Managed block**: the profile editor only touches lines between its
  markers. Everything else is preserved byte-exact.
- **No PowerShell execution**: psknife invokes `pwsh` exactly once, and
  only to read `Get-Command` output for the cmdlet inventory. It never
  runs PowerShell scripts for you.

## License

MIT OR Apache-2.0.
