#undef TRACEPOINT_PROVIDER
#define TRACEPOINT_PROVIDER null_rand_latency

#undef TRACEPOINT_INCLUDE
#define TRACEPOINT_INCLUDE "./tp.h"

#if !defined(_HELLO_TP_H) || defined(TRACEPOINT_HEADER_MULTI_READ)
#define _HELLO_TP_H

#include <lttng/tracepoint.h>

TRACEPOINT_EVENT(
    null_rand_latency,
    tx,
    TP_ARGS(
        uint64_t, block,
        uint64_t, samples
    ),
    TP_FIELDS(
        ctf_integer(uint64_t, block, block)
        ctf_integer(uint64_t, samples, samples)
    )
)

TRACEPOINT_EVENT(
    null_rand_latency,
    rx,
    TP_ARGS(
        uint64_t, block,
        uint64_t, samples
    ),
    TP_FIELDS(
        ctf_integer(uint64_t, block, block)
        ctf_integer(uint64_t, samples, samples)
    )
)
#endif /* _HELLO_TP_H */

#include <lttng/tracepoint-event.h>
