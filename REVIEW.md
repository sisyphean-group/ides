# Code Review

## What it does

A Nix library that auto-manages ephemeral services (Redis, Caddy, etc.) inside devshells — services start on shell entry and stop on exit, using systemd user units with config-hash-based idempotency.

## Strengths

- **Solid architecture**: Two-stage initialization, NixOS module system for extensibility, clean separation between options/build/cli/monitor.
- **Idempotency via hashing**: SHA256-hashing config content into the systemd unit name (`shell-{name}-{hash}`) is a clean way to prevent duplicate instances.
- **Config polymorphism**: Supporting text, file, structured content (JSON/YAML/TOML/INI/XML/PHP), and custom formatters is well thought out.
- **Good module example**: `modules/redis.nix` serves as a clear reference for writing new service modules.

## Issues & Suggestions

1. **Only one bundled module**: Only Redis is included. The framework is general but users have to write their own modules for everything else. A few more (Postgres, Caddy fully baked) would demonstrate the system's versatility and reduce onboarding friction.

2. **Monitor robustness** (`lib/monitor.nix`): The editor detection list is hardcoded (`code`, `zsh`, `vim`, `emacs`, etc.). This is fragile — `zsh` being listed as an "editor" is questionable, and any unlisted editor/tool will trigger premature shutdown. Consider inverting the logic (track shells explicitly) or making the list configurable.

3. **No error handling in generated bash** (`lib/cli.nix`, `lib/monitor.nix`): The generated shell scripts lack `set -e` or meaningful error handling. If `systemd-run` or `socat` fails, the user gets silent failures or confusing output.

4. **`%CFG%` templating** (`lib/build.nix`): String replacement of `%CFG%` in args is simple but fragile — no escaping or validation. If a user's args contain `%CFG%` literally (unlikely but possible), there's no way to escape it.

5. **Security consideration**: `systemd-run --user` with `--unit` names derived from user input (service names) — these aren't sanitized. Malicious or accidental special characters in service names could cause unexpected behavior.

6. **Documentation generation** (`docs/gendocs.sh`): The script uses a hardcoded Nix store path for `nixpkgs`, which will break on other machines. Should use a pinned input or the flake's nixpkgs.

7. **No tests**: There's no test infrastructure. Even basic smoke tests (e.g., evaluating module options, verifying generated scripts) would catch regressions.

8. **`flake.nix` is minimal**: No `devShells` output for self-development, no CI checks, no formatter. The flake template is useful but the flake itself could do more.

## Minor Nits

- The `et-tu` command alias for stop-all is fun but non-discoverable — good that it's documented in help output.
- The `TODO` file mentions PrivateDirectories/sysext but gives no context on priority or design direction.
- `name` file containing the project description is unconventional; this metadata could live solely in `flake.nix`.

## Overall

Well-designed Nix library with a clear problem domain and clean abstractions. The core idempotency mechanism is sound. Main gaps are in hardening (error handling, input sanitization), test coverage, and expanding the module ecosystem beyond Redis.
