---
version: alpha
name: "LumaSync"
description: "Un pupitre nocturne précis qui transforme les couleurs de l’écran en lumière Hue sans distraire du film ou du jeu."
colors:
  canvas: "#101214"
  surface: "#191C1F"
  surface-raised: "#25292D"
  border: "#393F45"
  text: "#F3F5F6"
  text-muted: "#A8B0B8"
  primary: "#72C9BA"
  primary-strong: "#A5E4D7"
  success: "#66E3B4"
  warning: "#FFCB77"
  danger: "#FF7D91"
  focus: "#8EDBFF"
typography:
  display:
    fontFamily: "Bahnschrift, 'Segoe UI Variable Display', 'Segoe UI', sans-serif"
  body:
    fontFamily: "'Segoe UI Variable Text', 'Segoe UI', sans-serif"
  utility:
    fontFamily: "'Cascadia Code', Consolas, monospace"
rounded:
  DEFAULT: "0.875rem"
  sm: "0.625rem"
  md: "0.875rem"
  lg: "1.375rem"
  pill: "999px"
spacing:
  control: "0.75rem"
  panel: "1.25rem"
  section-gap: "1.5rem"
  page-max: "78rem"
components:
  button: {}
  card: {}
  range: {}
  profile-selector: {}
  telemetry-strip: {}
  status: {}
---

# LumaSync Design System

## Overview

### Creative North Star

L’interface est un pupitre de régie lumière : surfaces charbon neutres, commandes vert menthe, repères nets. Les couleurs vives représentent les sorties Hue et les états d’erreur.

### Product context and register

- **Audience and primary job:** une personne devant son PC Windows qui veut associer son Hue Bridge, placer quatre lumières et démarrer l’Ambilight en quelques secondes.
- **Target market(s) and evidence:** usage personnel francophone ; le brief et la langue de l’utilisateur constituent l’unique contexte produit disponible.
- **Locale(s) and language policy:** français uniquement pour cette première version, avec libellés courts et erreurs orientées vers la prochaine action.
- **Usage scene:** grand écran dans une pièce sombre, souris ou clavier, réglages occasionnels puis contrôle marche/arrêt fréquent.
- **Register:** produit utilitaire.
- **Memorable signature:** un cadre-écran central dont les quatre bords diffusent les vraies couleurs échantillonnées.
- **Instrumentation signature:** une réglette compacte au-dessus de l’écran expose cadence, temps de traitement, cadre utile et stabilité comme sur un moniteur de régie ; elle reste secondaire face au halo.
- **Restraint:** réglages, états réseau et association restent calmes, sans gradients décoratifs ni animations permanentes.
- **Anti-references:** pas de néon « gamer » omniprésent, pas de cartes vitrées génériques, pas de fausse visualisation audio.
- **Token ownership/runtime mapping:** ce fichier est la source durable ; les variables CSS sont définies dans `src/styles/tokens.css`. `src/styles.css` importe les feuilles spécialisées dans l’ordre historique, sans changer la cascade.

## Colors

`canvas` et `surface` installent l’obscurité sans employer du noir pur. `primary` identifie l’action sûre, `success`, `warning` et `danger` gardent leur sens. Les teintes du contenu vidéo ne remplacent jamais les couleurs sémantiques : elles restent confinées au cadre de prévisualisation.

## Typography

Bahnschrift donne aux titres une largeur technique propre aux instruments Windows. Segoe UI Variable porte tous les textes d’usage. Cascadia Code est réservée aux fréquences, adresses réseau et valeurs mesurées. Les titres restent en casse de phrase.

## Layout

À partir de 980 px, la fenêtre se partage entre un panneau de contrôle dense (~318 px) et une scène qui remplit la hauteur utile. La densité reste haute : titres courts, paddings serrés, cartes compactes. Entre 720 et 980 px, l’aperçu passe au-dessus et les cartes s’organisent en deux colonnes. Sous 720 px, tout s’empile. La fenêtre Tauri démarre à 1040×680.

## Elevation & Depth

La profondeur vient des différences de tons, d’une bordure froide et d’une ombre diffuse seulement sur la scène-écran. Les cartes de réglage restent plates. Aucun flou translucide n’est utilisé derrière le texte.

## Shapes

Les sections sont séparées par des lignes, sans cartes flottantes. Les contrôles et la fenêtre de diagnostic utilisent au maximum 8 px de rayon. Les boutons segmentés et états courts peuvent être en pilule. L’écran garde une géométrie 16:9 stable.

## Components

### Foundational visual states

Chaque commande possède repos, survol, focus visible, actif, désactivé et occupé. Le chargement réserve sa place. Le focus `focus` est renforcé par un contour et non par la couleur seule.

### Buttons and actions

Le bouton principal plein sert uniquement à démarrer ou associer. Les actions secondaires sont bordées ; l’arrêt actif emploie `danger` avec une icône et un verbe explicite. Les libellés ne changent pas de largeur pendant une opération.

### Navigation and data display

L’application tient sur une vue. Les étapes sont indiquées par un rail compact et un statut textuel ; il n’y a ni onglet ni fil d’Ariane artificiel.

Les mesures en direct utilisent une réglette stable à quatre cellules. Elles occupent la même géométrie à l’arrêt et en diffusion, emploient la police utilitaire pour les valeurs, et ne deviennent jamais un graphique décoratif.

### Forms and overlays

Les réglages sont des curseurs natifs, augmentés de leur valeur et de boutons de choix sémantiques. Luminosité, saturation et réactivité sont immédiatement accessibles ; plafond lumineux et portée restent dans « Réglages avancés ». Le placement des lampes se déplie dans « Position des lumières ». Les erreurs du moteur sont visibles au-dessus du contenu, avec accès au journal. Les erreurs de lecture du statut proposent une relance indépendante du flux.

Trois profils nommés — Cinéma, Jeu et Naturel — règlent ensemble la cadence, le lissage, la saturation et la profondeur d’analyse. Toute retouche individuelle produit implicitement un réglage personnalisé sans ajouter un quatrième bouton inactif. La détection des bandes noires est un interrupteur explicite et reste activée par défaut.

Deux dispositions d’échantillonnage coexistent : **Bords** (gauche, droite, haut, bas) et **Angles** (haut gauche, haut droit, bas gauche, bas droit). Le sélecteur reprend le même contrôle segmenté que la cadence. En mode Angles, le sélecteur de chaque lumière devient une grille 2×2 qui mime les coins de l’écran ; l’aperçu déplace marqueurs et halos aux quatre coins. Le mode choisi est mémorisé et le passage de l’un à l’autre réaffecte automatiquement les canaux.

Quand aucune zone Entertainment n’existe, la carte Lumières présente les pièces Hue détectées et une action explicite « Créer depuis … ». Cette action crée un groupe Entertainment séparé avec les lampes de la pièce, sans modifier l’appartenance ni les automatismes de la pièce d’origine.

### Iconography

Lucide, trait 1,8 px, 18–20 px. Une icône ne remplace un libellé que pour un contrôle universel doté d’un nom accessible.

### Motion

Les changements de halo se lissent côté Rust en Oklab avec une coupure adaptative et une extrapolation plafonnée à 12 ms. La capture DXGI n’accumule pas d’images ; une grille fixe alimente toutes les zones. Les cônes regardent vers l’intérieur depuis les coins ou les côtés. « Portée du cône » est un indice de 5 à 30, pas un pourcentage de pixels. La cadence affichée compte les envois Hue, même sur un bureau immobile. L’interface conserve ses transitions de 180 ms pour les contrôles et de 100 ms pour les couleurs. Sur fenêtre étroite, l’aperçu passe au-dessus des réglages et le flou d’ambiance est réduit. `prefers-reduced-motion` supprime les déplacements, les pulsations et le flou.

### Content and data visualization

Le ton est direct : « Détecter », « Associer », « Démarrer l’éclairage ». L’aperçu montre les couleurs envoyées par zone ; il ne représente pas les pixels capturés. Le journal s’ouvre dans un dialogue clavier avec fermeture par Échap, filtre des problèmes, actualisation et export JSONL. La typographie garde un espacement des lettres nul et des tailles indépendantes de la largeur de fenêtre.

## Do's and Don'ts

- **Do:** laisser la lumière capturée être le seul élément très coloré.
- **Do:** conserver les actions réseau et leurs erreurs dans le même panneau.
- **Don't:** multiplier les halos, dégradés ou reflets sans rapport avec l’écran.
- **Don't:** masquer un réglage ou un état important derrière un survol.
