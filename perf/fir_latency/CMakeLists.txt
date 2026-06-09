project(FirLatencyFlowgraph)

cmake_minimum_required(VERSION 3.8)

find_package(Gnuradio "3.9" REQUIRED COMPONENTS blocks filter fft)
find_package(gnuradio-sched)
find_package(Boost 1.40 COMPONENTS program_options REQUIRED)

add_executable(fir_latency_flowgraph fir_latency_flowgraph.cpp)

target_link_libraries(fir_latency_flowgraph
  gnuradio::gnuradio-runtime
  gnuradio::gnuradio-pmt
  gnuradio::gnuradio-blocks
  gnuradio::gnuradio-fft
  gnuradio::gnuradio-filter
  gnuradio::gnuradio-sched
  ${Boost_LIBRARIES}
)
