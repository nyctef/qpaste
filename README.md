a quick-and-dirty paste service for self-hosting. not super secure.

some security notes:

- no protection against ID collisions. The ID space is moderately big (and this service isn't expected to get much traffic) so it's not a huge deal, but enumeration of IDs is a possibility. Don't use for sensitive information.
- no content validation. Content is served as-is with no processing or mime type sniffing, and consumers get axum's default content-type of `application/octet-stream`.
- no rate-limiting. need to keep an eye on traffic; abuse or denial-of-service pretty likely if something malicious comes across the service.
- internal server errors are logged directly to clients; not sure but this could end up exposing sensitive implementation details.
