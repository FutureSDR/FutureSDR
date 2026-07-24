cmake_minimum_required(VERSION 3.8)

project(LoRaRx)

find_package(Gnuradio "3.10" REQUIRED COMPONENTS blocks)
find_path(LORA_SDR_INCLUDE_DIR NAMES gnuradio/lora_sdr/crc_verif.h REQUIRED)
find_library(LORA_SDR_LIBRARY NAMES gnuradio-lora_sdr REQUIRED)

add_executable(gnuradio-rx gnuradio-rx.cc)

set_target_properties(gnuradio-rx PROPERTIES
  CXX_STANDARD 20
  CXX_STANDARD_REQUIRED ON
  CXX_EXTENSIONS OFF
)

target_include_directories(gnuradio-rx PRIVATE ${LORA_SDR_INCLUDE_DIR})

target_link_libraries(gnuradio-rx
  gnuradio::gnuradio-runtime
  gnuradio::gnuradio-pmt
  gnuradio::gnuradio-blocks
  ${LORA_SDR_LIBRARY}
)
