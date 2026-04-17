## ADDED Requirements

### Requirement: Locate the PowerShell 7 user profile

L'outil SHALL cibler le profil `CurrentUser / CurrentHost` de PowerShell 7 sur Windows.

#### Scenario: Default PS7 profile path resolved
- **WHEN** `psknife` démarre sur une machine Windows
- **THEN** le chemin `~/Documents/PowerShell/Microsoft.PowerShell_profile.ps1` est résolu et affiché dans l'écran Profile

#### Scenario: Profile file absent
- **WHEN** le fichier de profil n'existe pas
- **THEN** le formulaire est présenté avec des valeurs par défaut et le fichier sera créé lors du premier save

### Requirement: Managed block delimiters

L'outil SHALL stocker les réglages qu'il édite entre deux marqueurs dédiés dans le fichier de profil.

#### Scenario: Block markers recognized
- **WHEN** le fichier de profil contient une paire de lignes correspondant exactement à `# >>> managed by powershellknife — do not edit manually` et `# <<< managed by powershellknife`
- **THEN** le contenu situé entre ces deux marqueurs est considéré comme le bloc managé

#### Scenario: No block present
- **WHEN** les marqueurs sont absents du fichier
- **THEN** aucun réglage n'est pré-rempli depuis le fichier et le bloc sera ajouté en fin de fichier lors du premier save

#### Scenario: Corrupted markers refused
- **WHEN** le fichier contient plusieurs marqueurs de début ou de fin, ou des marqueurs mal imbriqués
- **THEN** l'outil refuse toute écriture, affiche un message explicite et invite l'utilisateur à corriger manuellement

### Requirement: Preserve user code outside managed block

L'outil SHALL conserver octet pour octet tout contenu situé en dehors du bloc managé lors d'une écriture.

#### Scenario: Custom code untouched
- **WHEN** l'utilisateur a du code personnel avant et après le bloc managé (fonctions, imports, variables)
- **THEN** après un save, ce code est strictement identique à l'original (y compris les espaces, commentaires et ordre des lignes)

### Requirement: Expose common PSReadLine settings

Le formulaire SHALL exposer un jeu fixe de réglages PSReadLine courants.

#### Scenario: Boolean options rendered as checkboxes
- **WHEN** l'écran Profile est affiché
- **THEN** les options `HistoryNoDuplicates` et `HistorySearchCursorMovesToEnd` sont présentées comme des cases à cocher

#### Scenario: Enum options rendered as dropdowns
- **WHEN** l'écran Profile est affiché
- **THEN** les options `PredictionSource` (None/History/Plugin/HistoryAndPlugin), `EditMode` (Windows/Emacs/Vi) et `BellStyle` (None/Visual/Audible) sont présentées comme des sélecteurs à valeurs fixes

### Requirement: Manage auto-imported modules

L'outil SHALL permettre à l'utilisateur de gérer la liste des modules importés au démarrage de PowerShell via le formulaire.

#### Scenario: Add module
- **WHEN** l'utilisateur ajoute un module via le formulaire avec un nom non vide
- **THEN** une ligne `Import-Module <name>` est ajoutée dans le bloc managé au prochain save

#### Scenario: Remove module
- **WHEN** l'utilisateur supprime un module de la liste
- **THEN** la ligne `Import-Module <name>` correspondante est retirée du bloc managé au prochain save

### Requirement: Manage persistent aliases

L'outil SHALL permettre à l'utilisateur de gérer une liste d'alias persistants via le formulaire.

#### Scenario: Add alias
- **WHEN** l'utilisateur ajoute un alias avec un nom et une valeur non vides
- **THEN** une ligne `Set-Alias <name> '<value>'` est ajoutée dans le bloc managé au prochain save

#### Scenario: Edit alias
- **WHEN** l'utilisateur modifie la valeur d'un alias existant
- **THEN** la ligne correspondante est mise à jour (et non dupliquée) au prochain save

### Requirement: Round-trip fidelity

L'outil SHALL produire un bloc managé identique à l'original lorsqu'il lit puis écrit un fichier de profil sans modification du modèle.

#### Scenario: Read then write is a no-op
- **WHEN** l'outil lit un bloc managé bien formé puis écrit le fichier sans modification du modèle
- **THEN** le contenu du bloc après écriture est identique au contenu d'origine (même ordre, mêmes options)

### Requirement: Backup before modification

L'outil SHALL créer une copie horodatée du fichier de profil avant toute écriture.

#### Scenario: Timestamped backup directory created
- **WHEN** l'utilisateur applique des modifications au profil
- **THEN** le fichier d'origine est copié vers `~/.powershellknife/backups/<YYYY-MM-DD_HHMMSS>/Microsoft.PowerShell_profile.ps1` avant toute modification

### Requirement: Atomic write

L'outil SHALL effectuer l'écriture du fichier de profil de façon atomique.

#### Scenario: Write uses temp file and rename
- **WHEN** l'outil écrit le nouveau profil
- **THEN** les données sont d'abord écrites dans un fichier temporaire dans le même dossier, puis renommées vers la cible finale via une opération atomique

### Requirement: Display custom code section read-only

Le formulaire SHALL montrer l'existence du code custom du profil sans permettre de l'éditer.

#### Scenario: Custom code section collapsed by default
- **WHEN** l'écran Profile est affiché
- **THEN** une section "Code custom du profil" existe, repliée par défaut, et son contenu est affiché en lecture seule lorsqu'elle est dépliée
