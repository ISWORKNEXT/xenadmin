# Unit R1 — Fix CI and prepare the M1 release

Read `docs/supervision/CHARTER.md` first. It is binding.

## Why

GitHub Actions "Rust desktop checks" has failed on every push since S1
(commit 555e4ad9e). The failing step is "Native clipboard regression
(X11)": `tools/audit_clipboard_live.py` clicks fixed pixel points from the
old layout (1360,77 for the old "Add connection" button, and
`--host-point 700 378` from `.github/workflows/checks.yml` for the Host
field), so it never reaches the dialog and reports `COPY_HAS_NOT_OCCURRED`.

## Deliver

1. Make the clipboard check independent of pixel layout: open the Add New
   Server dialog through a stable path (a keyboard shortcut the app already
   has, or File > Add New Server via keyboard navigation, or an existing
   startup flag that opens it). Focus the Host field the same way (the
   dialog should focus it on open; if it does not, make it — upstream
   focuses the server field). Keep the test's purpose: paste a Unicode
   string with Ctrl+V, copy back with Ctrl+C, compare via an independent
   clipboard client.
2. Update `.github/workflows/checks.yml` arguments accordingly.
3. Run the same Xvfb sequence locally (`Xvfb :98`, `DISPLAY=:98`) and get
   `"passed": true`.
4. Set the crate versions in `native/Cargo.toml` and
   `crates/xen-core/Cargo.toml` to `0.2.0` (update `Cargo.lock`).
5. Write `docs/releases/v0.2.0.md`: short user-facing notes for M1 (new
   XCP-ng Center layout: menu bar/toolbar/status bar, navigation tree and
   view modes, per-object tabs, Search tab, context menus and
   multi-select, Notifications: Alerts/Events) plus "Known gaps" from the
   FOUND/NOT VERIFIED lists in `docs/supervision/logs/S*-report.txt`.
   Plain language, no internal unit names.

## Accept

- fmt, clippy, tests pass; local Xvfb clipboard check passes.
- Do not tag or push; the supervisor will.
