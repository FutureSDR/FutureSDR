#ifndef MSG_FLOWGRAPH_HPP
#define MSG_FLOWGRAPH_HPP

#include <gnuradio/top_block.h>
#include <sched/msg_forward.h>

#include <iostream>
#include <boost/program_options.hpp>

using namespace gr;

class msg_flowgraph {

public:
    msg_flowgraph(int pipes, int stages);

    top_block_sptr tb;

    std::vector<sched::msg_forward::sptr> d_srcs;
};


#endif
