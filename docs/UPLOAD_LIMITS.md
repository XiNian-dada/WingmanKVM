# Upload limits and retry behavior

WingmanKVM accepts one virtual-media image upload at a time.

## Deadlines

- Network idle deadline: **15 seconds**. It applies while waiting for the next
  multipart field and while reading each field chunk, including ignored fields.
  Any non-empty chunk counts as progress; empty chunks do not reset the clock.
- Total request deadline: **2 hours**, covering field parsing, body reads, disk
  writes and the final sync. A healthy large file is not rejected merely for its
  size; the existing `media.max_upload_bytes` limit still applies.

## Responses

- `408 Request Timeout` — idle or total deadline elapsed. The partial file is
  removed and no destination file is published.
- `413 Payload Too Large` — the upload exceeded `media.max_upload_bytes`.
- `409 Conflict` — another upload is already active, or the target name already
  exists. Retry a busy upload after the current one ends.

## Publication and cleanup

The server writes to a private random `.partial` file with create-new semantics.
It syncs the data before publishing, then uses a same-directory hard link to
create the final name without overwriting an existing file. An existing valid
image is never replaced or deleted, including when two uploads race. On
disconnect, cancellation, timeout, size-limit rejection or I/O failure only
that request's temporary file is removed.

## Background URL downloads

`GET /api/media/download` returns the current job, including after a browser
reload. `GET /api/media/download/{id}` reads one specific job. To cancel, send
`POST /api/media/download/cancel` with JSON `{"id":"<job id>"}`. The ID is required:
a missing or stale ID must never cancel a newer job created by another page.

Image downloads connect directly and ignore environment proxy settings. This
keeps hostname resolution under the download address filter. Private LAN/NAS
addresses remain supported; loopback and link-local targets remain refused.

The two-hour download deadline starts before directory preparation and includes
network reads, file writes and the final data sync. Cancelling or reaching the
deadline changes the observable state to `cancelling` or `timing_out` immediately.
Pending filesystem operations cannot be forcibly cancelled: the original job
keeps the transfer slot, drains those operations and removes its partial file
before becoming `cancelled` or `failed`. New transfers remain busy until then.

After syncing, the job checks cancellation and the deadline under the same lock
used by cancellation requests. If publication is admitted, state becomes
`completing`; the final name is being committed and further cancellation returns
409. The job retains its slot until this commit finishes. A deadline or late
cancel cannot undo a publication already admitted; the UI keeps polling this
phase rather than presenting an idle form or prematurely claiming completion.
