# Vérification 0.3.0 — 5 septembre 2026

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
