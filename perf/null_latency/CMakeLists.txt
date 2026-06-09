cmake_minimum_required(VERSION 3.8)

project(NullLatencyFlowgraph)

find_package(Gnuradio "3.9" REQUIRED COMPONENTS blocks)
find_package(gnuradio-sched)
find_package(Boost 1.40 COMPONENTS program_options REQUIRED)
include_directories(${Boost_INCLUDE_DIR})

add_executable(null_latency_flowgraph null_latency_flowgraph.cpp)

target_link_libraries(null_latency_flowgraph
  gnuradio::gnuradio-runtime
  gnuradio::gnuradio-pmt
  gnuradio::gnuradio-blocks
  gnuradio::gnuradio-sched
  ${Boost_LIBRARIES}
)
