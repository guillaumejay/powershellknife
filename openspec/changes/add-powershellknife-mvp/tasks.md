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

- [ ] 3.1 Embarquer une liste core (~200 cmdlets Microsoft.PowerShell.*) en JSON compilé
- [ ] 3.2 Module `inventory.rs` : lancer `pwsh -NoProfile -Command "..."` pour récolter `Get-Command`
- [ ] 3.3 Sérialiser le cache dans `~/.powershellknife/cmdlet_inventory.json` avec timestamp
- [ ] 3.4 Détection "cache > 30j" et proposition de refresh

## 4. History cleaner — backend

- [ ] 4.1 Parser de `ConsoleHost_history.txt` (ligne simple + support des continuations backtick)
- [ ] 4.2 Détection de doublons : groupement par commande trimmée, conservation de l'ordre
- [ ] 4.3 Détection de typos : extraction du premier token, lookup inventaire, Levenshtein ≤ 2 avec seuil d'écart
- [ ] 4.4 Denylist d'exécutables externes (git, docker, npm, node, python, cargo, go, ...)
- [ ] 4.5 Modèle de plan d'édition (liste d'actions : delete, replace, keep) + serialization pour preview
- [ ] 4.6 Tests unitaires sur fixtures d'historique (doublons, typos, cas limites)

## 5. History cleaner — UI

- [ ] 5.1 Écran ratatui : en-tête de stats, liste des entrées signalées, panneau de détail
- [ ] 5.2 Navigation clavier (j/k ou flèches, tabs pour filtrer doublons/typos)
- [ ] 5.3 Actions par entrée : Delete, Replace, Keep, Collapse (pour les groupes)
- [ ] 5.4 Actions bulk : Auto-fix tous les typos à haute confiance, Collapse tous les doublons
- [ ] 5.5 Preview diff avant apply (F5 ou `p`)
- [ ] 5.6 Apply → backup + écriture atomique + toast de confirmation

## 6. Profile editor — backend

- [ ] 6.1 Module `profile/block.rs` : détection/lecture/écriture du bloc managé (`# >>> ... # <<<`)
- [ ] 6.2 Gestion de l'absence de bloc (création en fin de fichier au 1er save)
- [ ] 6.3 Refus d'écriture si bloc corrompu (marqueurs en double, mal imbriqués) avec message clair
- [ ] 6.4 Modèle `settings.rs` des réglages exposés :
  - PSReadLine : HistoryNoDuplicates, HistorySearchCursorMovesToEnd, PredictionSource, EditMode, BellStyle
  - Modules auto-importés (liste)
  - Alias persistants (map name → value)
- [ ] 6.5 Parsing des lignes du bloc vers le modèle (`Set-PSReadLineOption`, `Import-Module`, `Set-Alias`)
- [ ] 6.6 Sérialisation du modèle vers les lignes du bloc (stable, ordre déterministe)
- [ ] 6.7 Tests unitaires sur round-trip parse/serialize et sur préservation du code hors bloc

## 7. Profile editor — UI

- [ ] 7.1 Écran ratatui : sections (PSReadLine, Modules, Aliases), chacune éditable
- [ ] 7.2 Widgets : checkbox pour bool, dropdown (liste déroulante avec ← →) pour enum
- [ ] 7.3 Ajout/suppression de module (prompt + validation basique du nom)
- [ ] 7.4 Ajout/édition/suppression d'alias (prompt nom + valeur)
- [ ] 7.5 Section "Code custom du profil" en lecture seule, repliée par défaut
- [ ] 7.6 Apply → backup + écriture atomique + toast de confirmation

## 8. Intégration TUI globale

- [ ] 8.1 Layout principal avec barre d'onglets (History / Profile / About)
- [ ] 8.2 Barre de statut permanente (raccourcis, chemin cible, état dirty/clean)
- [ ] 8.3 Gestion propre de la sortie (F10 / Ctrl-C, prompt si dirty)
- [ ] 8.4 Gestion des erreurs runtime (afficher dans un toast/panneau, jamais de panic visible à l'utilisateur)

## 9. Polish & release

- [ ] 9.1 README.md : installation, captures d'écran, commandes de base
- [ ] 9.2 Tests d'intégration end-to-end sur environnement temporaire (tempdir avec fichiers fixtures)
- [ ] 9.3 Tag v0.1.0 + GitHub Release avec binaire `psknife.exe`
- [ ] 9.4 Collecter un retour d'utilisation réel avant d'élargir le scope (PS 5.1, autres profils, etc.)
