# Playwright Tests for Matrix Authentication Service

Ce projet contient des tests Playwright pour tester le scénario d'authentification OIDC avec Keycloak dans le service d'authentification Matrix (MAS).

## Prérequis

- Node.js 16 ou supérieur
- npm ou yarn
- Docker pour exécuter Keycloak et PostgreSQL
- Keycloak accessible sur `sso.tchapgouv.com`
- Matrix Authentication Service accessible sur `auth.tchapgouv.com`

### Démarrage des services

Avant d'exécuter les tests, vous devez démarrer les services Keycloak et Matrix Authentication Service :

```bash
# Démarrer Keycloak et PostgreSQL
./tchap/start-local-stack.sh

# Démarrer le service Matrix Authentication Service
./tchap/start-local-mas.sh
```

Ces scripts démarrent les services nécessaires pour les tests :
- PostgreSQL pour le stockage des données
- Keycloak avec le realm `proconnect-mock` préconfiguré
- Matrix Authentication Service configuré pour utiliser Keycloak comme fournisseur OIDC

Les entrees DNS doivent etre présentes dans le fichier hosts

## Installation

Pour initialiser rapidement le projet, utilisez le script d'initialisation :

```bash
# Rendre le script exécutable
chmod +x init.sh

# Exécuter le script d'initialisation
./init.sh
```

Ce script va :
- Créer le répertoire pour les résultats des tests
- Installer les dépendances npm
- Installer les navigateurs Playwright


Vous pouvez également effectuer ces étapes manuellement :

```bash
# Installer les dépendances
npm install

# Installer les navigateurs Playwright
npx playwright install
```

## Configuration

Les tests utilisent un fichier `.env` pour la configuration. Vous pouvez modifier ce fichier pour adapter les tests à votre environnement.

```
# URLs
MAS_URL=https://auth.tchapgouv.com
KEYCLOAK_URL=https://sso.tchapgouv.com

# Keycloak Admin Credentials
KEYCLOAK_ADMIN_USERNAME=admin
KEYCLOAK_ADMIN_PASSWORD=admin
KEYCLOAK_REALM=proconnect-mock
KEYCLOAK_CLIENT_ID=mas

# MAS Admin API Credentials
MAS_ADMIN_CLIENT_ID=01J44RKQYM4G3TNVANTMTDYTX6
MAS_ADMIN_CLIENT_SECRET=phoo8ahneir3ohY2eigh4xuu6Oodaewi

# Test User Credentials
TEST_USER_PREFIX=playwright_test_user
TEST_USER_PASSWORD=Test@123456
```

## Exécution des tests

```bash
# Exécuter tous les tests
npm test

# Exécuter les tests avec l'interface utilisateur visible
npm run test:headed

# Exécuter les tests en mode debug
npm run test:debug

# Exécuter les tests avec l'interface utilisateur de Playwright
npm run test:ui
```

## Captures d'écran

Les tests génèrent des captures d'écran à chaque étape importante du processus d'authentification. Ces captures sont enregistrées dans le répertoire `playwright-results/`.