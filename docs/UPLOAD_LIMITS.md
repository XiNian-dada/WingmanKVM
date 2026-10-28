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
