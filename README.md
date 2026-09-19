a quick-and-dirty paste service for self-hosting. not super secure.

## config

all via environment variables:

| var | default | notes |
| --- | --- | --- |
| `QPASTE_DATA_DIR` | *(required)* | where pastes are written. created on startup if missing. |
| `QPASTE_ADDR` | `127.0.0.1:3000` | address to bind to. |
| `QPASTE_BASE_URL` | `http://$QPASTE_ADDR` | the base url handed back to uploaders. set this if you're behind a reverse proxy or binding to `0.0.0.0`, otherwise the returned links won't be reachable. trailing slash is optional. |


## security notes


- no protection against ID collisions. The ID space is moderately big (and this service isn't expected to get much traffic) so it's not a huge deal, but enumeration of IDs is a possibility. Don't use for sensitive information.
- no content validation. Content is served as-is with no processing or mime type sniffing, and consumers get axum's default content-type of `application/octet-stream`.
- no rate-limiting. need to keep an eye on traffic; abuse or denial-of-service pretty likely if something malicious comes across the service.
- internal server errors are logged directly to clients; not sure but this could end up exposing sensitive implementation details.
