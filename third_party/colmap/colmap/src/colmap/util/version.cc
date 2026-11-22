#include "colmap/util/version.h"
namespace colmap {
std::string GetVersionInfo() { return "COLMAP 4.3.0-sparse-opengl"; }
std::string GetBuildInfo() { return "Sparse+OpenGL (headless)"; }
}  // namespace colmap
