# Tchap Modifications — Matrix Authentication Service Fork

This file documents all Tchap-specific modifications to the upstream
`element-hq/matrix-authentication-service` project.

- **Upstream**: https://github.com/element-hq/matrix-authentication-service
- **Fork**: https://github.com/tchapgouv/matrix-authentication-service
- **Branch**: `main_tchap`
- **tag**: v1.24.0-1.22.0

## How Tchap modifications are tagged

Every Tchap-specific code change in upstream files is enclosed between
`:tchap:` and `:tchap: end` (or `:tchap:end`) comment tags, matched
case-insensitively (`:TCHAP:` is also used). These tags must **never** be
removed during upstream merges. Files that exist only in the Tchap fork
(e.g. `crates/tchap/**`, `tchap/resources/**`, `frontend/tchap/**`) do not
need tags.

## Feature map

| # | Feature | Main files |
|---|---|---|
| 1 | Tchap core library | `crates/tchap/src/lib.rs`, `crates/tchap/src/identity_client.rs`, `crates/tchap/src/test_utils.rs` |
| 2 | Tchap configuration | `crates/config/src/sections/mod.rs`, `crates/cli/src/app_state.rs`, `crates/cli/src/commands/server.rs`, `crates/data-model/src/lib.rs` |
| 3 | Email-based registration | `crates/handlers/src/views/register/password.rs`, `crates/handlers/src/views/register/steps/finish.rs`, `frontend/src/entrypoints/register/PasswordCreationDoubleInput.tsx`, `tchap/resources/templates/pages/register/**` |
| 4 | Email gating on login and recovery | `crates/handlers/src/views/login.rs`, `crates/handlers/src/views/recovery/start.rs` |
| 5 | SSO account reactivation | `crates/handlers/src/upstream_oauth2/link.rs` (reactivation logic) |
| 6 | SSO linking by email | `crates/handlers/src/upstream_oauth2/link.rs` (email matching), `crates/handlers/src/upstream_oauth2/template.rs`, `tchap/resources/templates/pages/sso.html`, `tchap/resources/templates/pages/upstream_oauth2/do_register.html` |
| 7 | Kill sessions admin API | `crates/handlers/src/admin/v1/users/kill_sessions.rs`, `crates/handlers/src/admin/v1/users/mod.rs`, `crates/handlers/src/admin/v1/mod.rs` |
| 8 | Consent page email and index link | `crates/handlers/src/oauth2/authorization/consent.rs`, `crates/handlers/src/views/index.rs`, `crates/templates/src/context.rs`, `tchap/resources/templates/pages/consent.html` |
| 9 | Branding (templates) | `tchap/resources/templates/base.html`, `tchap/resources/templates/app.html`, `tchap/resources/templates/pages/**` |
| 10 | Frontend customizations | `frontend/index.html`, `frontend/src/components/Layout/Layout.tsx`, `frontend/src/routes/_account.index.tsx`, `frontend/src/routes/password.recovery.index.tsx`, `frontend/src/routes/reset-cross-signing.tsx`, `frontend/src/routes/reset-cross-signing.index.tsx`, `frontend/src/utils/password_complexity/index.ts`, `frontend/knip.config.ts`, `frontend/tests/routes/reset-cross-signing.test.tsx` |
| 11 | Cookies | `crates/axum-utils/src/cookies.rs` |
| 12 | Build and CI | `.github/workflows/build.yaml`, `.github/workflows/ci.yaml`, `Dockerfile` |
| 13 | Handler wiring and test state | `crates/handlers/src/lib.rs`, `crates/handlers/src/test_utils.rs` |
| 14 | GraphQL user mutations | `crates/handlers/src/graphql/mutations/user.rs` |
| 15 | OIDC end session (RP-initiated logout) | `crates/handlers/src/oauth2/end_session.rs`, `crates/handlers/src/oauth2/discovery.rs`, `crates/router/src/endpoints.rs` |
| 16 | Client registration policy for Tchap Desktop | `policies/client_registration/client_registration.rego` |
| 17 | Password visibility toggle on login page | `templates/components/password_field.html`, `templates/pages/login.html` |

## Detail sections

### 1. Tchap core library

A dedicated crate containing Tchap-specific business logic: email-to-MXID
conversion, email-to-display-name generation, email server validation (checks
whether an email is allowed on the current server by querying the identity
server), user search by email with configurable fallback rules, and the
identity server HTTP client. Test helpers provide a default configuration for
unit tests.

Files:
- [crates/tchap/src/lib.rs](../crates/tchap/src/lib.rs) (email conversion, server validation, user search)
- [crates/tchap/src/identity_client.rs](../crates/tchap/src/identity_client.rs) (identity server HTTP client)
- [crates/tchap/src/test_utils.rs](../crates/tchap/src/test_utils.rs) (test configuration)
- [Cargo.toml](../Cargo.toml) (tchap crate registration, dependency pins)
- [crates/cli/Cargo.toml](../crates/cli/Cargo.toml) (icu_experimental dependency)

### 2. Tchap configuration

Adds a Tchap-specific configuration section to the MAS configuration system.
The runtime `TchapConfig` type holds the identity server URL, email lookup
fallback rules, and a link to the Tchap web app. It is injected into the
application state and made available as an axum extractor for handlers.

Files:
- [crates/config/src/sections/mod.rs](../crates/config/src/sections/mod.rs) (tchap config submodule registration)
- [crates/config/src/sections/tchap.rs](../crates/config/src/sections/tchap.rs) (config section definition)
- [crates/data-model/src/lib.rs](../crates/data-model/src/lib.rs) (TchapConfig type and re-exports)
- [crates/data-model/src/tchap_config.rs](../crates/data-model/src/tchap_config.rs) (runtime TchapConfig type)
- [crates/cli/src/app_state.rs](../crates/cli/src/app_state.rs) (AppState field + FromRef impl)
- [crates/cli/src/commands/server.rs](../crates/cli/src/commands/server.rs) (config extraction + injection)

### 3. Email-based registration

Replaces the upstream username-based registration with an email-based flow.
The email is pre-filled from the OAuth2 login hint, validated against the
identity server (wrong server, invitation required), and used to generate
both the MXID localpart and the display name automatically. The username
existence check is moved earlier and an "email in use" error page is shown
when a collision is detected, including a specific message for deactivated
accounts. The frontend uses a custom password creation component with
Tchap-specific labels.

Files:
- [crates/handlers/src/views/register/password.rs](../crates/handlers/src/views/register/password.rs) (email validation, MXID/display-name generation)
- [crates/handlers/src/views/register/steps/finish.rs](../crates/handlers/src/views/register/steps/finish.rs) (email-in-use check, deactivated account flag)
- [frontend/src/entrypoints/register/PasswordCreationDoubleInput.tsx](../frontend/src/entrypoints/register/PasswordCreationDoubleInput.tsx) (custom password input component)
- [frontend/src/entrypoints/register/PasswordDoubleInput.tsx](../frontend/src/entrypoints/register/PasswordDoubleInput.tsx) (register password input entrypoint)
- [tchap/resources/templates/pages/register/index.html](../tchap/resources/templates/pages/register/index.html) (registration entry template)
- [tchap/resources/templates/pages/register/password.html](../tchap/resources/templates/pages/register/password.html) (password registration template)
- [tchap/resources/templates/pages/register/steps/display_name.html](../tchap/resources/templates/pages/register/steps/display_name.html) (display name step template)
- [tchap/resources/templates/pages/register/steps/email_in_use.html](../tchap/resources/templates/pages/register/steps/email_in_use.html) (email-in-use error template)

### 4. Email gating on login and recovery

Blocks login and password recovery attempts when the user's email is mapped
to a different Matrix server. Before processing the login form or recovery
form, the handler queries the identity server to check whether the email
belongs to the current server. If not, a French error message is displayed
without revealing whether the account exists.

Files:
- [crates/handlers/src/views/login.rs](../crates/handlers/src/views/login.rs) (pre-login email server check)
- [crates/handlers/src/views/recovery/start.rs](../crates/handlers/src/views/recovery/start.rs) (pre-recovery email server check)

### 5. SSO account reactivation

When a user attempts to log in via SSO (ProConnect) and an existing account
is found in a deactivated state, the flow reactivates the account on both the
homeserver and the database instead of showing a "deactivated" error page.
The stored email is preserved rather than being overwritten by the OIDC
email.

Files:
- [crates/handlers/src/upstream_oauth2/link.rs](../crates/handlers/src/upstream_oauth2/link.rs) (reactivation logic, lines ~802-843)

### 6. SSO linking by email

Replaces the upstream username-based SSO account linking with email-based
matching. When a ProConnect user logs in, the handler searches for an
existing Tchap account by email (with configurable fallback rules, e.g.
`@numerique.gouv.fr` -> `@beta.gouv.fr`). If the ProConnect email differs
from the stored Tchap email, an error page is shown to prevent account
hijacking. New accounts are only created after validating the email is
allowed on the current server. Custom minijinja template filters are
registered for email-to-display-name and email-to-MXID conversion.

Files:
- [crates/handlers/src/upstream_oauth2/link.rs](../crates/handlers/src/upstream_oauth2/link.rs) (email matching, server validation, fallback rules)
- [crates/handlers/src/upstream_oauth2/template.rs](../crates/handlers/src/upstream_oauth2/template.rs) (minijinja template filters)
- [tchap/resources/templates/pages/sso.html](../tchap/resources/templates/pages/sso.html) (SSO page template)
- [tchap/resources/templates/pages/upstream_oauth2/do_register.html](../tchap/resources/templates/pages/upstream_oauth2/do_register.html) (upstream OAuth registration template)

### 7. Kill sessions admin API

A Tchap-only admin API endpoint that forcibly terminates all OAuth2 and
compat sessions for a given user. This is used by Tchap operators to
disconnect a user from all devices.

Files:
- [crates/handlers/src/admin/v1/users/kill_sessions.rs](../crates/handlers/src/admin/v1/users/kill_sessions.rs) (endpoint handler + tests)
- [crates/handlers/src/admin/v1/users/mod.rs](../crates/handlers/src/admin/v1/users/mod.rs) (module registration)
- [crates/handlers/src/admin/v1/mod.rs](../crates/handlers/src/admin/v1/mod.rs) (route registration)

### 8. Consent page email and index link

Displays the user's email address on the OAuth2 consent page so the user
can verify which account they are consenting with. The index page is
customized to render a link to the Tchap web application.

Files:
- [crates/handlers/src/oauth2/authorization/consent.rs](../crates/handlers/src/oauth2/authorization/consent.rs) (fetch user email, pass to consent context)
- [crates/handlers/src/views/index.rs](../crates/handlers/src/views/index.rs) (pass Tchap app link to index context)
- [crates/templates/src/context.rs](../crates/templates/src/context.rs) (ConsentContext.email, IndexContext.tchap_app_link, RegisterStepsEmailInUseContext.is_deactivated)
- [tchap/resources/templates/pages/consent.html](../tchap/resources/templates/pages/consent.html) (consent page template)
- [tchap/resources/templates/pages/index.html](../tchap/resources/templates/pages/index.html) (index page with Tchap app links)

### 9. Branding (templates)

Tchap-specific Jinja2 templates that override or extend the upstream web UI
with Tchap branding (header, footer, colors, layout). The base template
includes the La Suite header and footer. Email templates are also customized
for Tchap's visual identity. Only the French and English translations are
kept: all other upstream locales are deleted and must be deleted again after
each upstream merge.

Files:
- [tchap/resources/templates/base.html](../tchap/resources/templates/base.html) (La Suite header/footer)
- [tchap/resources/templates/tchap/header.html](../tchap/resources/templates/tchap/header.html) (La Suite header component)
- [tchap/resources/templates/tchap/footer.html](../tchap/resources/templates/tchap/footer.html) (La Suite footer component)
- [tchap/resources/templates/app.html](../tchap/resources/templates/app.html) (app HTML wrapper)
- [tchap/resources/templates/emails/_mail-base.html](../tchap/resources/templates/emails/_mail-base.html) (email base template)
- [tchap/resources/templates/emails/recovery.html](../tchap/resources/templates/emails/recovery.html) (account recovery email)
- [tchap/resources/templates/emails/verification.html](../tchap/resources/templates/emails/verification.html) (email verification template)
- [tchap/resources/translations/fr.json](../tchap/resources/translations/fr.json) (French translations)
- [tchap/resources/translations/en.json](../tchap/resources/translations/en.json) (English translations)
- [translations/en.json](../translations/en.json) (upstream English translations, other locales deleted)
- [frontend/locales/fr.json](../frontend/locales/fr.json) (French locale with Tchap messages, other locales deleted)
- [frontend/locales/en.json](../frontend/locales/en.json) (English locale, other locales deleted)

### 10. Frontend customizations

Customizes the React frontend with Tchap branding and Tchap-specific
routing. The page title and favicon are set to Tchap. The layout uses Tchap
header and footer components. Custom routes handle password recovery redirect
(to the MAS welcome page) and cross-signing reset with desktop deep-link
support. A password complexity utility is included (TODO: French
dictionaries). Knip configuration excludes Tchap-only files from dead-code
analysis. A shared React mount helper, a Tchap stylesheet and a dedicated
build script produce the Tchap frontend bundle.

Files:
- [frontend/index.html](../frontend/index.html) (title + favicon)
- [frontend/src/components/Layout/Layout.tsx](../frontend/src/components/Layout/Layout.tsx) (Tchap header/footer)
- [frontend/src/entrypoints/mount.tsx](../frontend/src/entrypoints/mount.tsx) (shared React mount helper)
- [frontend/src/entrypoints/shared.css](../frontend/src/entrypoints/shared.css) (shared styles)
- [frontend/src/entrypoints/vendor.css](../frontend/src/entrypoints/vendor.css) (imports the Tchap stylesheet)
- [frontend/package.json](../frontend/package.json) (Tchap build script and dependencies)
- [frontend/src/routes/_account.index.tsx](../frontend/src/routes/_account.index.tsx) (desktop flag for cross-signing reset)
- [frontend/src/routes/password.recovery.index.tsx](../frontend/src/routes/password.recovery.index.tsx) (redirect to welcome page)
- [frontend/src/routes/reset-cross-signing.tsx](../frontend/src/routes/reset-cross-signing.tsx) (desktop search param)
- [frontend/src/routes/reset-cross-signing.index.tsx](../frontend/src/routes/reset-cross-signing.index.tsx) (desktop deep-link redirect)
- [frontend/src/utils/password_complexity/index.ts](../frontend/src/utils/password_complexity/index.ts) (TODO: French dictionaries)
- [frontend/knip.config.ts](../frontend/knip.config.ts) (exclude tchap/** from knip)
- [frontend/tests/routes/reset-cross-signing.test.tsx](../frontend/tests/routes/reset-cross-signing.test.tsx) (desktop deep-link test)

### 11. Cookies

Forces `SameSite::None` on cookies to allow the MAS to be embedded in an
iframe. The SameSite restriction is instead enforced via frame-ancestor
headers.

Files:
- [crates/axum-utils/src/cookies.rs](../crates/axum-utils/src/cookies.rs) (SameSite::None override)

### 12. Build and CI

The Docker image build is self-contained: the Dockerfile builds the frontend
assets and the Rust binary internally, and adds the Tchap templates,
translations and stylesheets to the image. The CI workflow skips the Element
Server Suite (ESS) jobs, which don't apply to the fork.

The upstream `build.yaml` workflow is also modified for Tchap:
- Triggers on `main_tchap` instead of `main`.
- Docker image is pushed to `ghcr.io/tchapgouv/matrix-authentication-service`
  instead of `ghcr.io/element-hq/matrix-authentication-service`.
- The `oci-push.vpn.infra.element.io` registry and Tailscale/Vault logins are
  disabled (`if: false`).
- Archive jobs (`build-assets`, `build-binaries`, `assemble-archives`) are
  disabled (`if: false`) — the Dockerfile builds everything internally.
- The `release` and `unstable` jobs no longer depend on `assemble-archives`
  and don't upload archive files.
- Cosign signing is gated on `refs/heads/main_tchap` instead of
  `refs/heads/main`.
- Multi-arch builds (amd64 + arm64) are preserved.

The previous standalone `build_tchap.yaml` is kept as a disabled backup
(`workflow_dispatch` only).

Files:
- [Dockerfile](../Dockerfile) (Tchap resources, internal frontend and binary build)
- [.github/workflows/build.yaml](../.github/workflows/build.yaml) (Tchap CI modifications)
- [.github/workflows/ci.yaml](../.github/workflows/ci.yaml) (ESS jobs disabled)
- [.github/workflows/build_tchap.yaml](../.github/workflows/build_tchap.yaml) (disabled backup)

### 13. Handler wiring and test state

Wires the `TchapConfig` into the handlers crate and the test state
infrastructure so that handlers can access Tchap configuration via axum
state extraction in both production and tests.

Files:
- [crates/handlers/src/lib.rs](../crates/handlers/src/lib.rs) (TchapConfig import)
- [crates/handlers/src/test_utils.rs](../crates/handlers/src/test_utils.rs) (TestState field + FromRef impl)
- [crates/handlers/Cargo.toml](../crates/handlers/Cargo.toml) (tchap crate dependency)

### 14. GraphQL user mutations

Disables the anonymous-user check in the `set_password_by_recovery` GraphQL
mutation so that authenticated users can use account recovery.

Files:
- [crates/handlers/src/graphql/mutations/user.rs](../crates/handlers/src/graphql/mutations/user.rs) (anonymous-user check disabled)

### 15. OIDC end session (RP-initiated logout)

Adds an OIDC end-session endpoint so that clients can log the user out
(RP-initiated logout). The request is validated with an ID token hint, the
current browser session and its tokens are ended, a logout notification is
sent to the homeserver, and the user is redirected to the post-logout
redirect URI. The endpoint is advertised in the OIDC discovery document.

Files:
- [crates/handlers/src/oauth2/end_session.rs](../crates/handlers/src/oauth2/end_session.rs) (endpoint handler)
- [crates/handlers/src/oauth2/mod.rs](../crates/handlers/src/oauth2/mod.rs) (module registration)
- [crates/handlers/src/lib.rs](../crates/handlers/src/lib.rs) (route registration)
- [crates/handlers/src/oauth2/discovery.rs](../crates/handlers/src/oauth2/discovery.rs) (end session endpoint in discovery document)
- [crates/router/src/endpoints.rs](../crates/router/src/endpoints.rs) (route path)
- [crates/router/src/url_builder.rs](../crates/router/src/url_builder.rs) (end session endpoint URL builder)
- [crates/storage/src/oauth2/session.rs](../crates/storage/src/oauth2/session.rs) (session lookup by browser session)
- [crates/storage-pg/src/oauth2/session.rs](../crates/storage-pg/src/oauth2/session.rs) (database implementation)

### 16. Client registration policy for Tchap Desktop

Extends the OPA client registration policy so that Tchap Desktop (a Tauri
application) can register dynamically as an OAuth2 client. Its native
redirect URLs (`tauri://localhost` on macOS, `http://tauri.localhost` on
Windows) are accepted as secure, and the `tchap://`, `tchap-preprod://` and
`tchap-dev://` deep links are valid native redirectors in all environments.

Files:
- [policies/client_registration/client_registration.rego](../policies/client_registration/client_registration.rego) (Tauri redirect URLs, Tchap deep links)
- [policies/client_registration/client_registration_test.rego](../policies/client_registration/client_registration_test.rego) (policy tests)

### 17. Password visibility toggle on login page

Adds a show/hide password toggle to the login form. A reusable password
field component with a visibility button is added to the server-rendered
templates and used by the login page; the Tchap login page override uses it
as well. The button reuses the compound-web action stylesheet.

Files:
- [templates/components/password_field.html](../templates/components/password_field.html) (password field with visibility toggle)
- [templates/pages/login.html](../templates/pages/login.html) (login page uses the component)
- [tchap/resources/templates/pages/login.html](../tchap/resources/templates/pages/login.html) (Tchap login page override)
- [frontend/src/entrypoints/templates.css](../frontend/src/entrypoints/templates.css) (compound-web action stylesheet import)

## Excluded files (Tchap-only, no upstream counterpart)

These files exist only in the Tchap fork and have no upstream equivalent:
- `crates/tchap/**` — the Tchap crate
- `tchap/resources/**` — Tchap templates, emails, and resources
- `tchap/tmp/` — generated template copies (build output, do not edit)
- `tchap/start*.sh`, `tchap/docker-compose.yml`, `tchap/build*.sh` — Tchap dev tooling
- `.github/workflows/build_tchap.yaml` — Tchap CI workflow (disabled backup)

Generated files are not tagged either: lockfiles (`Cargo.lock`,
`pnpm-lock.yaml`), test snapshots, generated GraphQL and OpenAPI code, and
the sqlx query cache.

## Upstream merge procedure

When merging a new upstream tag:

1. Read this file to understand which features touch which files.
2. For each conflict, check if the file has `:tchap:` tags.
3. Preserve all `:tchap:` tagged code.
4. Accept the upstream version for non-tagged conflicts.
5. Adapt `:tchap:` code if the upstream API has changed.
6. Update the `tag:` line at the top of this file.
7. Run `cargo check --workspace` and fix any compilation errors.
