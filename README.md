# xmip-core-transport-peppol

Peppol transport: the OpenPeppol AS4 profile over the as4 technology — one business document in its Standard Business Document Header is one Stream, its participants beside it as `iso6523-actorid-upis::0088:…` identifiers; a Receive Location unwraps what an access point posts, a Send Location wraps a document and resolves the far end from a static table or a given URL. A technology of [xmip-core-transport](https://github.com/IlleNilsson/xmip-core-transport).

A Receive Location keeps its listener, bound on the first receive, and the connections senders keep open on it (`http::inbound::Inbound`): each receive takes the next request from whichever sends first, where until 2026-09-27 each receive bound a listener of its own, answered one request with `Connection: close`, and refused a request that came between two receives.

An access point written `as4://` is read under AS4's declared schemes (`as4::SCHEMES`) by `net::Endpoint`; until 2026-09-28 it was rewritten by AS4's `as_http` first.

## Acknowledged after the receive cycle

The sending access point waits for its Receipt until the runtime's whole receive cycle has ended, as AS4's does: the Receipt on `Accepted`; on `Refused` a final ebMS Error (`EBMS:0101` for a sender not identified, `EBMS:0004` otherwise) with HTTP's `401`, `403` or `422`, so it does not send the document again; on `Failed` `503` and an ebMS Error `EBMS:0004`, so it sends the document again. A document whose body broke as it was read fails; one that passed the check and still cannot be unwrapped is refused as unacceptable. The profile and the Standard Business Document Header — that its receiver is this participant — are checked as the message is read, through AS4's `checking`, and a document that fails is answered its Error (`EBMS:0103`) at once rather than receipted. Until 2026-10-02 the header's receiver was checked after the Receipt was written. The header is read twice, once to check and once to unwrap, an in-memory step; no round trip is added.

## Toolchain

`rust-toolchain.toml` pins the toolchain for the whole estate. Do not change it
here.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.
