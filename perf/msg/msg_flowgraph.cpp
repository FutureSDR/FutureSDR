#include "msg_flowgraph.hpp"

#include <boost/format.hpp>
#include <chrono>
#include <stdexcept>

namespace po = boost::program_options;

using namespace gr;


msg_flowgraph::msg_flowgraph(int pipes, int stages, int burst_size) {

    this->tb = make_top_block("msg_flowgraph");

    for (int pipe = 0; pipe < pipes; pipe++) {
        sched::msg_burst::sptr src = sched::msg_burst::make(1.23, burst_size);
        basic_block_sptr prev = src;

        for (int stage = 0; stage < stages; stage++) {
            sched::msg_forward::sptr block = sched::msg_forward::make();
            tb->msg_connect(prev, "out", block, "in");
            prev = block;
        }

        sched::msg_sink::sptr sink = sched::msg_sink::make();
        tb->msg_connect(prev, "out", sink, "in");
        d_sinks.push_back(sink);
    }
}

bool msg_flowgraph::counts_match(std::uint64_t expected) const {
    for (const auto &sink : d_sinks) {
        if (sink->received() != expected) {
            return false;
        }
    }
    return true;
}

int main(int argc, char **argv) {
    int run;
    int pipes;
    int stages;
    int repetitions;
    int burst_size;

    po::options_description desc("MSG Flow Graph");
    desc.add_options()("help,h", "display help")("run,r",
            po::value<int>(&run)->default_value(1),
            "Run Number")("pipes,p", po::value<int>(&pipes)->default_value(4),
            "Number of pipes")("stages,s",
            po::value<int>(&stages)->default_value(6), "Number of stages")(
            "repetitions,R", po::value<int>(&repetitions)->default_value(1),
            "Number of repetitions")("burst_size,b",
            po::value<int>(&burst_size)->default_value(10000),
            "Number of PDUs per burst");

    po::variables_map vm;
    po::store(po::parse_command_line(argc, argv, desc), vm);
    po::notify(vm);

    if (vm.count("help")) {
        std::cout << desc << std::endl;
        return 0;
    }

    if (pipes < 1 || stages < 0 || repetitions < 1 || burst_size < 0) {
        throw std::invalid_argument("invalid message flowgraph parameters");
    }

    for (int repetition = 0; repetition < repetitions; repetition++) {
        msg_flowgraph runner(pipes, stages, burst_size);

        auto start = std::chrono::steady_clock::now();
        runner.tb->run();
        auto finish = std::chrono::steady_clock::now();

        if (!runner.counts_match(burst_size)) {
            throw std::runtime_error(
                    "message flowgraph did not receive the expected messages");
        }

        auto time = std::chrono::duration<double>(finish - start).count();

        std::cout << boost::format("%1$4d, %2$4d,  %3$4d,   %4$4d,       "
                                   "%5$4d, legacy,       %6$20.12f") %
                             run % pipes % stages % repetition % burst_size %
                             time
                  << std::endl;
    }

    return 0;
}
