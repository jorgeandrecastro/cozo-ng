# Publier Cozo-NG sur crates.io

Ce guide décrit l'ordre de publication Rust pour la version `0.8.0`. La crate principale s'appelle `cozo-ng`, mais son nom de bibliothèque reste `cozo` : le code Rust des utilisateurs peut donc continuer à faire `use cozo::*`.

## Avant la publication

1. Vérifier que les versions, le changelog, les README et les fichiers inclus dans les paquets sont définitifs. Une version publiée sur crates.io ne peut pas être remplacée ni republiée avec le même numéro.
2. Vérifier que les contrôles passent :

   ```sh
   cargo check --workspace
   cargo test -p cozo-ng --lib
   git diff --check
   ```

   La compilation avec toutes les fonctionnalités peut demander des bibliothèques système supplémentaires, notamment `liburing` pour `io-uring`.
3. Créer un compte crates.io, vérifier l'adresse e-mail, puis créer un jeton API sur <https://crates.io/settings/tokens>. Sur la machine qui publie, se connecter avec :

   ```sh
   cargo login
   ```

   Cargo demande alors le jeton. Ne le mettez jamais dans le dépôt, un fichier de configuration versionné ou une commande enregistrée dans l'historique du shell.
4. Vérifier les paquets et leur contenu avant d'envoyer quoi que ce soit :

   ```sh
   cargo package -p cozorocks --list
   cargo package -p cozo-ng --list
   cargo publish -p cozorocks --dry-run
   ```

   Le dry-run de `cozorocks 0.2.0` a déjà réussi dans l'environnement de développement. Il incluait environ 24 Mo de fichiers avant compression : inspecter la liste, en particulier les sources RocksDB, avant la vraie publication.

## Ordre de publication

La dépendance `cozorocks` doit être publiée en premier. Même si elle est optionnelle dans le moteur, `cozo-ng 0.8.0` référence `cozorocks ^0.2.0`, et crates.io doit pouvoir résoudre cette version au moment de publier le moteur.

### 1. Publier `cozorocks`

```sh
cargo publish -p cozorocks --allow-dirty

```

Attendre que crates.io ait indexé la version. Vérifier ensuite qu'elle est visible :

```sh
cargo info cozorocks@0.2.0
```

### 2. Vérifier puis publier le moteur

```sh
cargo publish -p cozo-ng --dry-run
cargo publish -p cozo-ng
```

Le paquet publié s'appelle `cozo-ng`; le nom de bibliothèque `cozo` est conservé dans son manifeste.

### 3. Publier les autres crates Rust, si elles font partie de la livraison

Ces paquets dépendent de `cozo-ng = 0.8.0`. Attendre que le moteur soit indexé avant de les publier. Les publier un par un permet de repérer précisément une erreur :

```sh
cargo publish -p cozo-bin
cargo publish -p cozo_c
cargo publish -p cozo_java
cargo publish -p cozo_py
cargo publish -p cozo-node
cargo publish -p cozo-swift
cargo publish -p cozo-lib-wasm
```

Ils peuvent être testés avant en remplaçant chaque commande par `cargo publish -p <nom> --dry-run`. Ne publiez que les bindings effectivement prêts et maintenus pour cette version. `cozo-core-examples` est un paquet d'exemples interne au workspace et n'est pas inclus dans la liste de publication.

## Après publication

Vérifier la présence des versions sur crates.io et tester une résolution depuis le registre, pas seulement depuis le workspace local. Par exemple :

```sh
cargo info cozo-ng@0.8.0
cargo info cozorocks@0.2.0
```

Mettre ensuite à jour les liens de release et annoncer les crates réellement publiées. La publication sur crates.io ne publie pas les paquets npm, Python/PyPI ni les artefacts Swift : ces distributions ont leurs propres procédures et doivent être traitées séparément.

## À retenir

- Ne pas utiliser `--allow-dirty` pour la publication réelle. Cette option ne convient qu'à un dry-run local quand l'arbre de travail contient des changements non commités.
- Ne pas lancer les publications avant d'avoir figé et relu le contenu des paquets : les versions envoyées sur crates.io sont immuables.
- Si un dry-run du moteur échoue avec « no matching package named `cozorocks` », vérifier que `cozorocks 0.2.0` est publié et attendre l'indexation crates.io avant de réessayer.
