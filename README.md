# LumaSync

[![CI](https://github.com/RemiPelloux/lumasync/actions/workflows/ci.yml/badge.svg)](https://github.com/RemiPelloux/lumasync/actions/workflows/ci.yml)

Ambilight local et réactif pour Philips Hue, construit avec Rust, Tauri et React/TypeScript.

LumaSync capture les couleurs aux bords de l’écran et les diffuse vers une zone Hue Entertainment. Le traitement reste entièrement local : aucune image de l’écran ni clé Hue ne quitte le PC.

## Fonctionnalités

- streaming basse latence via Hue Entertainment et DTLS 1.2 PSK ;
- capture GPU persistante, avec abandon automatique des images devenues obsolètes ;
- quatre zones configurables en disposition **Bords** ou **Angles** ;
- profils **Cinéma**, **Jeu** et **Naturel** ;
- extraction perceptuelle des couleurs avec rejet des sous-titres et HUD blancs ;
- lissage prédictif, anti-scintillement et détection automatique des bandes noires ;
- adaptation de la charge aux écrans haute résolution ;
- télémétrie en direct : FPS, temps de traitement et images manquées ;
- détection du Hue Bridge et création d’une zone Entertainment depuis une pièce existante.

## Matériel et système

- Windows 10 ou Windows 11 ;
- Hue Bridge v2, sur le même réseau local que le PC ;
- au moins une lampe compatible Hue Entertainment ;
- une zone Entertainment existante, ou une pièce Hue depuis laquelle LumaSync pourra en créer une.

Le signal vidéo peut arriver à l’écran en DVI, HDMI ou DisplayPort : LumaSync capture directement l’image rendue par Windows.

## Démarrage

1. Lancez LumaSync et choisissez **Détecter le pont**.
2. Appuyez sur le bouton central du Hue Bridge.
3. Choisissez **Associer le pont**.
4. Sélectionnez l’écran, la zone Entertainment et la position de chaque lampe.
5. Démarrez l’éclairage.

Si la pièce apparaît mais pas la zone Entertainment, utilisez **Créer depuis cette pièce**. La pièce et ses automatismes ne sont pas modifiés ; une configuration Entertainment distincte est ajoutée.

## Développement

Installez Node.js 22+, Rust 1.88+ et les [prérequis Tauri pour Windows](https://v2.tauri.app/start/prerequisites/), puis :

```powershell
npm install
npm run tauri dev
```

Commandes utiles :

```powershell
npm run check
npm test
npm run build
npm run tauri build
```

Le build Release produit :

- `src-tauri/target/release/lumasync.exe` ;
- `src-tauri/target/release/bundle/nsis/LumaSync_<version>_x64-setup.exe`.

## Architecture

```text
React / TypeScript        configuration, aperçu et télémétrie
        │ Tauri IPC
Rust                     capture, analyse et ordonnancement temps réel
        │ DTLS / UDP local
Hue Bridge               distribution Hue Entertainment vers les lampes
```

Le moteur lit une seule image cohérente pour toutes les zones, calcule chaque zone unique une seule fois, puis envoie les couleurs dans la même trame Entertainment. Les ressources Hue indépendantes sont chargées en parallèle afin de limiter les allers-retours réseau.

## Sécurité et confidentialité

- les identifiants Hue sont stockés dans le gestionnaire d’identifiants Windows ;
- les captures restent en mémoire et ne sont jamais enregistrées ;
- aucune télémétrie distante n’est envoyée ;
- le pont est contacté uniquement sur le réseau local.

Les contenus vidéo protégés par DRM peuvent apparaître noirs dans une capture logicielle.

## Documentation du design

Les principes visuels, composants et comportements d’interaction sont documentés dans [`DESIGN.md`](DESIGN.md). Les bibliothèques et licences tierces sont recensées dans [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

## Statut

LumaSync est un projet indépendant et n’est ni affilié ni approuvé par Signify ou Philips Hue. Philips Hue est une marque de son propriétaire respectif.
