// Copyright 2024, 2025 New Vector Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE files in the repository root for full details.

import type { KnipConfig } from "knip";

export default {
<<<<<<< HEAD
  entry: ["src/entrypoints/**", "src/routes/*"],
  ignore: [
    "src/gql/*",
    "src/routeTree.gen.ts",
    ".storybook/locales.ts",

    "tchap/**", //:tchap: add tchap folder
    "i18next.config.ts",
  ],
  ignoreDependencies: [
    // This is used by the tailwind PostCSS plugin, but not detected by knip
    "postcss-nesting",
  ],
=======
  entry: ["src/entrypoints/*", "src/routes/*"],
  ignore: ["src/gql/*", ".storybook/locales.ts", "i18next.config.ts"],
>>>>>>> v1.22.0
} satisfies KnipConfig;
