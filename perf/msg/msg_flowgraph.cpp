#include "msg_flowgraph.hpp"

#include <chrono>
#include <boost/format.hpp>
#include <gnuradio/realtime.h>

namespace po = boost::program_options;

using namespace gr;


msg_flowgraph::msg_flowgraph(int pipes, int stages) {

    this->tb = make_top_block("msg_flowgraph");

    sched::msg_forward::sptr prev;

    for(int pipe = 0; pipe < pipes; pipe++) {
        prev = sched::msg_forward::make();
        d_srcs.push_back(prev);

        for(int stage = 1; stage <= stages; stage++) {
            sched::msg_forward::sptr block = sched::msg_forward::make();
            tb->msg_connect(prev, "out", block, "in");
            prev = block;
        }
    }
}

int main (int argc, char **argv) {
    int run;
    int pipes;
    int stages;
    int repetitions;
    int burst_size;

    po::options_description desc("MSG Flow Graph");
    desc.add_options()
        ("help,h", "display help")
        ("run,r", po::value<int>(&run)->default_value(1), "Run Number")
        ("pipes,p", po::value<int>(&pipes)->default_value(5), "Number of pipes")
        ("stages,s", po::value<int>(&stages)->default_value(6), "Number of stages")
        ("repetitions,R", po::value<int>(&repetitions)->default_value(100), "Number of repetitions")
        ("burst_size,b", po::value<int>(&burst_size)->default_value(0), "Number of PDUs per burst");

    po::variables_map vm;
    po::store(po::parse_command_line(argc, argv, desc), vm);
    po::notify(vm);

    if (vm.count("help")) {
        std::cout << desc << std::endl;
        return 0;
    }

    msg_flowgraph* runner = new msg_flowgraph(pipes, stages);

    for(int repetition = 0; repetition < repetitions; repetition++) {
        // enqueue messages
        for (auto s : runner->d_srcs) {
            for (int p = 0; p < burst_size; p++) {
                pmt::pmt_t msg = pmt::from_double(1.23);
                s->post(pmt::mp("in"), msg);
            }

            // enqueue done message to terminate
            pmt::pmt_t msg = pmt::cons(pmt::intern("done"), pmt::from_long(1));
            s->post(pmt::mp("system"), msg);
        }

        auto start = std::chrono::high_resolution_clock::now();
        runner->tb->run();
        auto finish = std::chrono::high_resolution_clock::now();
        auto time = std::chrono::duration_cast<std::chrono::nanoseconds>(finish-start).count()/1e9;

        std::cout <<
            boost::format("%1$4d, %2$4d,  %3$4d,   %4$4d,       %5$4d,       %6$20.12f") %
                           run    % pipes % stages % repetition % burst_size % time << std::endl;
    }

    return 0;
}
