## Why

La maintenance d'un environnement PowerShell (nettoyage d'historique, ajustement du profil et des options PSReadLine) se fait aujourd'hui soit à la main dans un éditeur, soit via des one-liners PS mémorisés de tête. Deux douleurs récurrentes :

- **Historique PSReadLine pollué** par des doublons et des typos (`Get-Procss`, `docer ps`) qui encombrent la recherche et les suggestions.
- **Profil PS intimidant à éditer** pour les réglages courants : il faut connaître le nom exact du setting, ouvrir un fichier `.ps1`, ne rien casser, relancer la session pour vérifier.

`powershellknife` est une TUI (Rust + ratatui) qui traite ces deux cas sans demander à l'utilisateur d'écrire du PowerShell. Le choix "pas écrit en PS" est délibéré : on veut un binaire unique, utilisable même si le profil est cassé, et une vraie interface interactive plutôt qu'une série de prompts.

## What Changes

MVP avec deux fonctionnalités, livré en un binaire Windows `psknife.exe` :

1. **History cleaner** — détecte les doublons et les typos dans `ConsoleHost_history.txt`, propose des corrections, applique les changements après backup.
2. **Profile/Settings editor** — formulaire TUI pour éditer les réglages PSReadLine courants, les modules auto-importés et les alias persistants. Écrit dans un **bloc managé** délimité (`# >>> powershellknife` / `# <<<`) pour préserver le code custom de l'utilisateur.

Filet de sécurité commun : backup horodaté sous `~/.powershellknife/backups/<timestamp>/` avant toute écriture, avec commande `psknife restore` pour revenir en arrière.

## Capabilities

### New Capabilities
- `history-cleaner`: détection et correction interactive des doublons et typos dans l'historique PSReadLine, avec preview et backup.
- `profile-editor`: lecture/écriture d'un bloc managé dans le profil PowerShell pour exposer les réglages courants (PSReadLine, modules, alias) via un formulaire TUI, sans toucher au code custom.

### Modified Capabilities
(aucune — premier projet)

## Impact

- **Nouveau dépôt Rust** : crate `powershellknife`, binaire `psknife`.
- **Dépendances Rust** : `ratatui`, `crossterm`, `strsim`, `serde`/`serde_json` (cache inventaire), `anyhow`, `clap` (sous-commandes `restore`, futures extensions).
- **Cibles MVP** : Windows uniquement, PowerShell 7+, profil `CurrentUser/CurrentHost` uniquement.
- **Distribution** : GitHub Releases (artefact `.exe`). scoop/winget à envisager plus tard.
- **Fichiers utilisateur touchés** :
  - lecture/écriture `$env:APPDATA\Microsoft\Windows\PowerShell\PSReadLine\ConsoleHost_history.txt`
  - lecture/écriture `~/Documents/PowerShell/Microsoft.PowerShell_profile.ps1`
  - création `~/.powershellknife/` (cache inventaire cmdlets + backups)
- **Invocation PowerShell externe** : le tool appelle `pwsh -NoProfile -Command "Get-Command ..."` une fois pour peupler l'inventaire de cmdlets ; cette invocation est une source de données, pas du code métier en PS.
