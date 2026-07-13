#ifndef MSG_FLOWGRAPH_HPP
#define MSG_FLOWGRAPH_HPP

#include <gnuradio/top_block.h>
#include <sched/msg_burst.h>
#include <sched/msg_forward.h>
#include <sched/msg_sink.h>

#include <boost/program_options.hpp>
#include <iostream>
#include <vector>

using namespace gr;

class msg_flowgraph {

public:
    msg_flowgraph(int pipes, int stages, int burst_size);

    bool counts_match(std::uint64_t expected) const;

    top_block_sptr tb;

    std::vector<sched::msg_sink::sptr> d_sinks;
};


#endif
