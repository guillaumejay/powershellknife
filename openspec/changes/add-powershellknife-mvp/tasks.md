## 1. Scaffold & infra

- [x] 1.1 Initialiser le crate `powershellknife` avec binaire `psknife` (Cargo.toml, rustfmt, clippy)
- [x] 1.2 Ajouter dépendances : ratatui, crossterm, clap, strsim, serde, serde_json, anyhow, chrono
- [x] 1.3 Mettre en place la structure modulaire (src/ui, src/history, src/profile, src/inventory, src/backup, src/paths)
- [x] 1.4 Implémenter la résolution des chemins Windows (history, profile CU/CH PS 7, dossier `~/.powershellknife/`)
- [x] 1.5 GitHub Actions : lint (fmt + clippy), build, tests, artifact `.exe` sur tag

## 2. Backup & safety

- [x] 2.1 Module `backup.rs` : créer un dossier horodaté, copier les fichiers ciblés avant modification
- [x] 2.2 Écriture atomique (write-temp + rename) pour toute mutation de fichier
- [x] 2.3 Sous-commande `psknife restore` : lister les backups, restaurer le dernier ou un backup choisi

## 3. Cmdlet inventory

- [x] 3.1 Embarquer une liste core (~200 cmdlets Microsoft.PowerShell.*) en JSON compilé
- [x] 3.2 Module `inventory.rs` : lancer `pwsh -NoProfile -Command "..."` pour récolter `Get-Command`
- [x] 3.3 Sérialiser le cache dans `~/.powershellknife/cmdlet_inventory.json` avec timestamp
- [x] 3.4 Détection "cache > 30j" et proposition de refresh

## 4. History cleaner — backend

- [x] 4.1 Parser de `ConsoleHost_history.txt` (ligne simple + support des continuations backtick)
- [x] 4.2 Détection de doublons : groupement par commande trimmée, conservation de l'ordre
- [x] 4.3 Détection de typos : extraction du premier token, lookup inventaire, Levenshtein ≤ 2 avec seuil d'écart
- [x] 4.4 Denylist d'exécutables externes (git, docker, npm, node, python, cargo, go, ...)
- [x] 4.5 Modèle de plan d'édition (liste d'actions : delete, replace, keep) + serialization pour preview
- [x] 4.6 Tests unitaires sur fixtures d'historique (doublons, typos, cas limites)

## 5. History cleaner — UI

- [x] 5.1 Écran ratatui : en-tête de stats, liste des entrées signalées, panneau de détail
- [x] 5.2 Navigation clavier (j/k ou flèches, tabs pour filtrer doublons/typos)
- [x] 5.3 Actions par entrée : Delete, Replace, Keep, Collapse (pour les groupes)
- [x] 5.4 Actions bulk : Auto-fix tous les typos à haute confiance, Collapse tous les doublons
- [x] 5.5 Preview diff avant apply (F5 ou `p`)
- [x] 5.6 Apply → backup + écriture atomique + toast de confirmation

## 6. Profile editor — backend

- [x] 6.1 Module `profile/block.rs` : détection/lecture/écriture du bloc managé (`# >>> ... # <<<`)
- [x] 6.2 Gestion de l'absence de bloc (création en fin de fichier au 1er save)
- [x] 6.3 Refus d'écriture si bloc corrompu (marqueurs en double, mal imbriqués) avec message clair
- [x] 6.4 Modèle `settings.rs` des réglages exposés :
  - PSReadLine : HistoryNoDuplicates, HistorySearchCursorMovesToEnd, PredictionSource, EditMode, BellStyle
  - Modules auto-importés (liste)
  - Alias persistants (map name → value)
- [x] 6.5 Parsing des lignes du bloc vers le modèle (`Set-PSReadLineOption`, `Import-Module`, `Set-Alias`)
- [x] 6.6 Sérialisation du modèle vers les lignes du bloc (stable, ordre déterministe)
- [x] 6.7 Tests unitaires sur round-trip parse/serialize et sur préservation du code hors bloc

## 7. Profile editor — UI

- [x] 7.1 Écran ratatui : sections (PSReadLine, Modules, Aliases), chacune éditable
- [x] 7.2 Widgets : checkbox pour bool, dropdown (liste déroulante avec ← →) pour enum
- [x] 7.3 Ajout/suppression de module (prompt + validation basique du nom)
- [x] 7.4 Ajout/édition/suppression d'alias (prompt nom + valeur)
- [x] 7.5 Section "Code custom du profil" en lecture seule, repliée par défaut
- [x] 7.6 Apply → backup + écriture atomique + toast de confirmation

## 8. Intégration TUI globale

- [x] 8.1 Layout principal avec barre d'onglets (History / Profile / About)
- [x] 8.2 Barre de statut permanente (raccourcis, chemin cible, état dirty/clean)
- [x] 8.3 Gestion propre de la sortie (F10 / Ctrl-C, prompt si dirty)
- [x] 8.4 Gestion des erreurs runtime (afficher dans un toast/panneau, jamais de panic visible à l'utilisateur)

## 9. Polish & release

- [x] 9.1 README.md : installation, captures d'écran, commandes de base
- [x] 9.2 Tests d'intégration end-to-end sur environnement temporaire (tempdir avec fichiers fixtures)
- [ ] 9.3 Tag v0.1.0 + GitHub Release avec binaire `psknife.exe`
- [ ] 9.4 Collecter un retour d'utilisation réel avant d'élargir le scope (PS 5.1, autres profils, etc.)
