cmake_minimum_required(VERSION 3.20)
project(aad-winui LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Path to aadt_ffi.dll for POST_BUILD copy (override: -DAADT_FFI_DIR=...)
if(NOT AADT_FFI_DIR)
  if(DEFINED ENV{CARGO_TARGET_DIR})
    set(AADT_FFI_DIR "$ENV{CARGO_TARGET_DIR}/release")
  else()
    set(AADT_FFI_DIR "${CMAKE_SOURCE_DIR}/../../target/release")
  endif()
endif()

add_executable(aad-winui WIN32
  src/main.cpp
  src/FfiSession.cpp
  src/Direct2DCanvas.cpp
)

target_include_directories(aad-winui PRIVATE include)
target_compile_definitions(aad-winui PRIVATE
  UNICODE _UNICODE WIN32_LEAN_AND_MEAN AADT_FFI_DYNAMIC
)

# DLL is loaded at runtime via LoadLibrary — no import lib required.
target_link_libraries(aad-winui PRIVATE d2d1 user32 gdi32 comctl32 ole32)

if(MSVC)
  target_compile_options(aad-winui PRIVATE /W4 /EHsc /permissive-)
else()
  target_compile_options(aad-winui PRIVATE -Wall -Wextra)
  # MinGW: UNICODE entry is wWinMain — need -municode with WIN32 subsystem.
  target_link_options(aad-winui PRIVATE -municode -mwindows)
endif()

add_custom_command(TARGET aad-winui POST_BUILD
  COMMAND ${CMAKE_COMMAND} -E copy_if_different
    "${AADT_FFI_DIR}/aadt_ffi.dll"
    "$<TARGET_FILE_DIR:aad-winui>/aadt_ffi.dll"
  COMMENT "Copy aadt_ffi.dll beside aad-winui.exe"
)
