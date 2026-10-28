# HID configuration

Issue #20 retired the HID timing options `hid.write_timeout_ms` and
`hid.retry_interval_ms`. They never controlled the HID write path, so they are
removed rather than given new hardware behaviour.

## Current behaviour

- Every HID report is written through a readiness wait with a fixed **500 ms
  deadline per report**. The deadline is not configurable and applies to each
  report individually, so a request that emits several reports may take longer
  than 500 ms in total.
- Waiting is readiness-driven: the worker waits until the device becomes
  writable again. The former `retry_interval_ms` polling interval is obsolete
  and has no replacement setting.
- Failures still surface as the existing timeout, short-write, I/O or queue
  errors.

## Migrating a legacy configuration file

An existing `config.json` may still contain the retired keys:

```json
{
  "hid": {
    "keyboard_device": "/dev/hidg0",
    "write_timeout_ms": 500,
    "retry_interval_ms": 10
  }
}
```

Such files load normally. All active HID fields are preserved and both retired
keys are ignored. Startup logs the field name and a fixed reason for each
retired key; the stored values are never logged.

The retired keys are not part of the normalized configuration. `GET
/api/config`, the bootstrap response and the next atomic save omit them, so a
legacy file is cleaned up the next time the configuration is saved. A saved
file therefore contains only the active HID fields:

```json
{
  "hid": {
    "auto_detect": true,
    "keyboard_device": "/dev/hidg0",
    "mouse_device": null,
    "absolute_pointer_device": null,
    "pointer_mode": "auto"
  }
}
```

## Updating configuration over HTTP

Both `POST /api/config` (partial patch) and `PUT /api/config` (full
configuration) reject either retired HID key. This includes former defaults,
custom values and `null`:

```json
{"hid": {"write_timeout_ms": null}}
```

Rejection happens while decoding the request, before validation or any commit,
so the in-memory configuration and the file on disk are left unchanged. The
error names the field and gives the fixed reason that the option is deprecated
and ignored. Remove the key and resend the request.

`PUT /api/config` otherwise keeps the full `Config` contract unchanged and does
not add broader unknown-field rejection. `PUT` is not a way to migrate legacy
timing keys: legacy files migrate by loading successfully and being normalized
on their next save.
