#include <gnuradio/blocks/message_debug.h>
#include <gnuradio/blocks/null_sink.h>
#include <gnuradio/blocks/vector_source.h>
#include <gnuradio/lora_sdr/crc_verif.h>
#include <gnuradio/lora_sdr/deinterleaver.h>
#include <gnuradio/lora_sdr/dewhitening.h>
#include <gnuradio/lora_sdr/fft_demod.h>
#include <gnuradio/lora_sdr/frame_sync.h>
#include <gnuradio/lora_sdr/gray_mapping.h>
#include <gnuradio/lora_sdr/hamming_dec.h>
#include <gnuradio/lora_sdr/header_decoder.h>
#include <gnuradio/top_block.h>

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <fcntl.h>
#include <fstream>
#include <iostream>
#include <stdexcept>
#include <string>
#include <string_view>
#include <unistd.h>
#include <utility>
#include <vector>

namespace {

constexpr std::size_t BRANCHES = 4;
constexpr std::uint8_t SF = 7;
constexpr std::uint32_t SAMPLE_RATE = 1'000'000;
constexpr std::uint32_t BANDWIDTH = 125'000;
constexpr std::uint8_t OVERSAMPLING = SAMPLE_RATE / BANDWIDTH;
constexpr std::uint32_t CENTER_FREQUENCY = 869'525'000;
constexpr std::uint16_t PREAMBLE_LENGTH = 8;
constexpr char DEFAULT_FILE[] =
    "lora-dumps/samples_sf7_pad16_snr30_16B_300s.cf32";

struct Options {
    std::size_t run = 0;
    std::string file = DEFAULT_FILE;
};

class StdoutSilencer {
public:
    StdoutSilencer()
    {
        std::fflush(stdout);
        saved_stdout_ = dup(STDOUT_FILENO);
        const int null_fd = open("/dev/null", O_WRONLY);
        if (saved_stdout_ == -1 || null_fd == -1 || dup2(null_fd, STDOUT_FILENO) == -1) {
            if (null_fd != -1) {
                close(null_fd);
            }
            throw std::runtime_error("could not redirect stdout");
        }
        close(null_fd);
    }

    ~StdoutSilencer()
    {
        std::fflush(stdout);
        if (saved_stdout_ != -1) {
            dup2(saved_stdout_, STDOUT_FILENO);
            close(saved_stdout_);
        }
    }

    StdoutSilencer(const StdoutSilencer&) = delete;
    StdoutSilencer& operator=(const StdoutSilencer&) = delete;

private:
    int saved_stdout_ = -1;
};

struct Flowgraph {
    gr::top_block_sptr top_block;
    std::vector<gr::blocks::message_debug::sptr> message_stores;
};

std::string option_value(int& index, int argc, char** argv, std::string_view name)
{
    const std::string_view argument(argv[index]);
    const std::string prefix = std::string(name) + "=";
    if (argument.starts_with(prefix)) {
        return std::string(argument.substr(prefix.size()));
    }
    if (argument == name && index + 1 < argc) {
        return argv[++index];
    }
    return {};
}

Options parse_options(int argc, char** argv)
{
    Options options;
    for (int index = 1; index < argc; ++index) {
        if (const auto value = option_value(index, argc, argv, "--run"); !value.empty()) {
            options.run = std::stoul(value);
        } else if (const auto value = option_value(index, argc, argv, "--file");
                   !value.empty()) {
            options.file = value;
        } else if (std::string_view(argv[index]) == "-f" && index + 1 < argc) {
            options.file = argv[++index];
        } else {
            throw std::runtime_error("unknown or incomplete argument: " +
                                     std::string(argv[index]));
        }
    }
    return options;
}

std::vector<gr_complex> load_cf32(const std::string& path)
{
    static_assert(sizeof(gr_complex) == 8);

    std::ifstream input(path, std::ios::binary | std::ios::ate);
    if (!input) {
        throw std::runtime_error("could not open IQ dump: " + path);
    }
    const auto size = input.tellg();
    if (size < 0 || size % static_cast<std::streamoff>(sizeof(gr_complex)) != 0) {
        throw std::runtime_error("invalid CF32 IQ dump size: " + path);
    }

    std::vector<gr_complex> samples(static_cast<std::size_t>(size) / sizeof(gr_complex));
    input.seekg(0);
    input.read(reinterpret_cast<char*>(samples.data()), size);
    if (!input) {
        throw std::runtime_error("could not read complete IQ dump: " + path);
    }
    return samples;
}

Flowgraph build_flowgraph(const std::vector<gr_complex>& samples)
{
    auto top_block = gr::make_top_block("LoRa RX");
    std::vector<gr::blocks::message_debug::sptr> message_stores;
    message_stores.reserve(BRANCHES);

    for (std::size_t branch = 0; branch < BRANCHES; ++branch) {
        auto source = gr::blocks::vector_source_c::make(samples, false);
        auto frame_sync = gr::lora_sdr::frame_sync::make(CENTER_FREQUENCY,
                                                         BANDWIDTH,
                                                         SF,
                                                         false,
                                                         { 0x34 },
                                                         OVERSAMPLING,
                                                         PREAMBLE_LENGTH);
        auto fft_demod = gr::lora_sdr::fft_demod::make(false, true);
        auto gray_mapping = gr::lora_sdr::gray_mapping::make(false);
        auto deinterleaver = gr::lora_sdr::deinterleaver::make(false);
        auto hamming_dec = gr::lora_sdr::hamming_dec::make(false);
        auto header_decoder =
            gr::lora_sdr::header_decoder::make(false, 1, 11, true, 0, false);
        auto dewhitening = gr::lora_sdr::dewhitening::make();
        auto crc_verif = gr::lora_sdr::crc_verif::make(0, false);
        auto crc_ok_sink = gr::blocks::null_sink::make(sizeof(std::uint8_t));
        auto crc_bad_sink = gr::blocks::null_sink::make(sizeof(std::uint8_t));
        auto message_store = gr::blocks::message_debug::make();

        top_block->msg_connect(header_decoder, "frame_info", frame_sync, "frame_info");
        top_block->msg_connect(crc_verif, "msg", message_store, "store");
        top_block->connect(source, 0, frame_sync, 0);
        top_block->connect(frame_sync, 0, fft_demod, 0);
        top_block->connect(fft_demod, 0, gray_mapping, 0);
        top_block->connect(gray_mapping, 0, deinterleaver, 0);
        top_block->connect(deinterleaver, 0, hamming_dec, 0);
        top_block->connect(hamming_dec, 0, header_decoder, 0);
        top_block->connect(header_decoder, 0, dewhitening, 0);
        top_block->connect(dewhitening, 0, crc_verif, 0);
        top_block->connect(crc_verif, 0, crc_ok_sink, 0);
        top_block->connect(crc_verif, 1, crc_bad_sink, 0);
        message_stores.push_back(std::move(message_store));
    }

    return { std::move(top_block), std::move(message_stores) };
}

} // namespace

int main(int argc, char** argv)
{
    try {
        const auto options = parse_options(argc, argv);
        auto samples = load_cf32(options.file);
        Flowgraph flowgraph;
        double elapsed_seconds;

        {
            StdoutSilencer silence;
            flowgraph = build_flowgraph(samples);
            std::vector<gr_complex>().swap(samples);

            const auto start = std::chrono::steady_clock::now();
            flowgraph.top_block->start();
            flowgraph.top_block->wait();
            elapsed_seconds =
                std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
        }

        std::cout << options.run << ',' << options.file << ",legacy," << elapsed_seconds;
        for (const auto& message_store : flowgraph.message_stores) {
            std::cout << ',' << message_store->num_messages();
        }
        std::cout << '\n';
    } catch (const std::exception& error) {
        std::cerr << "gnuradio-rx: " << error.what() << '\n';
        return EXIT_FAILURE;
    }
    return EXIT_SUCCESS;
}
