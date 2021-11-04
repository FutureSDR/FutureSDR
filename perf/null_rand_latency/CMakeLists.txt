project(NullRandFlowgraph)

cmake_minimum_required(VERSION 3.8)

include_directories(.)

include (FindLTTngUST REQUIRED)

add_library(tracepoint-provider SHARED tp.cpp)
target_link_libraries(tracepoint-provider LTTng::UST)

find_package(Gnuradio "3.9" REQUIRED COMPONENTS blocks)
find_package(gnuradio-sched)

add_executable(null_rand_flowgraph null_rand_flowgraph.cpp)

target_link_libraries(null_rand_flowgraph
  gnuradio::gnuradio-runtime
  gnuradio::gnuradio-pmt
  gnuradio::gnuradio-blocks
  gnuradio::gnuradio-sched
  tracepoint-provider
)
