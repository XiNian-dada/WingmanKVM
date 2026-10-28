# Vendored xterm.js assets

Five upstream files are vendored with byte-identical contents. The only local filename
change is `LICENSE.addon-fit`; both npm archives name their file `package/LICENSE`.
Nothing was rewritten, minified, or patched. `SOURCES.json` is the authoritative source
for versions, commits, archive URLs, integrity values, and per-file SHA-256 hashes.

## Source mapping

Both packages come from <https://github.com/xtermjs/xterm.js> at commit
`f447274f430fd22513f6adbf9862d19524471c04`.

| Local file | Archive member |
| --- | --- |
| `web/vendor/xterm/xterm.js` | `@xterm/xterm` `package/lib/xterm.js` |
| `web/vendor/xterm/xterm.css` | `@xterm/xterm` `package/css/xterm.css` |
| `web/vendor/xterm/LICENSE` | `@xterm/xterm` `package/LICENSE` |
| `web/vendor/xterm/addon-fit.js` | `@xterm/addon-fit` `package/lib/addon-fit.js` |
| `web/vendor/xterm/LICENSE.addon-fit` | `@xterm/addon-fit` `package/LICENSE` (renamed) |

Both packages are MIT licensed; license files are retained and hash-checked. Upstream
`*.js.map` debug files are intentionally not vendored and are not runtime assets.

## Verification

```sh
python3 scripts/verify_xterm_vendor.py
```

The checker is offline: no networking, install, writes, extraction, or evaluation. It
requires exactly the two package names and five fixed file mappings, validates required
metadata, version/commit/hex/SRI formats, HTTPS registry URLs consistent with each
package name and version, MIT license mapping, paths, and duplicate local paths. Local
bytes are compared to manifest hashes. Symlinks are rejected.

To verify the official archives too:

```sh
tmpdir="$(mktemp -d)"
cleanup() {
  rm -f "$tmpdir/xterm-6.0.0.tgz" "$tmpdir/addon-fit-0.11.0.tgz"
  rmdir "$tmpdir"
}
trap cleanup EXIT
curl --fail --location --proto '=https' --tlsv1.2 \
  --output "$tmpdir/xterm-6.0.0.tgz" \
  "https://registry.npmjs.org/@xterm/xterm/-/xterm-6.0.0.tgz"
curl --fail --location --proto '=https' --tlsv1.2 \
  --output "$tmpdir/addon-fit-0.11.0.tgz" \
  "https://registry.npmjs.org/@xterm/addon-fit/-/addon-fit-0.11.0.tgz"
python3 scripts/verify_xterm_vendor.py --archives-dir "$tmpdir"
```

Cleanup removes only those two known files, then calls `rmdir`; no recursive deletion is
used. Never use `curl -k`. The checker verifies archive SHA-256, SHA-512 SRI, and every
mapped regular tar member with in-memory `extractfile` only.

## Update recipe

1. Download the official archives with validated TLS and verify their integrity.
2. Compare and replace only the five mapped members; do not unpack the whole archive.
3. Update `SOURCES.json` once with new versions, commit, URL, integrity values, and file
   hashes in the same change. The checker has no pinned versions or hashes; edit its
   static package/path mapping only if package names or file paths themselves change.
4. Refresh versioned examples in this README when applicable.
5. Re-run both verification modes. Keep the static layout and licenses; do not add npm,
   frontend build, or package-manager dependencies.
