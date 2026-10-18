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
- Le fichier de sortie doit toujours être finalisé proprement (envoi d'EOS avant de passer le pipeline à `Null`).
- Garde la description du pipeline GStreamer lisible et commentée.
- Ne lance jamais `sudo`. Si une dépendance système manque, dis-moi quel paquet installer.

## Feuille de route
Voir la section « Roadmap » du README.md.
