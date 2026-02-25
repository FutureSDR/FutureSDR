#ifndef ADD_FLOWGRAPH_HPP
#define ADD_FLOWGRAPH_HPP

#include <gnuradio/top_block.h>

#include <boost/program_options.hpp>
#include <iostream>

using namespace gr;

class add_flowgraph {

public:
    add_flowgraph( int pipes, int stages, uint64_t samples);
    top_block_sptr tb;
};


#endif
