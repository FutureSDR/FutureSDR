#include <algorithm>
#include <chrono>
#include <random>
#include <numeric>
#include <functional>
#include <boost/format.hpp>
#include <boost/program_options.hpp>
#include <iostream>

#include <gnuradio/blocks/head.h>
#include <gnuradio/blocks/interleave.h>
#include <gnuradio/blocks/null_sink.h>
#include <gnuradio/blocks/null_source.h>
#include <gnuradio/top_block.h>
#include <gnuradio/filter/fir_filter_blk.h>
#include <sched/copy_rand.h>

namespace po = boost::program_options;

using namespace gr;

class fir_rand_flowgraph {

public:
    fir_rand_flowgraph(
            int pipes, int stages, uint64_t samples, size_t max_copy);
    top_block_sptr tb;
};

fir_rand_flowgraph::fir_rand_flowgraph(int pipes, int stages, uint64_t samples, size_t max_copy) {

    std::default_random_engine e;
    std::uniform_real_distribution<float> dis(0, 1);

    std::vector<float> taps;
    for (int i = 0; i < 64; i++) {
        taps.emplace_back(dis(e));
    }

    // std::cout << "taps size " << taps.size() << std::endl;
    // for(float f: taps) {
    //     std::cout << f << ", ";
    // }
    // std::cout << std::endl;

    this->tb = gr::make_top_block("fir_flowgraph");

    for(int pipe = 0; pipe < pipes; pipe++) {

        auto src = blocks::null_source::make(4);
        auto head = blocks::head::make(4, samples);
        tb->connect(src, 0, head, 0);


        auto copy = sched::copy_rand::make(sizeof(float), max_copy);
        auto prev = filter::fir_filter_fff::make(1, taps);
        tb->connect(head, 0, copy, 0);
        tb->connect(copy, 0, prev, 0);

        for(int stage = 1; stage < stages; stage++) {
            auto block = sched::copy_rand::make(sizeof(float), max_copy);
            tb->connect(prev, 0, block, 0);
            prev = filter::fir_filter_fff::make(1, taps);
            tb->connect(block, 0, prev, 0);
        }

        auto sink = blocks::null_sink::make(sizeof(float));
        tb->connect(prev, 0, sink, 0);
    }
}

int main (int argc, char **argv) {
    int run;
    int pipes;
    int stages;
    uint64_t samples;
    size_t max_copy;

    po::options_description desc("Run Buffer Flow Graph");
    desc.add_options()
        ("help,h", "display help")
        ("run,r", po::value<int>(&run)->default_value(0), "Run Number")
        ("pipes,p", po::value<int>(&pipes)->default_value(5), "Number of pipes")
        ("stages,s", po::value<int>(&stages)->default_value(6), "Number of stages")
        ("max_copy,m", po::value<size_t>(&max_copy)->default_value(0xffffffff), "Maximum number of samples to copy in one go.")
        ("samples,n", po::value<uint64_t>(&samples)->default_value(15000000), "Number of samples");

    po::variables_map vm;
    po::store(po::parse_command_line(argc, argv, desc), vm);
    po::notify(vm);

    if (vm.count("help")) {
        std::cout << desc << std::endl;
        return 0;
    }

    fir_rand_flowgraph* runner = new fir_rand_flowgraph(pipes, stages, samples, max_copy);
    // runner->tb->set_max_output_buffer(4096);

    auto start = std::chrono::high_resolution_clock::now();
    runner->tb->run();
    auto finish = std::chrono::high_resolution_clock::now();
    auto time = std::chrono::duration_cast<std::chrono::nanoseconds>(finish-start).count()/1e9;

    std::cout <<
    boost::format("%1$4d, %2$4d,  %3$4d,   %4$15d,%5$10d,legacy,   %6$20.15f") %
                     run  % pipes % stages % samples % max_copy % time << std::endl;

    return 0;
}
