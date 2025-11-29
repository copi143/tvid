# tvid

`tvid` est un lecteur vidéo en terminal écrit en Rust. Il utilise FFmpeg pour le décodage et rend la vidéo, l’audio et les sous-titres directement dans votre terminal, avec une interface en surimpression, une vue de playlist et des interactions basiques clavier / souris.

---

*Traductions :*<br />
[en-us/English](../README.md) | [zh-cn/简体中文](README.zh-cn.md)

*Autres langues (traduites par ChatGPT) :*<br />
[ja-jp/日本語](doc/README.ja-jp.md) · **fr-fr/Français** · [de-de/Deutsch](doc/README.de-de.md) · [es-es/Español](doc/README.es-es.md)

---

> Ce projet est en cours de développement actif. Le comportement et l’interface peuvent changer.

## Fonctionnalités

- **Lecture de presque tous les formats** pris en charge par FFmpeg
- **Interface en surimpression dans le terminal** : barre de progression, messages et aide à l’écran
- **Support de playlist** :
  - passer plusieurs fichiers en ligne de commande
  - navigation dans la playlist en mémoire (suivant / précédent, boucle)
  - panneau latéral de playlist optionnel
- **Contrôle souris & clavier** pour le déplacement dans la vidéo et la navigation
- **Fichier de configuration & playlist par défaut** sous `~/.config/tvid/`
- Utilise **Unifont** pour une meilleure couverture de glyphes dans l’UI

## Prérequis

- Une toolchain Rust récente (nightly **non requise**)
  - sur Debian / Ubuntu : `sudo apt install cargo` ou `sudo apt install rustup && rustup install stable`
  - sur Arch : `sudo pacman -S rust` ou `sudo pacman -S rustup && rustup install stable`
- Les bibliothèques FFmpeg et en-têtes de développement disponibles sur votre système
  - sur Debian / Ubuntu : `sudo apt install ffmpeg libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev`
  - sur Arch : `sudo pacman -S ffmpeg`

## Construire & exécuter

1. Cloner le dépôt :

   ```sh
   git clone https://github.com/copi143/tvid.git
   cd tvid
   ```

2. Construire le projet :

   ```sh
   cargo build --release
   ```

3. Lancer le lecteur :

   ```sh
   cargo run -- <entrée1> [entrée2] [...]
   # ou, après compilation
   target/release/tvid <entrée1> [entrée2] [...]
   ```

Chaque entrée devient un élément de la playlist en mémoire.

## Utilisation

```sh
tvid <entrée1> [entrée2] [...]
```

### Fichiers de configuration & playlist

Au premier lancement, `tvid` crée un répertoire de config et deux fichiers :

- Répertoire de config : `~/.config/tvid/`
- Fichier de configuration : `tvid.toml`
  - exemples de clés :
    - `volume` (`0`–`200`) : volume initial
    - `looping` (`true` / `false`) : boucle de la playlist ou non
- Fichier de playlist : `playlist.txt`
  - chaque ligne est interprétée comme un chemin de fichier
  - les lignes vides et les lignes commençant par `#` sont ignorées

Au démarrage, `tvid` charge d’abord la playlist depuis `playlist.txt`, puis ajoute les fichiers passés en ligne de commande.

### Raccourcis clavier & souris

Contrôles de lecture principaux (globaux) :

- `Space` – lecture / pause
- `q` – quitter le lecteur
- Flèches – déplacement dans la vidéo
  - `←` – reculer de 5 secondes
  - `→` – avancer de 5 secondes
  - `↑` – reculer de 30 secondes
  - `↓` – avancer de 30 secondes

Contrôles de playlist :

- `n` – lire l’élément suivant dans la playlist
- `l` – (dés)afficher le panneau latéral de playlist
- Dans le panneau de playlist :
  - `w` / `↑` – déplacer la sélection vers le haut
  - `s` / `↓` – déplacer la sélection vers le bas
  - `Space` / `Enter` – lire l’élément sélectionné
  - `q` – fermer le panneau de playlist

Interface & autres :

- `f` – ouvrir le sélecteur de fichiers (panneau UI)
- `c` – changer le mode de couleur
- Barre de progression :
  - clic gauche près de la zone de progression en bas pour se déplacer
  - faire glisser avec le bouton gauche pour naviguer dans la vidéo

> Remarque : des raccourcis supplémentaires et des éléments d’interface peuvent être ajoutés au fil de l’évolution du projet.

## Dépannage

- Erreurs de compilation :
  - Assurez-vous que FFmpeg et ses en-têtes de développement sont installés sur votre système.
- Erreur de chargement des bibliothèques partagées (à l’exécution) :
  - Assurez-vous d’avoir compilé et exécuté le programme sur la même machine — d’autres machines peuvent avoir des versions de FFmpeg différentes.
  - Vérifiez que les bibliothèques FFmpeg d’exécution peuvent être trouvées (par exemple, vérifiez qu’un autre programme utilisant FFmpeg comme `vlc` fonctionne correctement).
- Au démarrage : `av init failed` :
  - Vérifiez que FFmpeg fonctionne correctement sur votre système.
- Après le démarrage : `No input files.` :
  - Assurez-vous que :
    - vous avez passé au moins un fichier vidéo/audio lisible en ligne de commande, ou
    - `~/.config/tvid/playlist.txt` contient des chemins valides et accessibles.

## Licence

Reportez-vous à la section License du `README.md` à la racine du dépôt (en anglais) pour les détails de la licence.
