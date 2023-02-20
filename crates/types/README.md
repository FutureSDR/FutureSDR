# FutureSDR Types

FutureSDR types that are used to exchange information between the runtime and the outside world through the REST API.

[![Crates.io][crates-badge]][crates-url]
[![Apache 2.0 licensed][apache-badge]][apache-url]
[![Docs][docs-badge]][docs-url]

[crates-badge]: https://img.shields.io/crates/v/futuresdr-types.svg
[crates-url]: https://crates.io/crates/futuresdr-types
[apache-badge]: https://img.shields.io/badge/license-Apache%202-blue
[apache-url]: https://github.com/futuresdr/futuresdr/blob/master/LICENSE
[docs-badge]: https://img.shields.io/docsrs/futuresdr-types
[docs-url]: https://docs.rs/futuresdr-types/

This includes
- `Pmt`s (polymorphic types) that are used as the input and output type for message passing.
- `FlowgraphDescription` and `BlockDescription`, which describe the topology of the flowgraph and the block, respectively.
