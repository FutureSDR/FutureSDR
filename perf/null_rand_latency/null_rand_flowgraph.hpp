#ifndef NULL_RAND_FLOWGRAPH_HPP
#define NULL_RAND_FLOWGRAPH_HPP

#include <gnuradio/top_block.h>
#include <sched/copy_rand.h>

#include <boost/program_options.hpp>
#include <iostream>

using namespace gr;

class null_rand_flowgraph {

public:
    null_rand_flowgraph(
            int pipes, int stages, uint64_t samples, size_t max_copy);
    top_block_sptr tb;
};


#endif
