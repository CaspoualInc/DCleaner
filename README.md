# DCleaner

Analyseur d'espace disque moderne pour Windows : arborescence triée + treemap imbriqué synchronisés,
simulation avant toute suppression, menu contextuel natif Windows 11. Tauri (Rust + interface web).
Aucune télémétrie, aucun réseau.

## Construire et lancer

```bash
cargo build --release          # target\release\dcleaner.exe + dcleaner_shell.dll
target\release\dcleaner.exe    # ou : dcleaner.exe "D:\Projets"  (analyse directement ce dossier)
cargo test -p dcleaner         # tests du moteur
```

## Installateur (Windows 10/11, 64 bits)

```bash
cargo install tauri-cli --version "^2" --locked   # une seule fois
powershell -ExecutionPolicy Bypass -File scripts\build-installer.ps1
```

Produit `target\release\bundle\nsis\DCleaner_1.0.0_x64-setup.exe` (installation pour tous les utilisateurs,
demande les droits administrateur, désinstallation dans « Applications installées »).
Il installe l'app, la DLL du menu contextuel et enregistre le menu Windows 11. Le paquet du menu est signé par un
certificat **auto-signé** (créé au build, clé privée conservée dans votre magasin utilisateur) ; l'installateur
l'approuve dans « Personnes autorisées » pour que Windows accepte le paquet sans mode développeur.
Pour une diffusion large, remplacez-le par un vrai certificat de signature de code.

## Menu contextuel Windows 11 en développement (sans installateur, mode développeur)

Prérequis : mode développeur activé (Paramètres → Système → Pour les développeurs).

```bash
powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -RestartExplorer
powershell -ExecutionPolicy Bypass -File scripts\uninstall.ps1
```

`install.ps1` copie l'app dans `dist\` et enregistre un paquet MSIX « sparse » non signé
(`package\AppxManifest.xml`) qui expose `dcleaner_shell.dll` (commande `IExplorerCommand`) à l'Explorateur.
Le menu moderne ne couvre que les **dossiers** (Windows refuse le type `Drive` dans ce mécanisme) : pour un
lecteur, ouvrez-le puis clic droit sur un de ses dossiers, ou choisissez-le dans l'écran Volumes de l'app.
`-RestartExplorer` ferme brièvement vos fenêtres Explorateur (nécessaire si la DLL est déjà chargée).
L'ancienne version Python (dossier `dcleaner\`) avait une entrée « classique » : retirez-la avec
`python dcleaner --uninstall-menu`, puis vous pouvez supprimer `dcleaner\` et `tests\`.

## Utilisation

- Clic sur un bloc du treemap : sélection (l'arbre se synchronise) ; double-clic : on entre dans le dossier.
  Clic sur un dossier de l'arbre : le treemap l'affiche. Ctrl/Maj + clic : sélection multiple.
- **Résidus connus** (node_modules, target/, bin/obj, caches Adobe/shaders…, filtrables par profil
  Développeur/Jeux/Créatif) et **Doublons** (≥ 1 Mo, XXH3) agissent sur le dossier affiché.
- Toute suppression passe par une fenêtre de simulation (liste exacte + espace libéré), puis Corbeille
  Windows (restaurable). Les zones système (Windows, System Volume Information, racines de lecteur…)
  sont refusées par le moteur lui-même, quelle que soit la forme du chemin (`..`, `\\`, noms courts, liens).
- Clavier dans l'arbre : flèches, Suppr.

## Limites connues

- Scan parallèle via `read_dir` (pas de lecture directe de la MFT) ; taille logique ; dossiers refusés ignorés
  sans droits administrateur. Fichiers OneDrive « dans le cloud » comptés pour 0 octet.
- Paquet non signé : usage personnel (mode développeur). Pour d'autres PC il faut signer le MSIX.
- Suppression : Windows uniquement. Pas encore : désinstallation propre, effacement sécurisé, historique de croissance.
