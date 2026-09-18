# xmip-core-transport-peppol

Peppol transport: the OpenPeppol AS4 profile over the as4 technology — one business document in its Standard Business Document Header is one Stream, its participants beside it as `iso6523-actorid-upis::0088:…` identifiers; a Receive Location unwraps what an access point posts, a Send Location wraps a document and resolves the far end from a static table or a given URL. A technology of [xmip-core-transport](https://github.com/IlleNilsson/xmip-core-transport).

## Toolchain

`rust-toolchain.toml` pins the toolchain for the whole estate. Do not change it
here.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.
