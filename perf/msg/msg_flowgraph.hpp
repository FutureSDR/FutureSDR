#ifndef MSG_FLOWGRAPH_HPP
#define MSG_FLOWGRAPH_HPP

#include <gnuradio/top_block.h>
#include <sched/msg_forward.h>

#include <iostream>
#include <boost/program_options.hpp>

using namespace gr;

class msg_flowgraph {

private:

    int d_pipes;
    int d_stages;

    void create_fork();
    void create_diamond();

public:
    msg_flowgraph(int pipes, int stages);
	~msg_flowgraph();

    top_block_sptr tb;
    sched::msg_forward::sptr src;
};


#endif
