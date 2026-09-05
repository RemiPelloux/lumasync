# Moteur couleur 0.3

## Corrections vérifiables

- L’ancien filtre comparait la cible à la cible précédente et retournait une sortie
  encore intermédiaire si la cible cessait de bouger. Une scène stable pouvait ainsi
  rester sur la mauvaise couleur. Le nouveau filtre converge à chaque envoi.
- L’ancien recorder xcap Windows utilisait un rendez-vous bloquant entre producteur
  et consommateur. Le producteur pouvait conserver une image pendant le sommeil
  du consommateur. Le chemin Windows natif interroge maintenant DXGI directement,
  avec un délai d’acquisition nul, sans thread producteur ni file d’images.
- Les petits rectangles plafonnés des angles sont remplacés par des cônes larges
  regardant vers le centre. Une couleur intérieure contribue sans toucher le coin.
- Les poids spatiaux ne changent plus en fonction de la charge CPU : le motif reste
  fixe, ce qui évite une source de variation artificielle.

## Acquisition et cadence

Le moteur sélectionne l’adaptateur GPU correspondant au moniteur, réutilise une
texture de staging et un tampon CPU BGRA, respecte le pas mémoire des lignes et
libère chaque image DXGI, même après une erreur de copie. Il ne convertit pas
tous les pixels en RGBA : seuls les échantillons utiles sont décodés.

Un délai DXGI nul signifie « vérifier immédiatement », pas « bloquer jusqu’à une
image ». Voir la [documentation Microsoft](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgioutputduplication-acquirenextframe).

Si le bureau ne change pas, la dernière cible est réutilisée, mais le filtre
continue de converger et le flux Hue continue. En cas de surcharge, aucune rafale
ne rejoue les images manquées. Après une reconnexion, l’image est recapturée.

Un échec DXGI déclenche la capture de secours xcap et une tentative de reprise
native après deux secondes. Les écrans pivotés utilisent aussi ce secours.
Ce chemin de secours peut être plus lent. Les bureaux sécurisés ou les moniteurs
déconnectés peuvent rendre toute capture impossible et arrêtent alors le flux.

## Cônes spatiaux

Une grille fixe de 80 × 45 positions couvre le cadre utile : au maximum
3 600 échantillons, quel que soit le nombre de lampes et la résolution.

Pour une position normalisée p, une ancre a (coin ou milieu d’un côté), et le
vecteur unitaire d orienté vers le centre :

- t = produit scalaire(p − a, d), profondeur vers l’intérieur ;
- q = projection perpendiculaire(p − a, d) ;
- R = 0,68 + portée / 60, avec portée dans [5, 30] ;
- sigma = 0,27 pour les coins ou 0,42 pour les côtés, augmenté de 0,35 t ;
- poids = exp(−0,5 (q / sigma)²) × (1 − t / R)² pour 0 ≤ t < R, sinon 0.

Les cônes se recouvrent volontairement. Leur centre reste visible même à faible
portée ; le voisinage de la lampe a plus d’influence que les pixels éloignés.
La portée est un indice, pas un pourcentage de largeur. Les poids sont précalculés
et reconstruits seulement lorsque le cadre utile change.

Les bandes noires sont recherchées au plus toutes les 50 ms sur les images fraîches.
La décision utilise luminance, variance, chroma et percentile 95, avec confirmation
temporelle (trois détections pour recadrer, douze pour retrouver le cadre entier).
Une scène entièrement noire ne devient pas un rectangle vide.

## Spectre perceptuel et couleur

1. Décodage sRGB exact vers RGB linéaire via une table de 256 valeurs.
2. Conversion en [Oklab, défini par Björn Ottosson](https://bottosson.github.io/posts/oklab/).
3. Accumulation unique par pixel : moyenne linéaire, moments Oklab et histogramme
   circulaire de 36 classes de teinte avec interpolation entre classes.
4. Estimation de la variance perceptuelle par E[Lab²] − E[Lab]².
5. Mélange continu entre moyenne et teinte dominante selon la variance, la part
   chromatique et la séparation entre les deux pics. Des masses égales de teintes
   éloignées ne sélectionnent pas arbitrairement un gagnant.
6. Atténuation progressive des petites zones blanches dans les scènes colorées.
   Une image blanche reste blanche. C’est une heuristique, pas une reconnaissance
   sémantique des sous-titres : un petit objet blanc peut aussi être atténué.
7. Saturation et transitions en Oklab, puis réduction de chroma à teinte et
   luminosité perceptuelle constantes pour rentrer dans le gamut sRGB.
8. Encodage sRGB et application des limites de luminosité avant la trame Hue.

Les conversions sont effectuées une fois par position, les zones une fois par image
et les lampes regroupées dans une seule trame. Les noms Hue sont obtenus par
collections en parallèle, sans requête par lampe ; le client HTTP partage son pool.

## Transitions et anticipation

Le filtre est inspiré du principe de coupure adaptative du
[One Euro Filter de Casiez et al.](https://gery.casiez.net/publications/CHI2012-casiez.pdf),
avec une discrétisation exponentielle liée au temps réel :

alpha = 1 − exp(−2 pi × coupure × dt)

La coupure augmente avec la réactivité et la vitesse observée de la couleur.
Les petites variations n’accélèrent pas le filtre. Une coupure de scène importante
ou une longue pause réinitialise la vitesse et suit immédiatement la cible.

L’extrapolation ne porte que sur les couleurs des zones, **pas sur des pixels futurs**.
Elle s’active lorsque les dérivées successives sont cohérentes, sur au plus 12 ms,
avec une distance Oklab maximale de 0,015. Elle s’arrête quand la cible se stabilise
ou change de direction. Ce n’est ni un modèle d’optical flow ni une prédiction
garantie ; les coupes futures ne peuvent pas être devinées.

## Mesures et validation

La télémétrie « Cadence » représente les envois Hue, pas la fréquence de nouvelles
images du bureau. « Traitement » mesure capture, analyse, filtrage et envoi local,
sans inclure le sommeil entre deux envois ni le délai radio/optique des lampes.

Tests reproductibles :

```powershell
npm run check:size
npm run type-check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build
cargo test --release --manifest-path src-tauri/Cargo.toml --lib -- --ignored --nocapture
```

La dernière commande lance le benchmark synthétique et lit le bureau pendant
deux secondes, sans sauvegarder d’image ni envoyer de couleurs au pont.
Le benchmark exclut l’acquisition ; le test DXGI distingue les appels sans nouvelle
image des copies effectives. Un bureau immobile ne constitue pas un test de vidéo 60 Hz.

## Limites physiques et colorimétriques

Ce moteur cible un bureau SDR sRGB. Il n’effectue pas de calibration ICC,
de tone mapping HDR/scRGB, ni de calibration optique par modèle de lampe.
Le gamut Hue, le blanc des ampoules, le mur, le Zigbee, le réseau et le rafraîchissement
de l’écran influencent le résultat. Une fidélité « parfaite » et une latence
bout-en-bout chiffrée nécessitent des mesures matérielles, absentes de ces tests.

L’échantillonnage fixe peut manquer des détails très fins. Les cônes mélangent
intentionnellement des régions voisines ; une couleur unique ne peut reproduire
tous les pixels d’une région multicolore. Les contenus DRM peuvent être noirs.

## Organisation

Chaque fichier source, configuration et document rédigé reste sous 350 lignes.
La vérification automatique accepte au maximum 349 lignes, en local et en CI.
Les fichiers de verrouillage générés Cargo/npm sont exclus : les découper ou les
compresser artificiellement nuirait à la reproductibilité et à la lisibilité.
