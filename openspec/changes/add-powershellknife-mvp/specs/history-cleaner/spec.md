## ADDED Requirements

### Requirement: Locate the PSReadLine history file

L'outil SHALL déterminer automatiquement le chemin du fichier d'historique PSReadLine pour l'utilisateur courant sous Windows.

#### Scenario: Default Windows path resolved
- **WHEN** `psknife` démarre sur une machine Windows avec PowerShell 7 installé
- **THEN** le chemin `$env:APPDATA\Microsoft\Windows\PowerShell\PSReadLine\ConsoleHost_history.txt` est résolu et affiché dans l'en-tête de l'écran History

#### Scenario: Missing history file
- **WHEN** le fichier d'historique n'existe pas
- **THEN** l'écran History affiche un message explicite "Aucun historique trouvé" et aucune action d'écriture n'est proposée

### Requirement: Detect duplicate history entries

L'outil SHALL identifier les groupes de commandes identiques dans l'historique.

#### Scenario: Identical commands grouped
- **WHEN** l'historique contient plusieurs lignes dont le contenu est identique après trim des espaces de début et de fin
- **THEN** ces lignes sont présentées comme un seul groupe de doublons avec le compteur d'occurrences et les numéros de ligne

#### Scenario: Case sensitivity
- **WHEN** deux lignes ne diffèrent que par la casse (`Get-Process` vs `get-process`)
- **THEN** elles ne sont PAS considérées comme doublons (on préserve l'intention de l'utilisateur)

### Requirement: Detect likely typos

L'outil SHALL signaler les commandes dont le premier token ressemble à une cmdlet connue mais n'existe dans aucune source reconnue.

#### Scenario: Close match found in inventory
- **WHEN** une ligne commence par `Get-Procss` et que `Get-Process` existe dans l'inventaire, avec une distance de Levenshtein de 1
- **THEN** la ligne est signalée comme typo probable avec suggestion `Get-Process`

#### Scenario: External executable excluded
- **WHEN** une ligne commence par un nom d'exécutable externe connu (ex. `git`, `docker`, `npm`, `cargo`, `node`, `python`)
- **THEN** la ligne n'est PAS signalée comme typo, même si elle n'est pas dans l'inventaire des cmdlets

#### Scenario: Ambiguous match skipped
- **WHEN** plusieurs candidats sont à la même distance minimale (≤ 2) sans écart net
- **THEN** aucune suggestion n'est affichée (on évite le bruit)

#### Scenario: Path-like token skipped
- **WHEN** le premier token commence par `.\`, `./`, `/`, une lettre de disque `C:\`, ou un `~`
- **THEN** la ligne n'est PAS signalée comme typo (c'est un appel de script)

### Requirement: Preview changes before apply

L'outil SHALL présenter toute modification à l'utilisateur sous forme de diff avant de l'écrire sur disque.

#### Scenario: Preview shows pending actions
- **WHEN** l'utilisateur demande une prévisualisation (touche dédiée)
- **THEN** un panneau de diff affiche les lignes à supprimer, remplacer et conserver, avec leurs numéros de ligne d'origine

#### Scenario: Apply requires explicit confirmation
- **WHEN** l'utilisateur tente d'appliquer des modifications
- **THEN** une confirmation clavier explicite est requise avant l'écriture

### Requirement: Backup before modification

L'outil SHALL créer une copie horodatée du fichier d'historique avant toute écriture.

#### Scenario: Timestamped backup directory created
- **WHEN** l'utilisateur applique des modifications à l'historique
- **THEN** le fichier d'origine est copié vers `~/.powershellknife/backups/<YYYY-MM-DD_HHMMSS>/ConsoleHost_history.txt` avant toute modification

#### Scenario: Restore command available
- **WHEN** l'utilisateur lance `psknife restore`
- **THEN** la liste des backups disponibles est présentée et l'utilisateur peut restaurer un backup choisi

### Requirement: Atomic write

L'outil SHALL effectuer l'écriture du fichier d'historique de façon atomique pour éviter toute troncature en cas de crash.

#### Scenario: Write uses temp file and rename
- **WHEN** l'outil écrit le nouvel historique
- **THEN** les données sont d'abord écrites dans un fichier temporaire dans le même dossier, puis renommées vers la cible finale via une opération atomique

### Requirement: Bulk actions

L'outil SHALL permettre à l'utilisateur d'appliquer des actions groupées sans parcourir chaque entrée.

#### Scenario: Auto-fix high-confidence typos
- **WHEN** l'utilisateur déclenche "Auto-fix tous les typos"
- **THEN** toutes les lignes signalées comme typos avec une suggestion unique et un écart de distance ≥ 1 sont marquées "Replace", les autres restent "Keep"

#### Scenario: Collapse all duplicates
- **WHEN** l'utilisateur déclenche "Collapse tous les doublons"
- **THEN** pour chaque groupe, seule l'occurrence la plus récente est conservée et les précédentes sont marquées "Delete"
