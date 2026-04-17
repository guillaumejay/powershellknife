## Context

PowerShell est extrêmement configurable mais la maintenance courante (historique, profil, options PSReadLine) reste manuelle. PSReadLine stocke l'historique en texte brut dans `ConsoleHost_history.txt` ; il offre l'option `HistoryNoDuplicates` mais uniquement de façon prospective — les doublons déjà présents restent. Le profil utilisateur est un simple `.ps1` dont l'utilisateur lambda hésite à modifier les réglages de peur de casser le reste.

Le choix structurant du projet est d'écrire l'outil **en Rust**, pas en PowerShell. Motivations : TUI keyboard-first type lazygit, binaire unique distribuable, utilisable même si le setup PS est cassé, pas d'ironie "un nettoyeur qui peut se casser lui-même à cause d'un profil pété".

## Goals / Non-Goals

**Goals:**
- Binaire unique `psknife.exe`, zéro dépendance runtime hors PowerShell 7 déjà installé.
- TUI ratatui fluide, clavier-only, navigation type lazygit.
- Édition de l'historique et du profil **réversible** (backup systématique avant écriture).
- Préservation stricte du code custom dans le profil de l'utilisateur.
- Détection de typos utile et peu bruyante (peu de faux positifs).

**Non-Goals:**
- Support de Windows PowerShell 5.1 au MVP (viendra si besoin).
- Support des profils AllUsers ou autres hosts (VSCode, ISE) au MVP.
- Support Linux/macOS au MVP (PS 7 y existe, mais les chemins et priorités diffèrent).
- Éditeur de code généraliste dans la TUI (pas un mini-VSCode).
- GUI native (Tauri, WPF) — exclu au profit du TUI.
- Gestion de plusieurs machines / sync de config cross-machine.
- Rédiger du PowerShell métier au runtime (l'outil n'exécute pas de script PS, il se contente d'invoquer `pwsh` une fois pour récolter l'inventaire des cmdlets).

## Decisions

### D1. Stack : Rust + ratatui + crossterm
- ratatui est la bibliothèque TUI Rust la plus mature (fork maintenu de tui-rs), avec un bon modèle de widgets et un backend crossterm cross-plateforme.
- strsim pour les distances d'édition (Levenshtein, Jaro-Winkler).
- clap pour les sous-commandes (`psknife`, `psknife restore`, futures extensions).
- serde/serde_json pour le cache d'inventaire des cmdlets.

Alternative écartée : **Go + bubbletea** — parfaitement viable, mais ratatui a une API plus fine pour la mise en page riche souhaitée (tableaux, panneaux à onglets, preview diff).

### D2. Bloc managé pour le profil
Le profil est modifié uniquement entre deux marqueurs :

```powershell
# >>> managed by powershellknife — do not edit manually
Set-PSReadLineOption -HistoryNoDuplicates
Set-PSReadLineOption -PredictionSource History
Import-Module posh-git
Set-Alias ll 'Get-ChildItem -Force'
# <<< managed by powershellknife
```

L'outil lit uniquement ce bloc pour peupler le formulaire, et récrit uniquement ce bloc lors du save. Tout ce qui est en dehors est conservé verbatim, incluant les imports et fonctions custom de l'utilisateur.

Si le bloc n'existe pas encore, il est ajouté en fin de fichier lors du premier save.

Alternatives écartées :
- **Regex sur les lignes connues** : fragile, casse sur des formats inhabituels (`Set-PSReadlineOption -HistoryNoDuplicates:$true` vs `-HistoryNoDuplicates`).
- **Parser AST officiel via invocation pwsh** : plus fiable mais couplage fort à pwsh et complexité disproportionnée pour le MVP.

Précédents bien acceptés : `starship init`, `zoxide init`, `oh-my-posh init`, `nvm init` écrivent tous dans des blocs balisés des rc-files.

### D3. Inventaire des cmdlets — hybride avec cache
Pour détecter les typos, il faut une liste de cmdlets/alias/fonctions considérés "valides".

- **Liste core embarquée** (~200 cmdlets du module `Microsoft.PowerShell.*`) → disponible offline, zéro latence au premier lancement.
- **Scan du setup réel** via `pwsh -NoProfile -Command "Get-Command | Select Name,CommandType | ConvertTo-Json -Compress"` → capte les modules installés par l'utilisateur (posh-git, Az, etc.). Invoqué au 1er lancement et rafraîchissable via une action TUI.
- **Cache disque** : `~/.powershellknife/cmdlet_inventory.json` avec timestamp. Proposition de refresh si > 30 jours.

### D4. Détection des doublons
Un doublon = commandes identiques au trim près (espaces de début/fin). Casse sensible (PS est case-insensitive mais on n'applatit pas pour ne pas perdre l'intention de l'utilisateur).

Stratégie "collapse" par défaut : garder la plus récente, supprimer les précédentes. L'utilisateur voit un groupe et peut choisir : collapse, keep all, ou sélection manuelle.

### D5. Détection des typos
Pour chaque ligne de l'historique :
1. Extraire le premier token (le nom de cmdlet/commande).
2. Lookup dans l'inventaire (cmdlets + alias + fonctions).
3. Si absent, chercher un candidat à distance de Levenshtein ≤ 2 parmi l'inventaire, restreint aux cmdlets commençant par la même lettre pour limiter le bruit.
4. Ignorer les chemins (`.\script.ps1`, `C:\...`) et les exécutables externes communs (`git`, `docker`, `npm`, etc. — liste denylist intégrée).
5. Afficher la suggestion uniquement si candidat unique ou très nettement meilleur que le second (écart de distance ≥ 1).

Seuil ajustable plus tard si trop de faux positifs.

### D6. Backup systématique
Avant toute écriture (historique ou profil) :

```
~/.powershellknife/backups/
  └── 2026-04-17_143022/
      ├── ConsoleHost_history.txt
      └── Microsoft.PowerShell_profile.ps1
```

Commande `psknife restore` liste les backups et restaure le dernier (ou un sélectionné). Les backups ne sont jamais supprimés automatiquement au MVP (on verra un gc plus tard).

### D7. Écriture atomique
Pattern write-temp-rename :
1. Écrire dans `<target>.psknife-tmp`.
2. `fs::rename` vers la cible (atomique sur NTFS pour un même volume).

Évite les fichiers tronqués en cas de crash.

### D8. Structure du crate

```
powershellknife/
├── Cargo.toml
├── src/
│   ├── main.rs              # clap dispatch
│   ├── app.rs               # state + event loop ratatui
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── history.rs       # écran History cleaner
│   │   └── profile.rs       # écran Profile editor
│   ├── history/
│   │   ├── mod.rs
│   │   ├── parse.rs         # parsing ConsoleHost_history.txt
│   │   ├── dedup.rs
│   │   └── typos.rs
│   ├── profile/
│   │   ├── mod.rs
│   │   ├── block.rs         # gestion du bloc managé
│   │   └── settings.rs      # modèle des settings exposés
│   ├── inventory.rs         # cmdlet inventory + cache
│   ├── backup.rs
│   └── paths.rs             # résolution des chemins Windows
└── tests/
    ├── fixtures/            # samples d'historique et de profils
    └── integration.rs
```

## Risks / Trade-offs

| Risque | Impact | Mitigation |
|--------|--------|------------|
| Profil corrompu après write | utilisateur locked out de PS | backup systématique + write atomique + test de parse pwsh avant rename (optionnel post-MVP) |
| Historique modifié pendant que PS tourne | perte de commandes récentes | avertir si une session `pwsh` est détectée active (snapshot puis replay optionnel post-MVP) |
| Faux positifs de typos | expérience frustrante | seuil conservateur, denylist d'exécutables, toujours proposer "keep" avant "delete" |
| Inventaire pas à jour (nouveau module installé) | typo non détectée ou faux positif | action "Refresh inventory" accessible dans la TUI, proposition auto si > 30j |
| ratatui breaking changes entre versions | maintenance | pin strict + vérifier la compat à chaque bump |
| Parser custom du profil incapable de gérer des blocs managés partiellement corrompus | write refusé | en cas d'ambiguïté (marqueurs en double, mal imbriqués), refuser d'écrire et proposer à l'utilisateur d'ouvrir le fichier manuellement |
