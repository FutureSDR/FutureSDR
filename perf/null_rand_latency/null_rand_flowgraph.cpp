#include "null_rand_flowgraph.hpp"

#include <algorithm>
#include <chrono>
#include <numeric>
#include <functional>
#include <boost/format.hpp>

#include <gnuradio/blocks/head.h>
#include <gnuradio/sync_block.h>
#include "tp.h"

namespace po = boost::program_options;

const uint64_t GRANULARITY = 32768;

using namespace gr;

// ============================================================
// NULL SOURCE LATENCY
// ============================================================
class null_source_latency : virtual public sync_block
{
private:
    uint64_t d_granularity;
public:
    typedef std::shared_ptr<null_source_latency> sptr;
    static sptr make(size_t sizeof_stream_item, uint64_t granularity);

    null_source_latency(size_t sizeof_stream_item, uint64_t granularity);

    int work(int noutput_items,
             gr_vector_const_void_star& input_items,
             gr_vector_void_star& output_items) override;
};

null_source_latency::sptr null_source_latency::make(size_t sizeof_stream_item, uint64_t granularity)
{
    return gnuradio::make_block_sptr<null_source_latency>(sizeof_stream_item, granularity);
}

null_source_latency::null_source_latency(size_t sizeof_stream_item, uint64_t granularity)
    : d_granularity(granularity), sync_block("null_source_latency",
                 io_signature::make(0, 0, 0),
                 io_signature::make(1, -1, sizeof_stream_item))
{
}

int null_source_latency::work(int noutput_items,
                           gr_vector_const_void_star& input_items,
                           gr_vector_void_star& output_items)
{
    void* optr;
    for (size_t n = 0; n < input_items.size(); n++) {
        optr = (void*)output_items[n];
        memset(optr, 0, noutput_items * output_signature()->sizeof_stream_item(n));
    }

    uint64_t items = nitems_written(0);
    uint64_t before = items / d_granularity;
    uint64_t after = (items + noutput_items) / d_granularity;
    if (before ^ after) {
        tracepoint(null_rand_latency, tx, 0, after);
        std::cout << "trigger source " << after << std::endl;
    }

    return noutput_items;
}

// ============================================================
// NULL SINK LATENCY
// ============================================================
class null_sink_latency : virtual public sync_block
{
private:
    uint64_t d_granularity;
public:
    typedef std::shared_ptr<null_sink_latency> sptr;
    static sptr make(size_t sizeof_stream_item, uint64_t granularity);

    null_sink_latency(size_t sizeof_stream_item, uint64_t granularity);

    int work(int noutput_items,
             gr_vector_const_void_star& input_items,
             gr_vector_void_star& output_items) override;
};

null_sink_latency::sptr null_sink_latency::make(size_t sizeof_stream_item, uint64_t granularity)
{
    return gnuradio::make_block_sptr<null_sink_latency>(sizeof_stream_item, granularity);
}

null_sink_latency::null_sink_latency(size_t sizeof_stream_item, uint64_t granularity)
    : d_granularity(granularity), sync_block("null_sink_latency",
                 io_signature::make(1, -1, sizeof_stream_item),
                 io_signature::make(0, 0, 0))
{
}

int null_sink_latency::work(int noutput_items,
                         gr_vector_const_void_star& input_items,
                         gr_vector_void_star& output_items)
{
    uint64_t items = nitems_read(0);
    uint64_t before = items / d_granularity;
    uint64_t after = (items + noutput_items) / d_granularity;
    if (before ^ after) {
        tracepoint(null_rand_latency, tx, 0, after);
    }

    return noutput_items;
}


// ============================================================
// FLOWGRAPH
// ============================================================
null_rand_flowgraph::null_rand_flowgraph(int pipes, int stages, uint64_t samples, size_t max_copy) {

    this->tb = gr::make_top_block("buf_flowgraph");

    for(int pipe = 0; pipe < pipes; pipe++) {

        auto src = null_source_latency::make(4, GRANULARITY);
        auto head = blocks::head::make(4, samples);
        tb->connect(src, 0, head, 0);

        auto prev = sched::copy_rand::make(sizeof(float), max_copy);
        tb->connect(head, 0, prev, 0);

        for(int stage = 1; stage < stages; stage++) {
            auto block = sched::copy_rand::make(sizeof(float), max_copy);
            tb->connect(prev, 0, block, 0);
            prev = block;
        }

        auto sink = null_sink_latency::make(sizeof(float), GRANULARITY);
        tb->connect(prev, 0, sink, 0);
    }
}

int main (int argc, char **argv) {
    int run;
    int pipes;
    int stages;
    uint64_t samples;
    uint64_t max_copy;

    po::options_description desc("Run Buffer Flow Graph");
    desc.add_options()
        ("help,h", "display help")
        ("run,r", po::value<int>(&run)->default_value(0), "Run Number")
        ("pipes,p", po::value<int>(&pipes)->default_value(5), "Number of pipes")
        ("stages,s", po::value<int>(&stages)->default_value(6), "Number of stages")
        ("max_copy,m", po::value<uint64_t>(&max_copy)->default_value(512), "Maximum number of samples to copy in one go.")
        ("samples,n", po::value<uint64_t>(&samples)->default_value(15000000), "Number of samples");

    po::variables_map vm;
    po::store(po::parse_command_line(argc, argv, desc), vm);
    po::notify(vm);

    if (vm.count("help")) {
        std::cout << desc << std::endl;
        return 0;
    }

    null_rand_flowgraph* runner = new null_rand_flowgraph(pipes, stages, samples, max_copy);
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
