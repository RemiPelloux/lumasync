# LumaSync

[![CI](https://github.com/RemiPelloux/lumasync/actions/workflows/ci.yml/badge.svg)](https://github.com/RemiPelloux/lumasync/actions/workflows/ci.yml)

Ambilight local et réactif pour Philips Hue, construit avec Rust, Tauri et React/TypeScript.

LumaSync analyse l’écran en cônes orientés vers son centre et diffuse les couleurs vers une zone Hue Entertainment. Les images restent sur le PC ; seules les couleurs calculées et les échanges d’authentification sont envoyés au pont local.

## Fonctionnalités

- streaming basse latence via Hue Entertainment et DTLS 1.2 PSK ;
- capture DXGI sans file d’images, texture et tampon mémoire réutilisés ;
- quatre zones configurables en disposition **Bords** ou **Angles** ;
- profils **Cinéma**, **Jeu** et **Naturel** ;
- histogramme circulaire de teintes, variance Oklab et atténuation des petites zones blanches ;
- lissage prédictif, anti-scintillement et détection automatique des bandes noires ;
- grille fixe de 3 600 échantillons au maximum, indépendante de la résolution ;
- télémétrie en direct : FPS, temps de traitement et images manquées ;
- détection du Hue Bridge et création d’une zone Entertainment depuis une pièce existante.
- mémorisation de l’écran, de la zone et du placement des lampes par pont ;
- journal de diagnostic local consultable et exportable, avec rotation et masquage des clés ;
- suivi allégé en arrière-plan et reprise des opérations après une erreur.

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
npm run check:size
npm test
npm run test:frontend
npm run test:ui
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

Les formules, tests, corrections de latence et limites sont décrits dans
[`docs/ALGORITHM.md`](docs/ALGORITHM.md). La prédiction porte sur les couleurs des
zones pendant au plus 12 ms ; elle ne devine pas les pixels des images futures.
Le moteur cible le SDR sRGB, sans calibration HDR ou optique des lampes.

Tous les fichiers rédigés sont limités à 349 lignes par `npm run check:size` et la CI.
Les fichiers de verrouillage générés Cargo/npm restent intacts et sont exclus.

## Sécurité et confidentialité

- les identifiants Hue sont stockés dans le gestionnaire d’identifiants Windows ;
- les captures restent en mémoire et ne sont jamais enregistrées ;
- aucune télémétrie distante n’est envoyée ;
- le pont est contacté uniquement sur le réseau local.

Le bouton **Diagnostic** de la barre supérieure affiche les 200 événements les
plus récents, leur heure et leur origine. Les erreurs de capture, reconnexions,
changements de moteur DXGI et échecs de commandes y sont consignés. Le chemin du
journal apparaît dans cette fenêtre ; son export produit un fichier JSONL.
L’écriture passe par une file bornée hors de la boucle de capture. Le journal
courant et sa sauvegarde sont limités à 512 Kio chacun. Les captures et les clés
Hue ne sont pas enregistrées. Les logs restent sur le PC.

`npm run dev` ouvre un aperçu navigateur avec des données simulées. Le pilotage
réel du pont et la capture d’écran nécessitent l’application Tauri. Les tests
Playwright utilisent Edge sous Windows et Chromium sur les autres systèmes.

Les contenus vidéo protégés par DRM peuvent apparaître noirs dans une capture logicielle.

## Documentation du design

Les principes visuels, composants et comportements d’interaction sont documentés dans [`DESIGN.md`](DESIGN.md). Les bibliothèques et licences tierces sont recensées dans [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

## Statut

LumaSync est un projet indépendant et n’est ni affilié ni approuvé par Signify ou Philips Hue. Philips Hue est une marque de son propriétaire respectif.
