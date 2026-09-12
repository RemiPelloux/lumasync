# Vérification du 11 septembre 2026

## Tests et parcours

- 47 tests Rust réussis ; benchmark et test de capture ignorés par défaut.
- 13 tests frontend réussis : persistance, restauration, bornes, identité des
  lampes, moniteurs déplacés et comparaison du statut.
- 6 parcours Playwright réussis sous Edge : configuration, profils et placement
  après rechargement, démarrage et arrêt, erreur du moteur, export du journal,
  récupération du suivi, données de journal malformées et création depuis une pièce.
- Les parcours asynchrones vérifient aussi le double clic, une réponse de statut
  obsolète, l’absence de lectures concurrentes et la relance d’un arrêt incomplet.
- Captures inspectées à 1 040 × 680 et en fenêtres étroites. Les contrôles de
  débordement horizontal et de chevauchement passent à 320, 390 et 860 px.
- TypeScript strict, build Vite, format Rust, Clippy sans avertissement et contrôle
  de taille réussis. Les tests frontend et navigateur sont ajoutés à la CI ;
  la CI distante n’a pas été exécutée dans cette session.

## Analyse CPU avant/après

Même machine, images synthétiques 4K, 500 itérations après échauffement, build
Release. Ces valeurs mesurent uniquement l’analyse, sans capture ni flux Hue.

| Scène | Avant p50 | Après p50 |
| --- | --- | --- |
| Couleurs variées, 8 zones | 0,357 ms | 0,353 ms |
| Couleurs variées, 4 zones | 0,302 ms | 0,299 ms |
| Couleurs variées, 1 zone | 0,278 ms | 0,253 ms |
| Blanc uniforme, 8 zones | 0,281 ms | 0,033 ms |

Le gain est surtout visible sur les aplats grâce à la réutilisation exacte des
observations. Les scènes variées en huit zones restent pratiquement inchangées.

## Capture locale et limites

Le test DXGI lit le bureau 3 840 × 2 160 pendant deux secondes sans enregistrer
ni transmettre d’image : 78 images fraîches, copie p50 8,774 ms, appels p95
9,106 ms. Le test contient une pause de 16 ms par appel ; il ne mesure pas le
plafond de cadence du moteur ni le délai radio/optique des lampes.

Les parcours navigateur utilisent des données simulées et des fixtures IPC.
L’exécutable Release et l’installeur NSIS 0.3.0 sont générés. Le démarrage natif
est vérifié avec une fenêtre répondant aux messages Windows. Le journal réel
confirme ensuite une session Hue de quatre lampes à 45 i/s cible, la capture
DXGI, puis l’arrêt du worker, du flux et de l’application à la fermeture.
Le journal persiste dans `%LOCALAPPDATA%/com.local.lumasync/logs/diagnostics.jsonl`.
La stabilité des couleurs dans une pièce et la latence optique nécessitent encore
une mesure matérielle ; les événements de démarrage ne mesurent pas ces délais.

## Historique : 5 septembre 2026

## Automatique

- 27 tests Rust réussis ; deux tests matériels/benchmark ignorés par défaut.
- Ces deux tests ont aussi été lancés séparément en Release et réussissent.
- TypeScript strict, build Vite, Clippy avec avertissements interdits : réussis.
- Exécutable Windows et installeur NSIS 0.3.0 produits.
- Contrôle de taille : aucun fichier rédigé ne dépasse 349 lignes ;
  le plus grand est `src-tauri/src/lib.rs`, 316 lignes.
- Les fichiers de verrouillage générés sont explicitement exclus.
- L’ordre des déclarations et toutes les valeurs CSS ont été comparés à la
  version précédente : découpage identique, hors espaces.
- Audit frontend-design-premium strict : aucune erreur ni avertissement.
- Impeccable signale des avis sur les couleurs/rayons historiques non recensés
  dans DESIGN.md. Aucun redesign n’a été effectué pour masquer ces avis.

## Mesures locales Release

Sur cet écran Windows 3840 × 2160 :

| Mesure | Résultat |
| --- | --- |
| Analyse synthétique, huit zones, 3 600 positions | p50 0,359 ms |
| Même analyse, 500 itérations après échauffement | p95 0,489 ms ; p99 0,638 ms |
| Capture DXGI effective, 80 images fraîches en deux secondes | p50 8,631 ms |
| Ensemble des appels DXGI de ce test | p95 8,995 ms |

Le test de capture comporte volontairement une pause de 16 ms après chaque
appel : ses 80 images ne mesurent pas le plafond de cadence du moteur.
Ces tests ne mesurent ni le Zigbee ni la réponse optique des lampes.
Ils ne constituent pas une garantie sur un autre GPU ou sous charge de jeu.

## Vérification native

La version compilée démarre et détecte le pont local. L’écran d’accueil,
l’état avant connexion et les commandes désactivées ont été inspectés dans
la vraie fenêtre Tauri ; l’arbre d’accessibilité expose leurs libellés.

Le contrôle de fenêtre a ensuite été interrompu par l’utilisateur avec Échap.
Aucune autre interaction de fenêtre n’a été effectuée. Les états connectés,
le format étroit et le rendu optique des quatre lampes ne sont donc pas validés
par cette inspection. Les tests mathématiques ne remplacent pas cette mesure.

Voir [ALGORITHM.md](ALGORITHM.md) pour les formules, commandes reproductibles
et limites SDR/HDR, calibration et gamut.
