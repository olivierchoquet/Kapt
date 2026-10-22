# kapt — instructions pour Claude Code

## Le projet
kapt est un enregistreur d'écran en ligne de commande pour Linux, pensé d'abord pour KDE Plasma sous Wayland.
Chaîne de capture : portail xdg-desktop-portal (ScreenCast) → PipeWire → GStreamer → fichier MKV.

## Langue
Réponds en français. Le code (identifiants, commentaires, messages affichés) est en anglais.

## Pile technique
- Rust édition 2021
- `ashpd` 0.11 : portail ScreenCast (D-Bus)
- `gstreamer` 0.23 : pipeline de capture et d'encodage
- `tokio` : runtime asynchrone
- `anyhow` : gestion des erreurs

Ne mets pas à jour `ashpd` ni `gstreamer` vers une version majeure plus récente sans me le demander : leurs API ont changé.

## Commandes
- Vérifier : `cargo check`
- Compiler : `cargo build`
- Lint : `cargo clippy -- -D warnings`
- Formater : `cargo fmt`
- Lancer : `cargo run` (ouvre la boîte de dialogue KDE, nécessite une session graphique ; ne le lance pas sans me demander)

## Règles
- Après chaque modification du code, lance `cargo fmt`, puis `cargo clippy -- -D warnings`, et corrige les avertissements.
- Pas de `unwrap()` dans le code applicatif : utilise `?` et `anyhow::Context`.
- Le fichier de sortie doit être finalisé proprement quand l'utilisateur arrête l'enregistrement (EOS poussé depuis la sortie de `pipewiresrc` avant de passer le pipeline à `Null`). Un arrêt extérieur ou une erreur est seulement signalé (voir `docs/DECISIONS.md`).
- Garde la description du pipeline GStreamer lisible et commentée.
- Ne lance jamais `sudo`. Si une dépendance système manque, dis-moi quel paquet installer.
- Après chaque problème réglé ou décision prise, ajoute une entrée en tête de `docs/DECISIONS.md` (en anglais : problème, décision, pistes écartées, conséquences, commit), dans le même commit que le code. Un problème non résolu va dans la section « Open issues » ; retire-le de là quand il est réglé.

## Feuille de route
Voir la section « Roadmap » du README.md.
