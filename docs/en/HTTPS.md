# HTTPS and certificates

Use **Settings → HTTPS and certificates** to select a mode, enter the access URL, and save. Built-in TLS uses `server.port`; port 443 is not required. With public TCP `38319 → device 8080`, enter `https://your-domain:38319` and keep the forwarding rule. Input and terminal sockets use WSS; WebRTC UDP forwarding is unchanged.

## Certificate modes

- **Automatic self-signed (default for new installations)**: generates a device-specific ECDSA P-256 identity on first startup and reuses it after restart. No domain verification or DNS TXT records are needed. Validity is 3650 days. Changing the access URL generates a certificate covering its hostname/IP; the regeneration checkbox also creates a new identity. Clients must trust the replacement again.
- **Custom certificate**: enter the HTTPS hostname covered by the certificate, then upload the PEM full chain (leaf first) and matching unencrypted PEM private key. The server checks key matching, validity, hostname, chain order and signatures before saving. A rejected import leaves the current certificate intact. Leave both files empty to reuse a saved custom certificate.

Switching modes retains the other mode's identity. New TLS connections immediately use the selected certificate; existing connections continue until closed. Settings shows expiration and the SHA-256 fingerprint, and downloads only the **public certificate**, never the private key.

Self-signed certificates encrypt traffic but are not trusted by browsers by default. Initial access may require confirming a certificate warning. For reliable Passkey, WebCodecs and other secure-context APIs, install client trust or use a trusted CA certificate; ignoring a warning is not equivalent to establishing trust. Importing a custom certificate does not itself establish client trust either.

## Migrating an existing HTTP installation

Existing `config.json` files without `server.tls` retain HTTP across upgrades. First enable automatic self-signed mode in the existing settings page, then open the displayed HTTPS URL. When enabling Secure cookies, the server revokes old sessions and clears this browser's old cookie. Sign in again over HTTPS before importing a CA certificate. Private-key uploads over plain HTTP are rejected.

With TLS enabled, HTTP GET/HEAD on the same port redirect to HTTPS; other plaintext requests return 426. Local installer and diagnostic `/healthz` probes can still use HTTP, only when both the TCP peer and the PPv2 client address are loopback addresses.

## Expiration and persistence

This version **does not implement ACME or DNS API renewal**. Renew custom certificates through their issuer and import replacements before expiry. Self-signed certificates can be regenerated in Settings; they are not silently rotated on restart or near expiry, since doing so would invalidate established client trust. An expired, missing or unreadable active certificate prevents startup instead of silently downgrading to HTTP; repair the device configuration and restart.

PEM identities are stored under `tls/` in the state directory (default `/var/lib/wingmankvm`), with directory mode 0700 and private-key file mode 0600. Config stores only device-local identity IDs. Preserve this directory across upgrades and device backups. JSON exports do not contain keys; configuration import retains the destination's TLS mode, certificate IDs and access URL.

## Reverse proxies and recovery

New installations terminate TLS themselves. If Caddy/Nginx already terminates HTTPS and uses an HTTP upstream, set `server.tls.mode` to `disabled` in the host configuration and configure the external HTTPS `server.public_origin`. Preserve the external Host header. Forwarded headers cannot assert that a connection was encrypted; manage certificates at the TLS-terminating proxy.

To restore HTTP locally, merge this fragment into the existing `server` object and restart, preserving the rest of the configuration:

```json
{
  "tls": { "mode": "disabled" },
  "public_origin": null
}
```

Without `public_origin`, native TLS WebSockets accept HTTPS origins matching Host. With it configured, only that exact origin is accepted. See [deployment](DEPLOYMENT.md#11-https) and [cookie policy](../SESSION_COOKIES.md) (Chinese).

## Validation

`cargo test --locked tls` covers certificate persistence, permissions, import validation, switching, real HTTPS/WSS and PPv2. `python3 scripts/verify_tls_ui.py --browser-channel chrome --output-dir output/playwright/tls-settings` checks Settings using temporary certificates and mocked APIs; it ignores browser trust errors for that fixture and does not establish trust for a real device.

## Passkey does not open the authenticator

Chromium can report `isSecureContext=true` while rejecting WebAuthn with `NotAllowedError: WebAuthn is not supported on sites with TLS certificate errors.` Video and ordinary HTTPS requests may still work. Explicit Passkey attempts now show the error beneath the button instead of silently ignoring it.

Sign in with your password, download the public certificate from Settings → HTTPS and certificates, verify its fingerprint and trust it in the client certificate store; some browsers use their own store. Alternatively import a certificate issued by a trusted CA. Bypassing the browser warning is insufficient. Passkeys are also scoped to the registration domain: when moving to a different domain, sign in with your password and register a new credential there. The port is not part of the RP ID.
