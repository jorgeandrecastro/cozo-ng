# Publier Cozo-NG sur crates.io

Les noms de packages de cette reprise utilisent le préfixe `cozo-ng-` pour ne pas publier sous les noms de crates de l'amont. Le package principal reste `cozo-ng`, avec le nom de bibliothèque `cozo`, afin de préserver les imports Rust existants (`use cozo::*`). Les bindings gardent également leurs noms de bibliothèques et artefacts natifs lorsque nécessaire.

## Noms des packages

| Rôle | Package crates.io | Bibliothèque ou exécutable conservé |
|---|---|---|
| Moteur Rust | `cozo-ng` | bibliothèque `cozo` |
| Backend RocksDB | `cozo-ng-rocks` | bibliothèque `cozorocks` |
| Serveur autonome | `cozo-ng-bin` | exécutable `cozo-bin` |
| Binding C | `cozo-ng-c` | bibliothèque `cozo_c` |
| Binding Java/JNI | `cozo-ng-java` | bibliothèque `cozo_java` |
| Binding Python | `cozo-ng-python` | bibliothèque `cozo_embedded` |
| Binding Node.js | `cozo-ng-node` | bibliothèque native `cozo_node` |
| Binding Swift | `cozo-ng-swift` | bibliothèque `cozo_swift` |
| Binding WebAssembly | `cozo-ng-wasm` | bibliothèque `cozo_lib_wasm` |

Le nom du package et celui de sa bibliothèque Cargo sont distincts. Pour le moteur, par exemple, une dépendance peut garder l'alias `cozo` :

```toml
[dependencies]
cozo = { package = "cozo-ng", version = "0.10.0" }
```

## Avant la publication

1. Vérifier les versions, le changelog, les README et les fichiers inclus dans les paquets. Une version publiée sur crates.io ne peut pas être remplacée ni republiée avec le même numéro.
2. Vérifier que les contrôles passent :

   ```sh
   cargo check --workspace
   cargo test -p cozo-ng --lib
   git diff --check
   ```

   La compilation avec toutes les fonctionnalités peut demander des bibliothèques système supplémentaires, notamment `liburing` pour `io-uring`.

3. Inspecter le contenu de chaque paquet et faire un dry-run :

   ```sh
   cargo package -p cozo-ng-rocks --list
   cargo package -p cozo-ng --list
   cargo publish -p cozo-ng-rocks --dry-run
   ```

   Inspecter en particulier les sources RocksDB incluses par `cozo-ng-rocks`; le paquet est volumineux.

## Ordre de publication

Le moteur `cozo-ng 0.10.0` dépend optionnellement de `cozo-ng-rocks 0.3.1`. Cette dépendance doit néanmoins être disponible sur crates.io pour que le moteur puisse être publié et résolu depuis le registre.

### 1. Publier le backend RocksDB

```sh
cargo publish -p cozo-ng-rocks --dry-run
cargo publish -p cozo-ng-rocks
```

Attendre l'indexation de la version, puis vérifier sa disponibilité :

```sh
cargo info --registry crates-io cozo-ng-rocks@0.3.1
```

le nouveau nom est `cozo-ng-rocks`.

### 2. Publier le moteur

Une fois `cozo-ng-rocks` indexé :

```sh
cargo publish -p cozo-ng --dry-run
cargo publish -p cozo-ng
```

Le package est `cozo-ng`, mais son nom de bibliothèque reste `cozo`.

### 3. Publier les autres crates Rust

Ces packages dépendent du moteur `cozo-ng 0.10.0`. Leurs versions de crate sont indépendantes de celle du moteur; vérifier chaque manifeste avant publication. Publier et attendre l'indexation du moteur avant les bindings. Ne publier que les crates Rust prêtes pour cette version :

```sh
cargo publish -p cozo-ng-bin
cargo publish -p cozo-ng-c
cargo publish -p cozo-ng-java
cargo publish -p cozo-ng-python
cargo publish -p cozo-ng-node
cargo publish -p cozo-ng-swift
cargo publish -p cozo-ng-wasm
```

On peut d'abord remplacer chaque commande par `cargo publish -p <package> --dry-run`. `cozo-core-examples` est un paquet d'exemples interne au workspace et n'est pas proposé à la publication.

## Après publication

Vérifier la présence des versions sur crates.io et tester une résolution depuis le registre, pas seulement depuis le workspace local :

```sh
cargo info --registry crates-io cozo-ng@0.10.0
cargo info --registry crates-io cozo-ng-rocks@0.3.1
```

Mettre ensuite à jour les liens de release et annoncer les crates réellement publiées. La publication sur crates.io ne publie pas les paquets npm, Python/PyPI ni les artefacts Swift : ces distributions ont leurs propres procédures et noms.

## À retenir

- Ne pas utiliser `--allow-dirty` pour une publication réelle. Pour publier, committer les changements et vérifier le paquet exact.
- Un dry-run ne vérifie pas que le compte possède les droits sur un nom. Le 403 rencontré pour `cozorocks` était un refus de propriété, pas une erreur de compilation ou de packaging.
- Les noms préfixés doivent être vérifiés sur crates.io avant publication; `cargo info --registry crates-io <nom>` permet de vérifier qu'un nom publié est visible.
- Si crates.io indique qu'un nom est déjà pris, choisir un autre nom disponible avant de publier : un package publié ne peut pas être renommé.
