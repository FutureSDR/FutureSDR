#ifndef NULL_RAND_FLOWGRAPH_HPP
#define NULL_RAND_FLOWGRAPH_HPP

#include <gnuradio/top_block.h>
#include <sched/copy_rand.h>

#include <iostream>
#include <boost/program_options.hpp>

using namespace gr;

class null_rand_flowgraph {

private:
    size_t d_max_copy;
    int d_pipes;
    int d_stages;
    uint64_t d_samples;

public:
    null_rand_flowgraph(int pipes, int stages,
                   uint64_t samples, size_t max_copy);
	~null_rand_flowgraph();

	top_block_sptr tb;
    std::vector<sched::copy_rand::sptr> blocks;
};


#endif
