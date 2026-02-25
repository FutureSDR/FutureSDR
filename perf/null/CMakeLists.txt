project(NullRandFlowgraph)

cmake_minimum_required(VERSION 3.8)

find_package(Gnuradio "3.10" REQUIRED COMPONENTS blocks)
find_package(Boost 1.40 COMPONENTS program_options REQUIRED)
include_directories(${Boost_INCLUDE_DIR})

add_executable(null_flowgraph null_flowgraph.cpp)

target_link_libraries(null_flowgraph
  gnuradio::gnuradio-runtime
  gnuradio::gnuradio-pmt
  gnuradio::gnuradio-blocks
  ${Boost_LIBRARIES}
)
