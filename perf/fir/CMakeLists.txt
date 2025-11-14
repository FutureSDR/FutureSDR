project(FirRandFlowgraph)

cmake_minimum_required(VERSION 3.8)

find_package(Gnuradio "3.9" REQUIRED COMPONENTS blocks filter fft)
find_package(gnuradio-sched)
find_package(Boost 1.40 COMPONENTS program_options REQUIRED)
include_directories(${Boost_INCLUDE_DIR})

add_executable(fir_rand_flowgraph fir_rand_flowgraph.cpp)

target_link_libraries(fir_rand_flowgraph
  gnuradio::gnuradio-runtime
  gnuradio::gnuradio-pmt
  gnuradio::gnuradio-blocks
  gnuradio::gnuradio-fft
  gnuradio::gnuradio-filter
  gnuradio::gnuradio-sched
  ${Boost_LIBRARIES}
)
